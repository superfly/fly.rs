extern crate clap;

extern crate fly;
extern crate tokio;

extern crate libfly;

extern crate hyper;
use hyper::rt::Future;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

extern crate futures;

use tokio::prelude::*;

use fly::fixed_runtime_selector::FixedRuntimeSelector;
use fly::http_server::serve_http;
use fly::runtime::*;
use fly::settings::SETTINGS;

use slog::o;
use slog::Drain;
use slog::*;

static mut SELECTOR: Option<FixedRuntimeSelector> = None;

fn main() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let logger = slog::Logger::root(drain, o!("version" => "0.5"));

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

    info!(logger, "V8 version: {}", libfly::version());

    let entry_file = matches.value_of("input").unwrap();
    let mut runtime = Runtime::new(None, None, &SETTINGS.read().unwrap(), &logger);

    debug!(logger, "Loading dev tools");
    runtime.eval_file("v8env/dist/dev-tools.js");
    runtime.eval("<installDevTools>", "installDevTools();");
    debug!(logger, "Loading dev tools done");
    runtime.eval(entry_file, &format!("dev.run('{}')", entry_file));

    let bind = match matches.value_of("bind") {
        Some(b) => b,
        None => "127.0.0.1",
    };
    let port: u16 = match matches.value_of("port") {
        Some(pstr) => pstr.parse::<u16>().unwrap(),
        None => 8080,
    };

    let addr = format!("{}:{}", bind, port).parse().unwrap();

    tokio::run(future::lazy(move || {
        tokio::spawn(
            runtime
                .run()
                .map_err(|e| println!("error running runtime event loop: {}", e)),
        );
        unsafe { SELECTOR = Some(FixedRuntimeSelector::new(runtime)) }
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
            .map_err(|e| eprintln!("server error: {}", e));

        info!(logger, "Listening on http://{}", addr);
        server
    }));
}
