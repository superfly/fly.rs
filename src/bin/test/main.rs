extern crate fly;

#[macro_use]
extern crate log;
extern crate env_logger;
use env_logger::Env;

extern crate libfly;

use fly::runtime::{Runtime, EVENT_LOOP_HANDLE};
use std::env;

extern crate futures;
use futures::future;

extern crate tokio;

use std::str;
use std::thread::sleep;
use std::time::Duration;

// const LIB_SOURCE: &'static [u8] = include_bytes!("lib.js");
const MOCHA_SOURCE: &'static [u8] = include_bytes!("mocha.js");
const CHAI_SOURCE: &'static [u8] = include_bytes!("chai.js");
// const EXPECT_SOURCE: &'static [u8] = include_bytes!("expect.js");
const SETUP_SOURCE: &'static [u8] = include_bytes!("setup.js");
const RUN_SOURCE: &'static [u8] = include_bytes!("run.js");

const FLY_TESTING_SOURCE: &'static [u8] = include_bytes!("../../../v8env/dist/testing.js");

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");
  env_logger::init_from_env(env);
  info!("V8 version: {}", libfly::version());

  let rt = Runtime::new(None);
  rt.eval("mocha.js", str::from_utf8(MOCHA_SOURCE).unwrap());
  // rt.eval("lib.js", str::from_utf8(LIB_SOURCE).unwrap());
  rt.eval("chai.js", str::from_utf8(CHAI_SOURCE).unwrap());
  rt.eval("testing.js", str::from_utf8(FLY_TESTING_SOURCE).unwrap());
  // rt.eval("expect.js", str::from_utf8(EXPECT_SOURCE).unwrap());
  rt.eval("setup.js", str::from_utf8(SETUP_SOURCE).unwrap());
  // let args: Vec<String> = env::args().collect();

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  rt.eval_file("test.js");

  rt.eval("run.js", str::from_utf8(RUN_SOURCE).unwrap());
  // rt.eval("run.js", "runTests()");

  // rt.eval("run.js", str::from_utf8(RUN_SOURCE).unwrap());

  main_el
    .block_on(future::lazy(|| -> Result<(), ()> {
      sleep(Duration::from_secs(5));
      Ok(())
    })).unwrap();
  // main_el.shutdown_on_idle();
}
