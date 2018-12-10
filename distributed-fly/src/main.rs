#[macro_use]
extern crate serde_derive;
extern crate rmp_serde as rmps;

extern crate serde;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

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

    // let port: u16 = match GLOBAL_SETTINGS.read().unwrap().proxy_dns_port {
    //     Some(port) => port,
    //     None => 8053,
    // };

    // let dns_server = DnsServer::new(
    //     SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port),
    //     &*SELECTOR,
    // );

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

    // let http_listener = tokio::net::TcpListener::bind(&addr).unwrap();
    let tls_listener = tokio::net::TcpListener::bind(&tls_addr).unwrap();

    let mut tls_builder =
        openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls()).unwrap();

    tls_builder.set_servername_callback(move |ssl_ref: &mut openssl::ssl::SslRef, _ssl_alert| {
        let name = ssl_ref.servername(openssl::ssl::NameType::HOST_NAME);
        println!("GOT NAME: {:?}", name);
        println!("version: {:?}", ssl_ref.version_str());
        match ssl_ref.current_cipher() {
            Some(cipher) => println!(
                "current cipher: {}, version: {}, desc: {}",
                cipher.name(),
                cipher.version(),
                cipher.description()
            ),
            None => println!("no cipher"),
        };
        match name {
            None => Err(openssl::ssl::SniError::NOACK),
            Some(name) => match cert::get_ctx(name) {
                Err(e) => {
                    error!("error getting context: {}", e);
                    Err(openssl::ssl::SniError::ALERT_FATAL)
                }
                Ok(maybe_ctx) => match maybe_ctx {
                    None => Err(openssl::ssl::SniError::NOACK),
                    Some(ctx) => {
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

    // let mut f = File::open("certs/default.crt").expect("could not open certificate file");
    // let mut rsabuf: Vec<u8> = vec![];
    // f.read_to_end(&mut rsabuf).unwrap();

    // let mut f = File::open("certs/default.pem").expect("could not open private key file");
    // let mut rsapembuf: Vec<u8> = vec![];
    // f.read_to_end(&mut rsapembuf).unwrap();

    // let cert_builder = openssl::x509::X509::builder()

    // cert_builder.

    // let certs = openssl::x509::X509::stack_from_pem(rsabuf.as_slice()).unwrap();
    // println!("certs count: {}", certs.len());
    // println!(
    //     "cert pem: {}",
    //     String::from_utf8(certs[0].to_pem().unwrap()).unwrap()
    // );

    // let pk = openssl::pkey::PKey::private_key_from_pem(rsabuf.as_slice()).unwrap();

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
    // tls_builder.set_certificate(&certs[0]).unwrap();

    let tls_acceptor = tls_builder.build();

    let tls_stream = tls_listener
        .incoming()
        .map_err(|e| error!("error accepting conn: {}", e))
        .for_each(move |stream| {
            println!("got a conn.");
            let remote_addr = stream.peer_addr().unwrap_or("0.0.0.0:0".parse().unwrap());
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

        let server = Server::bind(&addr)
            .serve(make_service_fn(|conn: &AddrStream| {
                let remote_addr = conn.remote_addr();
                service_fn(move |req| serve_http(false, req, &*SELECTOR, remote_addr))
            }))
            .map_err(|e| eprintln!("server error: {}", e));

        info!("Listening on http://{}", addr);

        server
    }));
}
