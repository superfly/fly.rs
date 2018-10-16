import { globalEval } from './global-eval'
import * as rollup from "rollup";
import { sendSync, sendAsync } from "./bridge";
import * as fbs from "./msg_generated";
import { flatbuffers } from "flatbuffers";
import { assert } from "./util";
import typescript from "rollup-plugin-typescript";
import json from "rollup-plugin-json"
// import * as path from "path"

console.log("evaluating build.js!")

function run(module: string, containingFile: string) {
  console.log("run", {module, containingFile})
  runInternal(module).then(x => console.log("done" ,x)).catch(e => console.error(e))
}

const window = globalEval("this");
window.run = run

type ModuleSpecifier = string
type ModuleId = string
type ContainingFile = string


const flyResolver = {
  name: "fly-resolve",
  resolveId: (id: ModuleSpecifier, parent?: ContainingFile): Promise<string | boolean | void | null> | string | boolean | void | null => {
    console.log("resolveId", id, parent)
    const resolvedId = resolveModule(id, parent)
    console.log("resolved", resolvedId)

    return resolvedId
  },
  load: (id: string): Promise< string | undefined> => {
    console.log("load", id)
    return loadModule(id)
    // if (id === "test-js/simple.js") {
    //   return `console.log("HELLO!")`
    // }
    // if (id === "test-js/a.js") {
    //   return 
    // }
    // return null;
  }
}

export async function runInternal(input: string | string[]) {
  if (typeof input === "string") {
    input = [input]
  }
  const bundle = await rollup.rollup({
    input: input,
    experimentalCodeSplitting: true,
    treeshake: false,
    plugins: [
      json(),
      flyResolver,
      typescript(),
    ]
  });

  const generated = await bundle.generate({
    format: "iife",
    // format: "amd",
  })

  const output = generated.output

  for (const filename in output) {
    const chunk = output[filename]
    console.log("run chunk: ", { filename, chunk })

    if (typeof chunk === "string") {
      console.log("-> is string")
      globalEval(chunk)
    } else if ("code" in chunk) {
      console.log("-> is chunk")
      globalEval(chunk.code)
    }
  }
}

async function resolveModule(id: ModuleId, containingFile?: ContainingFile): Promise<string | undefined> {
  console.log("resolveModule", { id, containingFile })
  
  if (id === "./fullName") {
    return "./test-js/fullName.js"
  } else if (id === "./greet") {
    return "./test-js/greet.ts"
  }

  return id

  // if (id === "simple.js") {
  //   return "simple.js"
  // }
  // // if (id === "test-js/a.js") {
  // //   retrn 
  // // }

  // if (isAbsolute(id)) {
  //   // module is an absolute path, pass through
  //   return id
  // }

  // if (id.startsWith(".")) {
  //   // module is a relative reference, resolve
  //   // let filename = null;
  //   // if (containingFile) {
  //   //   const parentDir = path.dirname(containingFile);
  //   //   filename = path.resolve(parentDir, id)
  //   // } else {
  //   //   filename = path.resolve(process.cwd(), id)
  //   // }

  //   // if (fs.existsSync(filename)) {
  //   //   return filename
  //   // }
  //   // // no extension specified, try ts, js, etc
  //   // if (path.extname(filename) === "") {
  //   //   for (const ext of ["ts", "js"]) {
  //   //     const filenameWithExt = `${filename}.${ext}`;
  //   //     if (fs.existsSync(filenameWithExt)) {
  //   //       return filenameWithExt
  //   //     }
  //   //   }
  //   // }

  //   // no file found
  //   // console.log("no file found")
  //   return null
  // }

  // // module is external, resolve against node modules or something
  // return `external/${id}`
}

async function loadModule(id: ModuleId): Promise<string | undefined> {
  console.log("loadModule", { id })
  
  try {
    const builder = new flatbuffers.Builder();
    const id_fb = builder.createString(id);
    fbs.ResolveModuleRequest.startResolveModuleRequest(builder);
    fbs.ResolveModuleRequest.addModuleSpecifier(builder, id_fb);
    const msg = fbs.ResolveModuleRequest.endResolveModuleRequest(builder);
    const resp = sendSync(builder, fbs.Any.ResolveModuleRequest, msg);
    console.log("loadModule resp", resp);
    const respMsg = new fbs.ResolveModuleResponse();
    resp.msg(respMsg);
    const sourceCode = respMsg.sourceCode()
    console.log("got resolve resp", { sourceCode })
    return Promise.resolve(sourceCode)
  } catch (e) {
    console.error("error from rust", e)
    return Promise.reject(e)
  }
}

const absolutePathPattern = /^(?:\/|(?:[A-Za-z]:)?[\\|/])/;

function isAbsolute(modulePath: string) {
  return absolutePathPattern.test(modulePath);
}
