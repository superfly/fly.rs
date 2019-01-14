import { sendSync } from './bridge';
import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers"

export interface LoadedSourceCode {
  isWasm: boolean;
  sourceMap?: string;
  source: string;
}

export interface LoadedModule {
  originUrl: string;
  loadedSource: LoadedSourceCode;
}

export function loadModule(specifierUrl: string, refererOriginUrl?: string): LoadedModule {
  if (refererOriginUrl) {
    return loadModuleStandard(specifierUrl, refererOriginUrl);
  } else {
    return loadModuleWithoutReferer(specifierUrl);
  }
}

function loadModuleWithoutReferer(specifierUrl: string): LoadedModule {
  // Allocate new message and fill it with data
  const fbb = flatbuffers.createBuilder();
  const fbSpcecifierUrl = fbb.createString(specifierUrl);
  // Fill message handle with data
  fbs.LoadModule.startLoadModule(fbb);
  fbs.LoadModule.addSpecifierUrl(fbb, fbSpcecifierUrl);
  // Send flatbuffer messaage and collect response
  const resp = sendSync(fbb, fbs.Any.LoadModule, fbs.LoadModule.endLoadModule(fbb));
  // Allocate new LoadModuleResp handle
  const msg = new fbs.LoadModuleResp();
  // Write message data to handle
  resp.msg(msg);
  // Transform data into local format and return.
  return {
    originUrl: msg.originUrl(),
    loadedSource: {
      isWasm: false,
      source: msg.sourceCode(),
    },
  };
}

function loadModuleStandard(specifierUrl: string, refererOriginUrl: string): LoadedModule {
  // Allocate new message handle
  const fbb = flatbuffers.createBuilder();
  const fbSpcecifierUrl = fbb.createString(specifierUrl);
  const fbRefererOriginUrl = fbb.createString(refererOriginUrl);
  // Fill message handle with data
  fbs.LoadModule.startLoadModule(fbb);
  fbs.LoadModule.addSpecifierUrl(fbb, fbSpcecifierUrl);
  fbs.LoadModule.addRefererOriginUrl(fbb, fbRefererOriginUrl);
  // Send flatbuffer messaage and collect response
  const resp = sendSync(fbb, fbs.Any.LoadModule, fbs.LoadModule.endLoadModule(fbb));
  // Allocate new LoadModuleResp handle
  const msg = new fbs.LoadModuleResp();
  // Write message data to handle
  resp.msg(msg);
  // Return data from handle
  return {
    originUrl: msg.originUrl(),
    loadedSource: {
      isWasm: false,
      source: msg.sourceCode(),
    },
  };
}