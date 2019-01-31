use crate::errors::*;
use crate::util::*;
use clap::{Arg, ArgMatches};

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

pub fn cli() -> App {
    subcommand("http")
        .about("Fly HTTP server")
        .arg(
            Arg::with_name("path")
                .help("The app to run")
                .default_value("index.{ts,js}")
                .index(1),
        )
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
            clap::Arg::with_name("lib")
                .short("l")
                .long("lib")
                .help("Libraries or shims to load before app code")
                .takes_value(true)
                .multiple(true),
        )
}

pub fn exec(args: &ArgMatches<'_>) -> FlyCliResult<()> {
    let (_guard, app_logger) = logging::configure();

    info!("V8 version: {}", libfly::version());

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

    if args.is_present("lib") {
        for lib_path in glob(args.values_of("lib").unwrap().collect())? {
            runtime.eval_file(&lib_path);
        }
    }

    if let Some(path) = glob(vec![args.value_of("path").unwrap()])?.first() {
        runtime.eval_file_with_dev_tools(path);
    } else {
        return Err(FlyCliError::from("No source code found"));
    }

    let bind = match args.value_of("bind") {
        Some(b) => b,
        None => "127.0.0.1",
    };
    let port: u16 = match args.value_of("port") {
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

        println!("Listening on http://{}", addr);

        sigfut
    }));

    Ok(())
}
