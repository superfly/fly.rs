import { sendSync } from './bridge';
import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers"

export interface ModuleInfo {
  moduleId: string;
  fileName: string;
  sourceCode: string;
}

export function loadModule(moduleSpecifier: string, containingFile: string): ModuleInfo {
  // Allocate new message handle
  const fbb = flatbuffers.createBuilder();
  const fbModuleSpecifier = fbb.createString(moduleSpecifier);
  const fbContainingFile = fbb.createString(containingFile);
  // Fill message handle with data
  fbs.LoadModule.startLoadModule(fbb);
  fbs.LoadModule.addModuleSpecifier(fbb, fbModuleSpecifier);
  fbs.LoadModule.addContainingFile(fbb, fbContainingFile);
  // Send flatbuffer messaage and collect response
  const resp = sendSync(fbb, fbs.Any.LoadModule, fbs.LoadModule.endLoadModule(fbb));
  // Allocate new LoadModuleResp handle
  const msg = new fbs.LoadModuleResp();
  // Write message data to handle
  resp.msg(msg);
  // Return data from handle
  return {
    moduleId: msg.moduleId(),
    fileName: msg.fileName(),
    sourceCode: msg.sourceCode(),
  }
}