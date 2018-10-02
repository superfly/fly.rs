extern crate fly;

use fly::runtime::{Runtime, EVENT_LOOP_HANDLE};
use std::env;

extern crate futures;
use futures::future;

extern crate tokio;

use std::str;
use std::thread::sleep;
use std::time::Duration;

const ROLLUP_BROWSER: &'static [u8] =
  include_bytes!("../../../node_modules/rollup/dist/rollup.browser.js");
const BUILDER_CODE: &'static [u8] = include_bytes!("./builder.js");

fn main() {
  let rt = Runtime::new();
  rt.eval("rollup.browser.js", str::from_utf8(ROLLUP_BROWSER).unwrap());
  rt.eval("builder.js", str::from_utf8(BUILDER_CODE).unwrap());
  let args: Vec<String> = env::args().collect();

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  rt.eval("<no file>", format!("buildFn('{}')", &args[1]).as_str());

  main_el
    .block_on(future::lazy(|| -> Result<(), ()> {
      sleep(Duration::from_secs(10));
      Ok(())
    })).unwrap();
  main_el.shutdown_on_idle();
}
