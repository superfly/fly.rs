async function buildFn(input) {
  console.log("BUILDING", input)
  try {
    const bundle = await rollup.rollup({
      input: input,
      plugins: [
        // rollupCommonJS(),
        {
          resolveId: (importee, importer) => {
            console.log("resolve importee:", importee, "importer:", importer);
            return importee
          },
          load: id => {
            console.log("load id:", id);
            return fetch(`file://${id}`).then(res => {
              console.log("got a fetch response!", res);
              return res.text()
            }).catch(e => {
              console.log("error fetching file:", e.stack)
            })
          },
        }
      ]
    })
    const generated = await bundle.generate({
      format: "iife"
    })
    console.log("DONE", generated.code);
    return generated.code
  } catch (e) {
    console.log("error", e.stack)
  }
}