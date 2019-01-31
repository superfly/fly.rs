#[macro_use]
extern crate log;
use hyper::rt::Future;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

use tokio::prelude::*;

use fly::fixed_runtime_selector::FixedRuntimeSelector;
use fly::http_server::serve_http;
use fly::logging;
use fly::runtime::*;
use fly::settings::SETTINGS;

static mut SELECTOR: Option<FixedRuntimeSelector> = None;

fn main() -> Result<(), Box<::std::error::Error>> {
    let (_guard, app_logger) = logging::configure();

    let matches = clap::App::new("fly-http")
        .version("0.0.1-alpha")
        .about("Fly HTTP server")
        .arg(
            clap::Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("bind")
                .short("b")
                .long("bind")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("input")
                .help("Sets the input file to use")
                .required(true)
                .index(1),
        )
        .get_matches();

    info!("V8 version: {}", libfly::version());

    let entry_file = matches.value_of("input").unwrap();
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

    runtime.eval_file_with_dev_tools(entry_file);

    let bind = match matches.value_of("bind") {
        Some(b) => b,
        None => "127.0.0.1",
    };
    let port: u16 = match matches.value_of("port") {
        Some(pstr) => pstr.parse::<u16>().unwrap(),
        None => 8080,
    };

    let addr = format!("{}:{}", bind, port).parse().unwrap();

    let (sigfut, sigrx) = fly::utils::signal_monitor();

    let server = Server::bind(&addr)
        .serve(make_service_fn(move |conn: &AddrStream| {
            let remote_addr = conn.remote_addr();
            service_fn(move |req| {
                serve_http(
                    false,
                    req,
                    unsafe { SELECTOR.as_ref().unwrap() },
                    remote_addr,
                )
            })
        }))
        .with_graceful_shutdown(sigrx)
        .map_err(|e| error!("server error: {}", e))
        .and_then(|_| {
            info!("HTTP server closed.");
            unsafe { SELECTOR = None };
            Ok(())
        });

    tokio::run(future::lazy(move || {
        tokio::spawn(
            runtime
                .run()
                .map_err(|e| error!("error running runtime event loop: {}", e)),
        );
        unsafe { SELECTOR = Some(FixedRuntimeSelector::new(runtime)) }

        tokio::spawn(server);
        info!("Listening on http://{}", addr);
        sigfut
    }));

    Ok(())
}
