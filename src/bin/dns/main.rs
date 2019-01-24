extern crate futures;
use futures::{future, Future};

extern crate tokio;

extern crate trust_dns as dns;
extern crate trust_dns_proto;
extern crate trust_dns_server;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate flatbuffers;

#[macro_use]
extern crate log;
extern crate fly;
extern crate libfly;

use fly::logging;
use fly::module_resolver::{JsonSecretsResolver, LocalDiskModuleResolver, ModuleResolver};
use fly::runtime::*;
use fly::settings::SETTINGS;
use fly::{dns_server::DnsServer, fixed_runtime_selector::FixedRuntimeSelector};

extern crate clap;

use std::path::PathBuf;

static mut SELECTOR: Option<FixedRuntimeSelector> = None;

fn main() {
  let (_guard, app_logger) = logging::configure();

  debug!("V8 version: {}", libfly::version());

  let matches = clap::App::new("fly-dns")
    .version("0.0.1-alpha")
    .about("Fly DNS server")
    .arg(
      clap::Arg::with_name("port")
        .short("p")
        .long("port")
        .takes_value(true),
    )
    .arg(
      clap::Arg::with_name("secrets-file")
        .short("sf")
        .long("secrets-file")
        .takes_value(true),
    )
    .arg(
      clap::Arg::with_name("input")
        .help("Sets the input file to use")
        .required(true)
        .index(1),
    )
    .get_matches();

  let mut module_resolvers: Vec<Box<ModuleResolver>> = std::vec::Vec::new();

  let secrets_file = match matches.value_of("secrets-file") {
    Some(v) => v,
    None => "./secrets.json",
  };

  let secrets_file_path = PathBuf::from(secrets_file);
  info!(
    "Loading secrets file from path {}",
    secrets_file_path.to_str().unwrap().to_string()
  );
  match secrets_file_path.is_file() {
    true => {
      let secrets_json =
        match std::fs::read_to_string(&secrets_file_path.to_str().unwrap().to_string()) {
          Ok(v) => v,
          Err(_err) => {
            info!("Failed to load secrets file!");
            "{}".to_string()
          }
        };
      let json_value: serde_json::Value = match serde_json::from_str(secrets_json.as_str()) {
        Ok(v) => v,
        Err(_err) => {
          // TODO: actual error output
          info!("Failed to parse json");
          serde_json::from_str("{}").unwrap()
        }
      };
      module_resolvers.push(Box::new(JsonSecretsResolver::new(json_value)));
    }
    false => {
      info!("Secrets file invalid");
    }
  };

  module_resolvers.push(Box::new(LocalDiskModuleResolver::new(None)));

  info!(
    "Module resolvers length {}",
    module_resolvers.len().to_string()
  );

  let entry_file = matches.value_of("input").unwrap();
  let mut runtime = Runtime::new(RuntimeConfig {
    name: None,
    version: None,
    settings: &SETTINGS.read().unwrap(),
    module_resolvers: Some(module_resolvers),
    app_logger: &app_logger,
    msg_handler: None,
    permissions: None,
  });

  debug!("Loading dev tools");
  runtime.eval_file("v8env/dist/dev-tools.js");
  runtime.eval("<installDevTools>", "installDevTools();");
  debug!("Loading dev tools done");
  runtime.eval(entry_file, &format!("dev.run('{}')", entry_file));

  let port: u16 = match matches.value_of("port") {
    Some(pstr) => pstr.parse::<u16>().unwrap(),
    None => 8053,
  };

  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

  tokio::run(future::lazy(move || -> Result<(), ()> {
    tokio::spawn(
      runtime
        .run()
        .map_err(|e| error!("error running runtime event loop: {}", e)),
    );
    unsafe { SELECTOR = Some(FixedRuntimeSelector::new(runtime)) }
    let server = DnsServer::new(addr, unsafe { SELECTOR.as_ref().unwrap() });
    server.start();
    Ok(())
  }));
}
