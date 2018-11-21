declare var fly;

interface Event {
  respondWith(...any): any;
}

console.log("addEventListener");

addEventListener("fetch", function (event) {
  console.log("fetch event");
  event.respondWith(new Response("HELLO!"));
})

// fly.http.respondWith(req => {
//   return new Response("HELLO FROM TYPESCRIPT!");
// })

