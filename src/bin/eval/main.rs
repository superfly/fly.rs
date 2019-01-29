extern crate clap;
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate log;

extern crate fly;
use fly::logging;
use fly::runtime::{Runtime, RuntimeConfig};
use fly::settings::SETTINGS;

use futures::Future;

fn main() {
  let (_guard, app_logger) = logging::configure();

  debug!("V8 version: {}", libfly::version());

  let matches = clap::App::new("fly-tsc")
    .version("0.0.1-alpha")
    .about("Fly typescript compiler")
    .arg(
      clap::Arg::with_name("input")
        .help("Sets the input file to use")
        .required(true)
        .index(1),
    )
    .get_matches();

  let mut runtime = Runtime::new(RuntimeConfig {
    name: None,
    version: None,
    settings: &SETTINGS.read().unwrap(),
    module_resolvers: None,
    app_logger: &app_logger,
    msg_handler: None,
    permissions: None,
    dev_tools: true,
  });

  let entry_file = matches.value_of("input").unwrap();
  runtime.eval_file_with_dev_tools(entry_file);
  runtime.run().wait().unwrap();
}
