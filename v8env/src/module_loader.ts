import { sendSync } from './bridge';
import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers"

export interface ModuleInfo {
  moduleId: string,
  sourceCode: string,
}

export function loadModule(moduleSpecifier: string, containingFile: string): ModuleInfo {
  const fbb = flatbuffers.createBuilder();
  const fbModuleSpecifier = fbb.createString(moduleSpecifier);
  const fbContainingFile = fbb.createString(containingFile);
  fbs.LoadModule.startLoadModule(fbb);
  fbs.LoadModule.addModuleSpecifier(fbb, fbModuleSpecifier);
  fbs.LoadModule.addContainingFile(fbb, fbContainingFile);
  const resp = sendSync(fbb, fbs.Any.LoadModule, fbs.LoadModule.endLoadModule(fbb));
  const msg = new fbs.LoadModuleResp();
  resp.msg(msg);
  return {
    moduleId: msg.moduleId(),
    sourceCode: msg.sourceCode(),
  }
}