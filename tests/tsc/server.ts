declare var fly;

fly.http.respondWith(req => {
  return new Response("HELLO FROM TYPESCRIPT!");
})