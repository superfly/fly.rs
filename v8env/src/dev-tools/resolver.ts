import { assetSourceCode, ContainerName, AssetsProtocol } from "./assets";
import { assert } from "./util";
import { loadModule, LoadedModule } from "../module_loader";
import { URL } from "../url";

export function fetchModule(specifierUrl: string, refererOriginUrl?: string): LoadedModule {  
  console.trace("[resolver] fetchModule()", { specifierUrl, refererOriginUrl });
  // If module is a "asset" I.E. lib.dom.d.ts
  if (isAsset(specifierUrl, refererOriginUrl)) {
    const parsedUrl = new URL(specifierUrl, refererOriginUrl);
    // Remove the path from the specifier
    let moduleFileName = parsedUrl.pathname.split("/").pop()!;
    /**
     * Not completely sure of the reason for this other than maybe if specifier is just "lib" every other library has at least one "." before the .d.ts.
     * "lim.dom" would not become "lib.dom.d.ts" this might be a bug.
     */ 
    const assetName = moduleFileName.includes(".") ? moduleFileName : `${moduleFileName}.d.ts`;
    // Check for asset in assetSourceCode object if not error
    assert(assetName in assetSourceCode, `No such asset "${assetName}"`);

    console.trace(`Finished asset module fetch ${parsedUrl.toString()}`);

    // Return LoadModuleResult with asset source code
    return {
      originUrl: parsedUrl.toString(),
      loadedSource: {
        isWasm: false,
        source: assetSourceCode[assetName],
      },
    };
  }

  console.trace(`Finished module fetch ${specifierUrl} from ${refererOriginUrl}`);
  // Use std loadModule function to load module
  return loadModule(specifierUrl, refererOriginUrl);
}

function isAsset(specifierUrl: string, refererOriginUrl: string): boolean {
  const parsedUrl = new URL(specifierUrl, refererOriginUrl); // << This might need a little more testing I'm not sure how this will handle some relative specifiers.
  return (parsedUrl.protocol === (AssetsProtocol + ":"));
}

