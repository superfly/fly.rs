test.skip("allow promise handlers after rejection", (done) => {
  const promise = Promise.reject();
  promise.then(done).catch(done)
})