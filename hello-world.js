// Requires the secrets module loader, not enabled everywhere yet.
// import someSecret from "secrets:///secretObject/subSecret";
// console.log(someSecret);

console.log("hello world")
const helloWorldStr = "Hello World";
const helloWorld = new TextEncoder().encode(helloWorldStr);

addEventListener("fetch", function (event) {
  const req = event.request;
  // console.log("req url:", event.request.url);
  let url = new URL(req.url)
  if (url.pathname == "/echo")
    event.respondWith(new Response(req.body, { headers: { foo: "bar" } }))

  else if (url.pathname == "/null")
    event.respondWith(new Response(null, { headers: {} }))

  else if (url.pathname == "/hello-world")
    event.respondWith(new Response(helloWorld))

  else if (url.pathname == "/kitchensink") {
    const coll = fly.data.collection("testing")
    coll.put("id", { foo: "bar", counter: 10 }).then(b => {
      console.log("put returned:", b);
      coll.get("id").then(d => {
        console.log("get returned:", d)
        coll.increment("id", "counter", 3).then(b => {
          coll.get("id").then(d => {
            console.log("get after incr returned:", d)
            coll.del("id").then(b => {
              console.log("del returned:", b)
              coll.get("id").then(d => {
                console.log("get returned:", d);
              }).catch(console.log)
            }).catch(console.log)
          }).catch(console.log)
        }).catch(console.log)
      }).catch(console.log)
    }).catch(console.log)

    console.log(new TextDecoder().decode(new Uint8Array([104, 101, 108, 108, 111])))

    console.trace("this is a trace message")
    console.debug("this is a debug message")
    console.info("this is a info message")
    console.warn("this is a warn message")
    console.error("this is a error message")
    console.log("this is a log message")

    let now = Date.now();
    setTimeout(() => { console.log("in timeout!", Date.now() - now); now = Date.now() }, 100)

    let arr = new Uint8Array(32);
    crypto.getRandomValues(arr);
    console.log("some random values:", arr);

    fetch("file://README.md").then(res => {
      console.log("file res:", res);
      res.text().then(p => { console.log("p", p) })
    }).catch(err => { console.log("err fetching file://:", err.stack) })

    resolv("fly.io").then(res => {
      console.log("got dns res:", res)
    }).catch(err => { console.log("error resolving I guess:", err.stack) })
    event.respondWith(new Response(null, { headers: {} }))
  }
  else if (url.pathname == "/image") {
    event.respondWith(fetch(url.searchParams.get("url")).then(res => {
      let img = new fly.Image(res.body);
      img.resize({ width: 512, height: 512 }).webp({ lossless: false, quality: 75 });
      return img.transform().then(stream => {
        console.log("image accepted!");
        return new Response(stream, {
          headers: {
            "content-type": "image/webp",
          }
        })
      }).catch(e => console.log("error processing image:", e))
    }))
  } else if (url.pathname == "/post") {
    console.log("post")
    // event.respondWith(fetch("http://localhost:8888/post-echo", { body: "from within", method: "POST" }))
    event.respondWith(fetch("http://localhost:8888/post-echo", { body: event.request.body, method: "POST" }))
  } else if (req.method == "POST" && url.pathname == "/post-echo") {
    console.log("post-echo")
    event.respondWith(new Response(event.request.body))
  } else {
    req.headers.delete("host");
    let u = url.searchParams.get("url");
    let toFetch = new Request(req)
    toFetch.url = u;

    if (url.searchParams.get("cache")) {
      return event.respondWith(cache.match(toFetch).then(res => {
        if (res)
          return res

        return fetch(toFetch).then(res => {
          try {
            cache.put(toFetch, res.clone())
            return res
          } catch (e) {
            console.log(e.message, e.stack)
            return new Response(null)
          }
        })
      }))
    }
    event.respondWith(fetch(toFetch))
  }
})

addEventListener("resolv", event => {
  console.log("got resolv event!")
  // event.respondWith(resolv(event.request.name, { nameservers: ["ns2.fly.io"] }))
  event.respondWith(function () {
    return new DNSResponse([
      {
        name: event.request.name,
        type: DNSRecordType.TXT,
        ttl: 5,
        data: { data: [new TextEncoder().encode("helloworld"), new TextEncoder().encode("helloworld2")] }
      }
    ], { authoritative: true })
  })
})