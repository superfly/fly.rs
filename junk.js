console.log("I am junk.");

// fetch("http://example.com", { method: "HEAD" }).then(res => {
//   console.log("in fetch res!", res)
//   return res.text()
// }).then(body => { console.log("body:", body) }).finally(() => { console.log("finally") })

setTimeout(() => { console.log("in timeout") }, 500)

var array = new Uint8Array(10);
window.crypto.getRandomValues(array);

console.log("rand values:", array);

// const coll = fly.data.collection("hello")
// coll.put("woot", `{"foo":"bar"}`).then(() => {
//   return coll.get("woot")
// }).then((i) => console.log("got data:", i))

// resolv("example.com")

