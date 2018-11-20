import { assetSourceCode, ContainerName } from "./assets";
import { assert } from "./util";
import { loadModule } from "../module_loader";

interface LoadModuleResult {
  moduleId: string,
  sourceCode: string,
}

export function fetchModule(moduleSpecifier: string, containingFile: string): LoadModuleResult | null {  
  console.log("resolver.fetchModule()", { moduleSpecifier, containingFile });
  if (isAsset(moduleSpecifier, containingFile)) {
    let moduleId = moduleSpecifier.split("/").pop()!;
    const assetName = moduleId.includes(".") ? moduleId : `${moduleId}.d.ts`;
    assert(assetName in assetSourceCode, `No such asset "${assetName}"`);

    return {
      // filename: `${ContainerName}/${assetName}`,
      moduleId: `${ContainerName}/${assetName}`,
      sourceCode: assetSourceCode[assetName]
    }
  }
  
  return loadModule(moduleSpecifier, containingFile);
}

function isAsset(moduleSpecifier: string, containingFile: string): boolean {
  return moduleSpecifier.startsWith(ContainerName) ||
    containingFile.startsWith(ContainerName);
}

const absolutePathPattern = /^(?:\/|(?:[A-Za-z]:)?[\\|/])/;

function isAbsolute(modulePath: string) {
  return absolutePathPattern.test(modulePath);
}
