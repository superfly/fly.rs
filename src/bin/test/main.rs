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

extern crate glob;
use glob::glob;

extern crate serde_json;

// const LIB_SOURCE: &'static [u8] = include_bytes!("lib.js");
const MOCHA_SOURCE: &'static [u8] = include_bytes!("mocha.js");
const CHAI_SOURCE: &'static [u8] = include_bytes!("chai.js");
// const EXPECT_SOURCE: &'static [u8] = include_bytes!("expect.js");
const SETUP_SOURCE: &'static [u8] = include_bytes!("setup.js");
const RUN_SOURCE: &'static [u8] = include_bytes!("run.js");

// const FLY_TESTING_SOURCE: &'static [u8] = include_bytes!("../../../v8env/dist/testing.js");

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");
  env_logger::init_from_env(env);

  let rt = Runtime::new(None);
  rt.eval("A", "console.log('A window.run', window.run);");
  rt.eval("A", "console.log('A run', run);");
  rt.eval_file("./v8env/node_modules/rollup/dist/rollup.browser.js");
  rt.eval_file("./loader.js");
  rt.eval_file("./v8env/dist/build.js");
  rt.eval("B", "console.log('B window.run', window.run);");
  rt.eval("B", "console.log('B run', run);");
  rt.eval("mocha.js", str::from_utf8(MOCHA_SOURCE).unwrap());
  rt.eval("chai.js", str::from_utf8(CHAI_SOURCE).unwrap());
  // rt.eval("testing.js", str::from_utf8(FLY_TESTING_SOURCE).unwrap());
  rt.eval_file("./v8env/dist/testing.js");
  rt.eval("C", "console.log('C window.run', window.run);");
  rt.eval("C", "console.log('C run', run);");
  rt.eval("C", "console.log('C run2', run2);");
  rt.eval("setup.js", str::from_utf8(SETUP_SOURCE).unwrap());
  rt.eval("C", "console.log('D run', run);");
  rt.eval("C", "console.log('D run2', run2);");
  let args: Vec<String> = env::args().collect();

  let mut patterns: Vec<String> = args[1..].to_vec();

  debug!("args: {:?}", &args);
  debug!("patterns: {:?}", &patterns);

  if patterns.len() == 0 {
    patterns.push(String::from("./**/*[._]spec.js"));
    patterns.push(String::from("./**/*[._]test.js"));
  }

  let mut files = Vec::new();

  for pattern in patterns {
    for path in glob(&pattern).unwrap().filter_map(Result::ok) {
      let path = path.to_str().expect("invalid path");
      info!("found test '{}'", path);
      files.push(path.to_string());
      // rt.eval_file(path.to_str().expect("invalid path"));
    }
  }

  rt.eval("runcheck", "console.log('A runcheck:', run);");
  rt.eval("runcheck", "console.log('A runcheck2:', run2);");

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  let run_args = serde_json::to_string(&files).unwrap();
  // rt.eval("wtf", "console.log('window', window);");
  rt.eval("runcheck", "console.log('B runcheck:', run);");
  rt.eval("runcheck", "console.log('B runcheck2:', run2);");
  rt.eval("entry", format!("run2({}, '/')", run_args).as_str());

  // rt.eval("run.js", str::from_utf8(RUN_SOURCE).unwrap());
  rt.eval_file("./src/bin/test/run.js");

  main_el
    .block_on(future::lazy(|| -> Result<(), ()> {
      sleep(Duration::from_secs(5));
      Ok(())
    })).unwrap();
}
