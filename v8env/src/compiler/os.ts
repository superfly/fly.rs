// import * as fs from "./fs"
import { files } from "./files"

// const libName = "lib.fly.d.ts";
// const lib = `declare const flyVersion: string;`;

export function readFile(path: string): string {
  console.log("os.readFile()", { path });
  // path = path.substring(2)
  // let cleanPath = path
  // if (cleanPath.startsWith("./")) {
  //   cleanPath = cleanPath.substring(2)
  // }
  // if (cleanPath.startsWith("/app/")) {
  //   cleanPath = cleanPath.substring(5)
  // }
  // cleanPath = cleanPath.replace("/./", "/")

  // if (cleanPath === libName || cleanPath === `external/${libName}`) {
  //   return lib;
  // }

  // const cleanPath = file.base

  const contents = files[path];
  if (!contents) {
    throw new Error(`File not found: ${path}`);
  }
  return contents;
  // return fs.readFileSync(path).toString();
}

export function exists(path: string): boolean {
  console.log("os.exists()", { path });
  // let cleanPath = path;
  // if (cleanPath.startsWith("./")) {
  //   cleanPath = cleanPath.substring(2)
  // }
  // if (cleanPath.startsWith("/app/")) {
  //   cleanPath = cleanPath.substring(5)
  // }
  // cleanPath = cleanPath.replace("/./", "/")
  // if (cleanPath === libName || cleanPath === `external/${libName}`) {
  //   return true;
  // }
  return files[path] != undefined;
  // return false;
  // return fs.existsSync(path);
}