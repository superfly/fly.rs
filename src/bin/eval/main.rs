extern crate clap;
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate log;

extern crate fly;
use fly::logging;
use fly::runtime::Runtime;
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

  let mut runtime = Runtime::new(None, None, &SETTINGS.read().unwrap(), None, &app_logger);
  debug!("Loading dev tools");
  runtime.eval_file("v8env/dist/dev-tools.js");
  runtime.eval("<installDevTools>", "installDevTools();");
  debug!("Loading dev tools done");

  let entry_file = matches.value_of("input").unwrap();

  runtime.eval(entry_file, &format!("dev.run('{}')", entry_file));
  runtime.run().wait().unwrap();
}
