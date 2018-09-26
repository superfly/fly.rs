console.log("ha");

// console.log(new TextDecoder().decode(new Uint8Array([104, 101, 108, 108, 111])))

let now = Date.now();
setTimeout(() => { console.log("in timeout!", Date.now() - now); now = Date.now() }, 100)

let arr = new Uint8Array(32);
crypto.getRandomValues(arr);
console.log("some random values:", arr);

addEventListener("fetch", function (event) {
  const req = event.request;
  // console.log("req url:", event.request.url);
  let url = new URL(req.url)
  if (url.pathname.endsWith("echo"))
    event.respondWith(new Response(req.body, { headers: { foo: "bar" } }))
  else if (url.pathname.endsWith("null"))
    event.respondWith(new Response(null, { headers: {} }))
  else {
    req.headers.delete("host");
    let u = url.searchParams.get("url");
    let toFetch = new Request(req)
    toFetch.url = u;
    console.log("to fetch url:", toFetch.url);

    if (url.searchParams.get("cache")) {
      return event.respondWith(cache.match(toFetch).then(res => {
        console.log("got res?", !!res);
        if (res)
          return res

        console.log("fetching then... url:", toFetch.url);
        return fetch(toFetch).then(res => {
          console.log("fetched!")
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