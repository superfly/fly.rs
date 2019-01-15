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

use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

use tokio::net::{TcpListener, TcpStream};

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

#[macro_use]
extern crate prometheus;

mod cert;
mod metrics;
mod proxy;
mod libs;
use crate::metrics::*;
use fly::metrics::*;

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

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let _guard = {
        if let Some(ref sentry_dsn) = GLOBAL_SETTINGS.read().unwrap().sentry_dsn {
            Some(sentry::init(sentry_dsn.as_str()))
        } else {
            None
        }
    };

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

    let http_listener = TcpListener::bind(&addr).unwrap();
    let tls_listener = TcpListener::bind(&tls_addr).unwrap();

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

    tls_builder.set_session_cache_mode(openssl::ssl::SslSessionCacheMode::BOTH);

    let tls_acceptor = tls_builder.build();

    let tls_stream = tls_listener
        .incoming()
        .and_then(|stream| proxy::ProxyTcpStream::peek(stream))
        .map_err(|e| error!("error in incoming tls conn: {}", e))
        .for_each(move |stream| {
            let remote_addr = stream.peer_addr().unwrap();
            let timer = TLS_HANDSHAKE_TIME_HISTOGRAM.start_timer();
            tokio::spawn(
                tls_acceptor
                    .accept_async(stream)
                    .map_err(|e| error!("error handshake conn: {}", e))
                    .and_then(move |stream| {
                        timer.observe_duration();
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

    let prom_listener: Option<TcpListener> = {
        let s = GLOBAL_SETTINGS.read().unwrap();
        if let Some(ref raw) = s.prometheus_bind_addr {
            match raw.parse() {
                Ok(addr) => Some(TcpListener::bind(&addr).unwrap()),
                Err(e) => {
                    error!("error parsing prometheus bind addr: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    tokio::run(future::lazy(move || {
        tokio::spawn(runtime_monitoring());

        tokio::spawn(
            Server::builder(http_stream)
                .serve(make_svc)
                .map_err(|e| error!("error in http server: {}", e)),
        );
        info!("HTTP listening on {}", addr);

        tokio::spawn(tls_stream);
        info!("HTTPS listening on {}", tls_addr);

        if let Some(prom_ln) = prom_listener {
            let addr = prom_ln.local_addr().unwrap();
            tokio::spawn(
                Server::builder(prom_ln.incoming())
                    .serve(make_service_fn(|_conn: &TcpStream| {
                        service_fn(move |req| fly::metrics::serve_metrics_http(req))
                    }))
                    .map_err(|e| error!("error in http prom server: {}", e)),
            );
            info!("Prometheus listening on {}", addr);
        }

        Ok(())
    }));
}

use std::sync::atomic::Ordering;
use std::time;

static MAX_RUNTIME_IDLE_SECONDS: usize = 5 * 60;

fn runtime_monitoring() -> impl Future<Item = (), Error = ()> + Send + 'static {
    Interval::new_interval(Duration::from_secs(15))
        .map_err(|e| error!("timer error: {}", e))
        .for_each(|_| {
            match SELECTOR.runtimes.read() {
                Err(e) => error!("error getting read lock on runtime selector: {}", e),
                Ok(guard) => {
                    guard.iter().for_each(|(k, rt)| {
                        let stats = rt.heap_statistics();
                        RUNTIME_USED_HEAP_GAUGE
                            .with_label_values(&[rt.name.as_str(), &rt.version.as_str()])
                            .set(stats.used_heap_size as i64);
                        RUNTIME_TOTAL_HEAP_GAUGE
                            .with_label_values(&[rt.name.as_str(), &rt.version.as_str()])
                            .set(stats.total_heap_size as i64);
                        RUNTIME_EXTERNAL_ALLOCATIONS_GAUGE
                            .with_label_values(&[rt.name.as_str(), &rt.version.as_str()])
                            .set(stats.externally_allocated as i64);
                        RUNTIME_MALLOCED_MEMORY_GAUGE
                            .with_label_values(&[rt.name.as_str(), &rt.version.as_str()])
                            .set(stats.malloced_memory as i64);
                        RUNTIME_PEAK_MALLOCED_MEMORY_GAUGE
                            .with_label_values(&[rt.name.as_str(), &rt.version.as_str()])
                            .set(stats.peak_malloced_memory as i64);
                        info!(
                            "{}:v{} runtime heap at: {:.2} MB",
                            rt.name,
                            rt.version,
                            stats.used_heap_size as f64 / 1024.0 / 1024.0
                        );

                        // teardown idle runtimes.
                        if let Ok(epoch) = time::SystemTime::now().duration_since(time::UNIX_EPOCH)
                        {
                            if epoch.as_secs() as usize - rt.last_event_at.load(Ordering::SeqCst)
                                > MAX_RUNTIME_IDLE_SECONDS
                            {
                                let key = k.clone();
                                tokio::spawn(future::lazy(move || {
                                    match SELECTOR.runtimes.write() {
                                        Err(e) => error!(
                                            "error getting write lock on runtime selector: {}",
                                            e
                                        ),
                                        Ok(mut guard) => match guard.remove(&key) {
                                            None => {}
                                            Some(mut rt) => {
                                                rt.dispose();
                                            }
                                        },
                                    };
                                    Ok(())
                                }));
                            }
                        }
                    });
                }
            };
            Ok(())
        })
}
