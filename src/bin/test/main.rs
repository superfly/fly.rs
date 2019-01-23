extern crate fly;

#[macro_use]
extern crate log;

extern crate libfly;

use fly::logging;
use fly::runtime::{Runtime, RuntimeConfig};
use fly::settings::SETTINGS;
use std::env;

extern crate futures;
use futures::Future;

extern crate tokio;

use std::str;

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
  let (_guard, app_logger) = logging::configure();

  let mut rt = Runtime::new(RuntimeConfig {
    name: None,
    version: None,
    settings: &SETTINGS.read().unwrap(),
    module_resolvers: None,
    app_logger: &app_logger,
    msg_handler: None,
  });
  rt.eval("mocha.js", str::from_utf8(MOCHA_SOURCE).unwrap());
  rt.eval("chai.js", str::from_utf8(CHAI_SOURCE).unwrap());
  rt.eval("testing.js", str::from_utf8(FLY_TESTING_SOURCE).unwrap());
  rt.eval("setup.js", str::from_utf8(SETUP_SOURCE).unwrap());

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
      rt.eval_file(path.to_str().expect("invalid path"));
    }
  }

  rt.eval("run.js", str::from_utf8(RUN_SOURCE).unwrap());

  tokio::run(
    rt.run()
      .map_err(|_| error!("error running runtime event loop")),
  );
}
