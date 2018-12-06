extern crate r2d2;
extern crate r2d2_redis;

#[macro_use]
extern crate serde_derive;
extern crate rmp_serde as rmps;

extern crate serde;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate futures;
extern crate tokio;

extern crate fly;
use fly::dns_server::DnsServer;

use std::time::Duration;
use tokio::timer::Interval;

extern crate hyper;
use futures::{future, Future, Stream};

use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

use env_logger::Env;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

mod release;

mod settings;
use settings::GLOBAL_SETTINGS;

use fly::http_server::serve_http;

mod runtime_selector;
use runtime_selector::DistributedRuntimeSelector;
mod kms;

extern crate rusoto_core;
extern crate rusoto_credential;

use rusoto_credential::{AwsCredentials, EnvironmentProvider, ProvideAwsCredentials};

lazy_static! {
    static ref SELECTOR: DistributedRuntimeSelector = DistributedRuntimeSelector::new();
    pub static ref AWS_CREDENTIALS: AwsCredentials =
        EnvironmentProvider::default().credentials().wait().unwrap();
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let addr = "127.0.0.1:8888".parse().unwrap(); // TODO: use config

    release::start_new_release_check();

    let port: u16 = match GLOBAL_SETTINGS.read().unwrap().proxy_dns_port {
        Some(port) => port,
        None => 8053,
    };

    let dns_server = DnsServer::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port),
        &*SELECTOR,
    );

    tokio::run(future::lazy(move || {
        tokio::spawn(
            Interval::new_interval(Duration::from_secs(30))
                .map_err(|e| error!("timer error: {}", e))
                .for_each(|_| {
                    SELECTOR
                        .runtimes
                        .read()
                        .unwrap()
                        .iter()
                        .for_each(|(k, rt)| {
                            info!("{} {:?}", k, rt.heap_statistics());
                        });
                    Ok(())
                }),
        );

        dns_server.start();

        let server = Server::bind(&addr)
            .serve(make_service_fn(|conn: &AddrStream| {
                let remote_addr = conn.remote_addr();
                service_fn(move |req| serve_http(req, &*SELECTOR, remote_addr))
            })).map_err(|e| eprintln!("server error: {}", e));

        info!("Listening on http://{}", addr);

        server
    }));
}
