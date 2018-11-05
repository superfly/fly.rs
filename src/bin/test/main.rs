extern crate fly;

#[macro_use]
extern crate log;
extern crate env_logger;
use env_logger::Env;

extern crate libfly;

use fly::runtime::{Runtime, EVENT_LOOP_HANDLE};
use fly::settings::SETTINGS;
use std::env;

extern crate futures;
use futures::future;

extern crate tokio;

use std::str;
use std::thread::sleep;
use std::time::Duration;

extern crate glob;
use glob::glob;

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

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  let mut rt = Runtime::new(None, &SETTINGS.read().unwrap());
  rt.eval("mocha.js", str::from_utf8(MOCHA_SOURCE).unwrap())
    .unwrap();
  rt.eval("chai.js", str::from_utf8(CHAI_SOURCE).unwrap())
    .unwrap();
  rt.eval("testing.js", str::from_utf8(FLY_TESTING_SOURCE).unwrap())
    .unwrap();
  rt.eval("setup.js", str::from_utf8(SETUP_SOURCE).unwrap())
    .unwrap();

  let args: Vec<String> = env::args().collect();

  let mut patterns: Vec<String> = args[1..].to_vec();

  debug!("args: {:?}", &args);
  debug!("patterns: {:?}", &patterns);

  if patterns.len() == 0 {
    patterns.push(String::from("./**/*[._]spec.js"));
    patterns.push(String::from("./**/*[._]test.js"));
  }

  for pattern in patterns {
    for path in glob(&pattern).unwrap().filter_map(Result::ok) {
      debug!("{}", path.display());
      rt.eval_file(path.to_str().expect("invalid path")).unwrap();
    }
  }

  rt.main_eval("run.js", str::from_utf8(RUN_SOURCE).unwrap())
    .unwrap();

  main_el
    .block_on(future::lazy(|| -> Result<(), ()> {
      sleep(Duration::from_secs(5));
      Ok(())
    })).unwrap();
}
