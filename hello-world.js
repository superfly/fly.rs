console.log("ha");

// console.log(new TextDecoder().decode(new Uint8Array([104, 101, 108, 108, 111])))

let now = Date.now();
setTimeout(() => { console.log("in timeout!", Date.now() - now); now = Date.now() }, 100)

addEventListener("fetch", function (event) {
  event.respondWith(async function () {
    return new Response("", { headers: { foo: "bar" } })
  })
})