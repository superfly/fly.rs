extern crate fly;

#[macro_use]
extern crate log;
extern crate env_logger;
use env_logger::Env;

use fly::runtime::{Runtime, EVENT_LOOP_HANDLE};
use std::env;

extern crate futures;
use futures::future;

extern crate tokio;

use std::str;
use std::thread::sleep;
use std::time::Duration;

const ROLLUP_BROWSER: &'static [u8] =
  include_bytes!("../../../v8env/node_modules/rollup/dist/rollup.browser.js");
// const BUILDER_CODE: &'static [u8] = include_bytes!("./builder.js");

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");
  env_logger::init_from_env(env);

  let rt = Runtime::new(None);
  rt.eval("rollup.browser.js", str::from_utf8(ROLLUP_BROWSER).unwrap());
  rt.eval_file("./v8env/dist/build.js");
  // rt.eval("<na>", "console.log('hello from builder main!')");
  // rt.eval("build.js", str::from_utf8(BUILDER_CODE).unwrap());
  let args: Vec<String> = env::args().collect();

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };
  // rt.eval("<na>", "console.log('after event loop!')");
  rt.eval("entry", format!("run('{}', '/')", &args[1]).as_str());

  main_el
    .block_on(future::lazy(|| -> Result<(), ()> {
      sleep(Duration::from_secs(5));
      Ok(())
    })).unwrap();
  main_el.shutdown_on_idle();
}
