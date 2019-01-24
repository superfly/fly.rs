extern crate fly;

#[macro_use]
extern crate log;

extern crate libfly;

use fly::logging;
use fly::runtime::{Runtime, RuntimeConfig};
use fly::runtime_permissions::RuntimePermissions;
use fly::settings::SETTINGS;
use std::env;

extern crate futures;
use futures::Future;

extern crate tokio;

extern crate glob;
use glob::glob;

fn main() {
  let (_guard, app_logger) = logging::configure();

  let mut rt = Runtime::new(RuntimeConfig {
    name: None,
    version: None,
    settings: &SETTINGS.read().unwrap(),
    module_resolvers: None,
    app_logger: &app_logger,
    msg_handler: None,
    permissions: Some(RuntimePermissions::new(true)),
  });

  trace!("Loading dev tools");
  rt.eval_file("v8env/dist/dev-tools.js");
  rt.eval("<installDevTools>", "installDevTools();");
  trace!("Loading dev tools done");
  
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
      debug!("Loading test file: {}", path.display());
      let filename = path
        .to_str()
        .expect(&format!("Invalid filename {}", path.display()));
      rt.eval(filename, &format!("dev.run('{}')", filename));
    }
  }

  rt.eval("<runTests>", "dev.runTests();");

  tokio::run(
    rt.run()
      .map_err(|_| error!("error running runtime event loop")),
  );
}
