// import { statSync } from 'fs';
import { resolveModule } from "../resolver"

export default {
  directoryExists(dirPath) {
    console.log("resolveHost.directoryExists", {dirPath})
    return false
    // try {
    //   return statSync(dirPath).isDirectory();
    // } catch (err) {
    //   return false;
    // }
  },
  fileExists(filePath) {
    console.log("resolveHost.fileExists", { filePath })
    // try {
    //   return statSync(filePath).isFile();
    // } catch (err) {
    //   return false;
    // }
  },
  // read
};


// async function exists(path: string): Promise<boolean> {
//   console.log("exists", { path, uri: `file://${path}` })
//   try {
//     // should this error or 404?
//     const resp = await fetch(`file://${path}`)
//     console.log("exists resp", { ok: resp.ok })
//     return resp.ok;
//   } catch (error) {
//     console.warn(`doesn't exist`, { path })
//     return false
//   }
// }