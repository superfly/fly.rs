#[macro_use]
extern crate serde_derive;
extern crate rmp_serde as rmps;

extern crate serde;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

#[macro_use]
extern crate futures;

// use fly::dns_server::DnsServer;

use std::time::Duration;
use tokio::timer::Interval;

extern crate hyper;
use futures::{future, Future, Stream};

use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

use env_logger::Env;

// use std::net::{IpAddr, Ipv4Addr, SocketAddr};

mod release;

mod settings;
use crate::settings::GLOBAL_SETTINGS;

use fly::http_server::serve_http;

mod runtime_selector;
use crate::runtime_selector::DistributedRuntimeSelector;
mod kms;

use rusoto_credential::{AwsCredentials, EnvironmentProvider, ProvideAwsCredentials};

use tokio_openssl::SslAcceptorExt;

mod cert;
mod proxy;

use r2d2_redis::RedisConnectionManager;

lazy_static! {
    static ref SELECTOR: DistributedRuntimeSelector = DistributedRuntimeSelector::new();
    pub static ref AWS_CREDENTIALS: AwsCredentials =
        EnvironmentProvider::default().credentials().wait().unwrap();
    pub static ref REDIS_POOL: r2d2::Pool<RedisConnectionManager> = r2d2::Pool::builder()
        .build(
            RedisConnectionManager::new(GLOBAL_SETTINGS.read().unwrap().redis_url.as_str())
                .unwrap()
        )
        .unwrap();
}

static CURVES: &str = "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-SHA:ECDHE-ECDSA-AES256-SHA:ECDHE-ECDSA-AES128-SHA256:ECDHE-ECDSA-AES256-SHA384:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-RSA-AES128-SHA:ECDHE-RSA-AES256-SHA:ECDHE-RSA-AES128-SHA256:ECDHE-RSA-AES256-SHA384:DHE-RSA-AES128-GCM-SHA256:DHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES128-SHA:DHE-RSA-AES256-SHA:DHE-RSA-AES128-SHA256:DHE-RSA-AES256-SHA256";

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let addr = {
        let s = GLOBAL_SETTINGS.read().unwrap();
        format!(
            "{}:{}",
            s.proxy_bind_ip.as_ref().unwrap_or(&"127.0.0.1".to_string()),
            s.proxy_port.unwrap_or(8888)
        )
    }
    .parse()
    .unwrap();

    release::start_new_release_check();

    let tls_addr = {
        let s = GLOBAL_SETTINGS.read().unwrap();
        format!(
            "{}:{}",
            s.proxy_bind_ip.as_ref().unwrap_or(&"127.0.0.1".to_string()),
            s.proxy_tls_port.unwrap_or(8443)
        )
    }
    .parse()
    .unwrap();

    let http_listener = tokio::net::TcpListener::bind(&addr).unwrap();
    let tls_listener = tokio::net::TcpListener::bind(&tls_addr).unwrap();

    let mut tls_builder =
        openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls()).unwrap();

    tls_builder.set_servername_callback(move |ssl_ref: &mut openssl::ssl::SslRef, _ssl_alert| {
        match ssl_ref.servername(openssl::ssl::NameType::HOST_NAME) {
            None => Err(openssl::ssl::SniError::NOACK),
            Some(name) => match cert::get_ctx(name) {
                Err(e) => {
                    error!("error getting context: {}", e);
                    Err(openssl::ssl::SniError::ALERT_FATAL)
                }
                Ok(maybe_ctx) => match maybe_ctx {
                    None => Err(openssl::ssl::SniError::NOACK),
                    Some(ctx) => {
                        debug!("got a ctx!");
                        ssl_ref.set_ssl_context(&ctx).unwrap();
                        Ok(())
                    }
                },
            },
        }
    });

    tls_builder.set_alpn_protos(b"\x02h2\x08http/1.1").unwrap();
    tls_builder.set_alpn_select_callback(|_, client| {
        openssl::ssl::select_next_proto(b"\x02h2\x08http/1.1", client)
            .ok_or(openssl::ssl::AlpnError::NOACK)
    });
    tls_builder.set_cipher_list(CURVES).unwrap();

    let certs_path = {
        match GLOBAL_SETTINGS.read().unwrap().certs_path {
            Some(ref cp) => cp.clone(),
            None => "certs".to_string(),
        }
    };

    tls_builder
        .set_certificate_file(
            &format!("{}/default.crt", certs_path),
            openssl::ssl::SslFiletype::PEM,
        )
        .unwrap();
    tls_builder
        .set_private_key_file(
            &format!("{}/default.pem", certs_path),
            openssl::ssl::SslFiletype::PEM,
        )
        .unwrap();
    tls_builder
        .set_certificate_file(
            &format!("{}/default.ecdsa.crt", certs_path),
            openssl::ssl::SslFiletype::PEM,
        )
        .unwrap();
    tls_builder
        .set_private_key_file(
            &format!("{}/default.ecdsa.pem", certs_path),
            openssl::ssl::SslFiletype::PEM,
        )
        .unwrap();

    let tls_acceptor = tls_builder.build();

    let tls_stream = tls_listener
        .incoming()
        .and_then(|stream| proxy::ProxyTcpStream::peek(stream))
        .map_err(|e| error!("error in stream: {}", e))
        .for_each(move |stream| {
            let remote_addr = stream.peer_addr().unwrap();
            tokio::spawn(
                tls_acceptor
                    .accept_async(stream)
                    .map_err(|e| error!("error handshake conn: {}", e))
                    .and_then(move |stream| {
                        let h = hyper::server::conn::Http::new();
                        h.serve_connection(
                            stream,
                            service_fn(move |req| serve_http(true, req, &*SELECTOR, remote_addr)),
                        )
                        .map_err(|e| error!("error serving conn: {}", e))
                    }),
            );
            Ok(())
        });

    let make_svc = make_service_fn(|conn: &proxy::ProxyTcpStream| {
        let remote_addr = conn.peer_addr().unwrap_or("0.0.0.0:0".parse().unwrap());
        service_fn(move |req| serve_http(false, req, &*SELECTOR, remote_addr))
    });

    let http_stream = http_listener
        .incoming()
        .and_then(|stream| proxy::ProxyTcpStream::peek(stream));

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

        tokio::spawn(tls_stream);

        info!("Listening on http://{}", addr);
        Server::builder(http_stream)
            .serve(make_svc)
            .map_err(|e| error!("error in hyper server: {}", e))
    }));
}
