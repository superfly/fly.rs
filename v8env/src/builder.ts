import { globalEval } from './global-eval'
import * as rollup from "rollup";
// import { sendSync, sendAsync } from "./bridge";
// import * as fbs from "./msg_generated";
// import { flatbuffers } from "flatbuffers";
// import { assert } from "./util";

// import * as System from "@types/systemjs"
import { loader } from "./compiler/loader"
import { resolveModule, loadModule, ModuleSpecifier, ContainingFile } from "./compiler/resolver"

import typescript from "./compiler/typescript-plugin"

console.log("evaluating build.js!")


function run(module: string, containingFile: string) {
  console.log("run", {module, containingFile})
  runInternal(module).then(x => console.log("done" ,x)).catch(e => console.error(e))
}

const process = {
  cwd() {
    return "/"
  }
}

const window = globalEval("this");
window.run = run
window.run2 = run




const flyResolver = {
  name: "fly-resolve",
  resolveId: (id: ModuleSpecifier, parent?: ContainingFile): Promise<string | boolean | void | null> | string | boolean | void | null => {
    console.log("resolveId", id, parent)
    if (id === "@fly/proxy") {
      return "./v8env/src/fly/proxy.ts"
    }

    const resolvedId = resolveModule(id, parent)
    console.log("resolved", resolvedId)

    return resolvedId
  },
  load: (id: string): Promise< string | undefined> => {
    console.log("load", id)
    return loadModule(id)
  }
}

async function runInternal(input: string | string[]) {
  if (typeof input === "string") {
    input = [input]
  }


  try {
    const bundle = await rollup.rollup({
      input: input,
      experimentalCodeSplitting: true,
      treeshake: false,
      plugins: [
        flyResolver,
        typescript(),
        loader({}) as rollup.Plugin,
      ]
    });

    const generated = await bundle.generate({
      format: "amd",
    })

    for (const filename in generated.output) {
      console.log("filename in chunk", { filename })
      const chunk = generated.output[filename]
      console.log("run chunk: ", { filename, chunk })

      if (typeof chunk === "string") {
        console.log("-> is string")
        globalEval(chunk)
      } else if ("code" in chunk) {
        console.log("-> is chunk")
        globalEval(chunk.code)
      }
    }
  } catch (error) {
    console.error("build error", error.toString())
  } 
}

