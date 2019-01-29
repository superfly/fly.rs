#[macro_use]
extern crate log;

use fly::logging;
use fly::runtime::{Runtime, RuntimeConfig};
use fly::runtime_permissions::RuntimePermissions;
use fly::settings::SETTINGS;
use std::env;

use futures::Future;

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
    dev_tools: true,
  });

  let args: Vec<String> = env::args().collect();

  let mut patterns: Vec<String> = args[1..].to_vec();

  debug!("args: {:?}", &args);
  debug!("patterns: {:?}", &patterns);

  if patterns.len() == 0 {
    patterns.push(String::from("./**/*[._]spec.js"));
    patterns.push(String::from("./**/*[._]test.js"));
  }

  let mut test_files: Vec<String> = vec![];

  for pattern in patterns {
    for path in glob(&pattern).unwrap().filter_map(Result::ok) {
      let filename = path
        .to_str()
        .expect(&format!("Invalid filename {}", path.display()));
      test_files.push(filename.to_owned());
    }
  }

  rt.eval(
    "<runTests>",
    &format!(
      "dev.runTests({});",
      serde_json::to_string(&test_files).expect("error loading test files")
    ),
  );

  tokio::run(
    rt.run()
      .map_err(|_| error!("error running runtime event loop")),
  );
}
