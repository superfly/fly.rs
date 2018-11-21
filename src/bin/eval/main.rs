extern crate clap;
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate log;
extern crate env_logger;

use env_logger::Env;

extern crate fly;
use fly::runtime::Runtime;
use fly::settings::SETTINGS;

use futures::Future;

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "debug");
  env_logger::init_from_env(env);
  debug!("V8 version: {}", libfly::version());

  let matches = clap::App::new("fly-tsc")
    .version("0.0.1-alpha")
    .about("Fly typescript compiler")
    .arg(
      clap::Arg::with_name("input")
        .help("Sets the input file to use")
        .required(true)
        .index(1),
    ).get_matches();

  let mut runtime = Runtime::new(None, &SETTINGS.read().unwrap());
  debug!("Loading dev tools");
  runtime.eval_file("v8env/dist/dev-tools.js");
  runtime.eval("<installDevTools>", "installDevTools();");
  debug!("Loading dev tools done");

  let entry_file = matches.value_of("input").unwrap();

  runtime.eval(entry_file, &format!("dev.run('{}')", entry_file));
  runtime.run().wait().unwrap();
}
