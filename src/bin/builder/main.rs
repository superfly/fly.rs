extern crate fly;

use fly::runtime::Runtime;
use std::env;

extern crate futures;
use futures::Future;

use std::str;

use fly::settings::SETTINGS;

const ROLLUP_BROWSER: &'static [u8] =
  include_bytes!("../../../v8env/node_modules/rollup/dist/rollup.browser.js");
const BUILDER_CODE: &'static [u8] = include_bytes!("./builder.js");

fn main() {
  let mut rt = Runtime::new(None, &SETTINGS.read().unwrap());
  rt.eval("rollup.browser.js", str::from_utf8(ROLLUP_BROWSER).unwrap());
  rt.eval("builder.js", str::from_utf8(BUILDER_CODE).unwrap());
  let args: Vec<String> = env::args().collect();

  rt.eval("<no file>", format!("buildFn('{}')", &args[1]).as_str());

  rt.run().wait().unwrap();
}
