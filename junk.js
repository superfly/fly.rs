// console.log("I am junk.");

// fetch("http://example.com", { method: "GET" })
//   .then(res => {
//     console.log("in fetch res!", res)
//     return res.text()
//   })
//   .then(body => { console.log("body:", body) })
//   .finally(() => { console.log("finally") })

// setTimeout(() => { console.log("Hello world!") }, 100)

// var array = new Uint8Array(10);
// window.crypto.getRandomValues(array);
// console.log("rand values:", array);

// fly.cache.set("hello", "world").then(() => {
//   return fly.cache.getString("hello")
// }).then((res) => {
//   console.log("cache res:", res)
// });

// const coll = fly.data.collection("hello")
// coll.put("woot", `{"foo":"bar"}`)
//   .then(() => {
//     console.log("done with PUT");
//     coll.get("woot").then((d) => console.log(d))
//   })

// resolv("example.com")

fetch("file://build.rs").then((res) => {
  return res.text()
}).then((t) => {
  console.log("file:", t)
})

fetch("file://.").then((res) => {
  return res.text()
}).then((t) => {
  console.log("file:", t)
})