import { assetSourceCode, ContainerName } from "./assets";
import { assert } from "./util";
import { loadModule } from "../module_loader";

interface LoadModuleResult {
  moduleId: string,
  fileName: string,
  sourceCode: string,
}

export function fetchModule(moduleSpecifier: string, containingFile: string): LoadModuleResult {  
  console.trace("[resolver] fetchModule()", { moduleSpecifier, containingFile });
  // If module is a "asset" I.E. lib.dom.d.ts
  if (isAsset(moduleSpecifier, containingFile)) {
    // Remove the path from the specifier
    let moduleId = moduleSpecifier.split("/").pop()!;
    /**
     * Not completely sure of the reason for this other than maybe if specifier is just "lib" every other library has at least one "." before the .d.ts.
     * "lim.dom" would not become "lib.dom.d.ts" this might be a bug.
     */ 
    const assetName = moduleId.includes(".") ? moduleId : `${moduleId}.d.ts`;
    // Check for asset in assetSourceCode object if not error
    assert(assetName in assetSourceCode, `No such asset "${assetName}"`);

    // Return LoadModuleResult with asset source code
    return {
      moduleId: `${ContainerName}/${assetName}`,
      fileName: `${ContainerName}/${assetName}`,
      sourceCode: assetSourceCode[assetName]
    }
  }

  // Use std loadModule function to load module
  return loadModule(moduleSpecifier, containingFile);
}

function isAsset(moduleSpecifier: string, containingFile: string): boolean {
  // True if specifier of file name contains `ContainerName`
  return moduleSpecifier.startsWith(ContainerName) ||
    containingFile.startsWith(ContainerName);
}

