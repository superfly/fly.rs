// import * as fs from "fs"
import * as path from "./path"
import { readFile, exists } from "./os";
import { assetSourceCode, ContainerName } from "./assets";
import { assert } from "./util";

/**
 * Load source code for the specified module
 * @param moduleId 
 */
export function load(moduleId: string): string {
  console.log("compiler.fetchSourceCode()", { moduleId })

  const filename = path.basename(moduleId);

  if (moduleId.startsWith("/assets/")) {
    return assetSourceCode[filename];
  }

  // if (moduleId.startsWith("assets/")) {
  //   return assetSourceCode[moduleId.substring(7)];
  // }


  // if (moduleId.startsWith("/app/")) {
  //   return readFile(moduleId.substring(5))
  // }

  return readFile(filename)
}

/**
 * Resole Load source code for the specified module
 * @param moduleId 
 */
export function resolve(id: string, containingFile?: string): string | null {
  console.log("compiler.resolveModuleId()", { id, containingFile })

  if (isAbsolute(id)) {
    // module is an absolute path, pass through
    return id
  }

  // if (id.startsWith("assets/")) {
  //   return id
  // }

  if (isAsset(id, containingFile)) {
    const assetName = id.endsWith(".d.ts") ? id : `${id}.d.ts`;
    console.log("assert", {assetName, assets :Object.keys(assetSourceCode)})
    assert(assetName in assetSourceCode, `Asset ${assetName} not found`);
    return `/assets/${id.split("/").pop()}`;
  }
  
  switch (id) {
    case "./entry.ts": return "entry.ts"
    case "./fullName": return "fullName.ts"
    case "./fullName.ts": return "fullName.ts"
    case "fullName.ts": return "fullName.ts"
    case "./fullName.js": return "fullName.ts"
    case "./greet": return "greet.ts"
    case "./greet.ts": return "greet.ts"
    case "greet.ts": return "greet.ts"
    case "./greet.js": return "greet.ts"
    case "./playground.ts": return "playground.ts"
    case "playground.ts": return "playground.ts"
    case "index.ts": return "index.ts"
  }

  // return path.basename(id);

  // switch (id)

  // console.log("is NOT  asset")
  // if (id.startsWith(".")) {
  //   // module is a relative reference, resolve
  //   let filename = null;
  //   if (containingFile) {
  //     const parentDir = path.dirname(containingFile);
  //     filename = path.resolve(parentDir, id)
  //     console.log("resolve from containing file", {containingFile, parentDir, filename})
  //   } else {
  //     filename = path.resolve(".", id)
  //     console.log("resolve from DOT - no containing file", { containingFile, filename })
  //   }
    
    
  //   if (exists(filename)) {
  //     return filename
  //   }
  //   // no extension specified, try ts, js, etc
  //   if (path.extname(filename) === "") {
  //     for (const ext of ["ts", "js"]) {
  //       const filenameWithExt = `${filename}.${ext}`;
  //       if (exists(filenameWithExt)) {
  //         return filenameWithExt
  //       }
  //     }
  //   }

  //   // no file found
  //   // console.log("no file found")
  //   return null
  // }

  // // module is external, resolve against node modules or something
  // return `/external/${id}`
}

function isAsset(moduleSpecifier: string, containingFile?: string): boolean {
  if (moduleSpecifier.startsWith(ContainerName)) {
    return true;
  }

  if (containingFile && containingFile.startsWith(ContainerName)) {
    return true;
  }

  return false;
}

const absolutePathPattern = /^(?:\/|(?:[A-Za-z]:)?[\\|/])/;

function isAbsolute(modulePath: string) {
  return absolutePathPattern.test(modulePath);
}
