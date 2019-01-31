declare var test: any;
declare var expect: any;

test("allow promise handlers after rejection", (done) => {
  const promise = Promise.reject("boom").then(_ => {
    done("`then` should not be excuted");
  }).catch(err => {
    if (err !== "boom") {
      done("Expected error to === boom");
    } else {
      done();
    }
  });
})

test("catch in async function", (done) => {
  async function thrower() {
    throw new Error("boom");
  }

  async function caller() {
    try {
      await thrower();
      expect.fail("catch block not invoked")
    } catch (err) {
      expect(err.message).to.eql("boom");
    }
  }

  caller().then(done).catch(done);
})