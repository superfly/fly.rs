extern crate clap;
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate log;
extern crate env_logger;

use env_logger::Env;

extern crate fly;
use fly::runtime::{Runtime, EVENT_LOOP_HANDLE};

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");
  env_logger::init_from_env(env);
  debug!("V8 version: {}", libfly::version());

  let matches = clap::App::new("fly-eval")
    .version("0.0.1-alpha")
    .about("Fly eval playground")
    .arg(
      clap::Arg::with_name("input")
        .help("Sets the input file to use")
        .required(true)
        .index(1),
    ).get_matches();

  let main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  main_el
    .block_on_all(futures::future::lazy(move || -> Result<(), ()> {
      let mut runtime = Runtime::new(None);
      runtime
        .main_eval_file(matches.value_of("input").unwrap())
        .unwrap();
      Ok(())
    })).unwrap();
}
