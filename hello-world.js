const helloWorldStr = "Hello World";
const helloWorld = new TextEncoder().encode(helloWorldStr);

addEventListener("fetch", function (event) {
  const req = event.request;
  // console.log("req url:", event.request.url);
  let url = new URL(req.url)
  if (url.pathname.endsWith("echo"))
    event.respondWith(new Response(req.body, { headers: { foo: "bar" } }))

  else if (url.pathname.endsWith("null"))
    event.respondWith(new Response(null, { headers: {} }))

  else if (url.pathname.endsWith("hello-world"))
    event.respondWith(new Response(helloWorld))

  else if (url.pathname == "/kitchensink") {
    const coll = flyData.collection("testing")
    coll.put("id", { foo: "bar" }).then(b => {
      console.log("put returned:", b);
      coll.get("id").then(d => {
        console.log("get returned:", d)
        coll.del("id").then(b => {
          console.log("del returned:", b)
          coll.get("id").then(d => {
            console.log("get returned:", d);
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

    fetch("file://.").then(res => {
      console.log("file res:", res);
      res.text().then(p => { console.log("p", p) })
    }).catch(err => { console.log("err fetching file://.:", err.stack) })

    resolv("fly.io").then(res => {
      console.log("got dns res:", res)
    }).catch(err => { console.log("error resolving I guess:", err.stack) })
    return new Response(null, { headers: {} })
  }
  else {
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
  // event.respondWith(resolv(event.request.queries[0]))
  event.respondWith(function () {
    return {
      authoritative: true,
      answers: [
        {
          name: event.request.queries[0].name,
          rrType: DNSRecordType.A,
          ttl: 5,
          data: "127.0.0.1"
        }
      ]
    }
  })
})