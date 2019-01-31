import { CallSite } from "./types";
import { sendSync } from "./bridge";
import * as fbs from './msg_generated';
import * as flatbuffers from './flatbuffers';

export function install() {
  Error.prepareStackTrace = prepareStackTraceWrapper
}

// @internal
export function prepareStackTraceWrapper(
  error: Error,
  stack: CallSite[]
): string {
  try {
    return prepareStackTrace(error, stack);
  } catch (prepareStackError) {
    Error.prepareStackTrace = undefined;
    console.log("=====Error inside of prepareStackTrace====");
    console.log(prepareStackError.stack.toString());
    console.log("=====Original error=======================");
    throw error;
  }
}

// @internal
export function prepareStackTrace(error: Error, stack: CallSite[]): string {
  const fbb = flatbuffers.createBuilder();
  const offsets: number[] = stack.map((frame: CallSite) => {
    const filename = fbb.createString(frame.getFileName() || "<unknown>");
    const fnName = fbb.createString(frame.getFunctionName() || "");
    fbs.Frame.startFrame(fbb);
    fbs.Frame.addCol(fbb, frame.getColumnNumber());
    fbs.Frame.addLine(fbb, frame.getLineNumber());
    fbs.Frame.addFilename(fbb, filename);
    fbs.Frame.addName(fbb, fnName);
    return fbs.Frame.endFrame(fbb);
  })

  const framesOffset = fbs.SourceMap.createFramesVector(fbb, offsets);

  fbs.SourceMap.startSourceMap(fbb);
  fbs.SourceMap.addFrames(fbb, framesOffset);

  const baseRes = sendSync(fbb, fbs.Any.SourceMap, fbs.SourceMap.endSourceMap(fbb));
  const msg = new fbs.SourceMapReady();
  baseRes.msg(msg);

  const frames: string[] = Array(msg.framesLength());
  for (let i = 0; i < msg.framesLength(); i++) {
    const frame = msg.frames(i);
    frames[i] = `\n    at ${frame.name()} (${frame.filename()}:${frame.line()}:${frame.col()})`;
  }
  return error.toString() + frames.join("");
}

const v8envFilter = /v8env/;

/**
 * Remove non-app frames from a stack trace
 */
export function filterStackTrace(stackTrace: string) {
  return stackTrace.split("\n").filter(l => !v8envFilter.test(l)).join("\n").trim();
}