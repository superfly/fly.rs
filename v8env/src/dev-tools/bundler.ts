// import * as rollup from "rollup";
// import { resolveModule, transform } from "./compiler"

// declare namespace rollup {
//   var rollup: any;
//   type TransformSourceDescription = any;
//   type OutputChunk = any;
// }

// const flyResolver = {
//   name: "fly-resolve",
//   resolveId: (id: string, parent?: string): Promise<string | boolean | void | null> | string | boolean | void | null => {
//     console.log("builder.resolveId()", { id, parent })
//     const moduleInfo = resolveModule(id, parent || "")
//     return moduleInfo.fileName
//   },
//   load: (id: string): string | undefined => {
//     console.log("bundler.load()", { id })
//     return ""
//   },
//   transform(code: string, id: string): Promise<rollup.TransformSourceDescription | string | void> | rollup.TransformSourceDescription | string | void {
//     console.log("bundler.transform()", { code, id })
//     return {
//       code: transform(id)
//     }
//   }
// }

// export async function generateBundle(input: string): Promise<rollup.OutputChunk> {
//   try {
//     const bundle = await rollup.rollup({
//       input: input,
//       plugins: [
//         flyResolver
//       ]
//     });

//     const generated = await bundle.generate({
//       format: "iife",
//     })

//     return generated
//   } catch (err) {
//     console.error("bundle failed", err)
//   }
// }


