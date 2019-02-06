use std::alloc::System;

#[global_allocator]
static A: System = System;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

#[macro_use]
extern crate futures;
use std::time::Duration;
use tokio::timer::Interval;

use futures::{future, Future, Stream};

use hyper::service::{make_service_fn, service_fn};
use hyper::Server;

use tokio::net::{TcpListener, TcpStream};

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
mod conn;
mod libs;
mod logging;
mod metrics;
mod proxy;
use crate::conn::*;
use crate::metrics::*;
use fly::metrics::*;

use r2d2_redis::RedisConnectionManager;
use slog_scope;

use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
pub static INTERRUPTING: AtomicBool = ATOMIC_BOOL_INIT;
pub fn is_interrupting() -> bool {
    return INTERRUPTING.load(Ordering::SeqCst);
}
fn interrupt() {
    INTERRUPTING.store(true, Ordering::SeqCst);
}

static mut SELECTOR: Option<DistributedRuntimeSelector> = None;

lazy_static! {
    pub static ref AWS_CREDENTIALS: AwsCredentials =
        EnvironmentProvider::default().credentials().wait().unwrap();
    pub static ref REDIS_POOL: r2d2::Pool<RedisConnectionManager> = r2d2::Pool::builder()
        .max_size(100)
        .min_idle(Some(0))
        .build(
            RedisConnectionManager::new(GLOBAL_SETTINGS.read().unwrap().redis_url.as_str())
                .unwrap()
        )
        .unwrap();
}

fn main() {
    let logger = crate::logging::build_logger();
    let _log_guard = slog_scope::set_global_logger(logger);
    slog_stdlog::init().unwrap();

    let _guard = {
        if let Some(ref sentry_dsn) = GLOBAL_SETTINGS.read().unwrap().sentry_dsn {
            let mut opts = sentry::ClientOptions::from(sentry_dsn.as_str());
            opts.release = Some(fly::BUILD_VERSION.into());
            let c = sentry::init(opts);
            sentry::integrations::panic::register_panic_handler();
            Some(c)
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

    tls_builder.set_session_cache_mode(openssl::ssl::SslSessionCacheMode::SERVER);

    let tls_acceptor = tls_builder.build();

    let tls_listener = TcpListener::bind(&tls_addr).unwrap();

    let tls_stream = tls_listener
        .incoming()
        .and_then(|stream| proxy::ProxyTcpStream::from_tcp_stream(stream, true))
        .and_then(move |pstream| {
            let timer = TLS_HANDSHAKE_TIME_HISTOGRAM.start_timer();
            tls_acceptor.accept_async(pstream).then(|r| {
                timer.observe_duration();
                match r {
                    Ok(stream) => Ok(Some(Conn::Tls(stream))),
                    Err(e) => {
                        error!("error accepting TLS connection: {}", e);
                        Ok(None)
                    }
                }
            })
        })
        .filter_map(|ssl_stream| ssl_stream);

    let tcp_listener = TcpListener::bind(&addr).unwrap();

    let tcp_stream = tcp_listener
        .incoming()
        .and_then(|stream| proxy::ProxyTcpStream::from_tcp_stream(stream, false))
        .map(|pstream| Conn::Tcp(pstream));

    let all_stream = tcp_stream.select(tls_stream);

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

    let (sigfut, sigrx) = fly::utils::signal_monitor();
    let (srv_shutdown_tx, srv_shutdown_rx) = futures::sync::oneshot::channel();
    let (prom_shutdown_tx, prom_shutdown_rx) = futures::sync::oneshot::channel();

    let http_server = Server::builder(all_stream)
        .serve(make_service_fn(|conn: &Conn| {
            let (remote_addr, tls) = match conn {
                Conn::Tcp(c) => (c.peer_addr(), false),
                Conn::Tls(c) => (c.get_ref().get_ref().peer_addr(), true),
            };
            let remote_addr = remote_addr
                .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
            service_fn(move |req| {
                serve_http(tls, req, unsafe { SELECTOR.as_ref().unwrap() }, remote_addr)
            })
        }))
        .with_graceful_shutdown(srv_shutdown_rx)
        .map_err(|e| {
            error!("server error: {}", e);
        })
        .and_then(move |_| {
            info!("http server closed.");
            unsafe { SELECTOR = None }; // Drops the selector
            Ok(())
        });

    tokio::run(future::lazy(move || {
        unsafe { SELECTOR = Some(DistributedRuntimeSelector::new()) };
        tokio::spawn(
            sigrx
                .map_err(|e| error!("error receiving signal: {}", e))
                .and_then(move |_| {
                    interrupt();
                    srv_shutdown_tx.send(()).ok();
                    prom_shutdown_tx.send(()).ok();
                    Ok(())
                }),
        );
        tokio::spawn(runtime_monitoring());
        tokio::spawn(http_server);
        info!("HTTP listening on {}", addr);
        info!("HTTPS listening on {}", tls_addr);

        if let Some(prom_ln) = prom_listener {
            let addr = prom_ln.local_addr().unwrap();
            tokio::spawn(
                Server::builder(prom_ln.incoming())
                    .serve(make_service_fn(|_conn: &TcpStream| {
                        service_fn(move |req| fly::metrics::serve_metrics_http(req))
                    }))
                    .with_graceful_shutdown(prom_shutdown_rx)
                    .map_err(|e| error!("error in http prom server: {}", e)),
            );
            info!("Prometheus listening on {}", addr);
        }

        sigfut
    }));
}

use std::time;

static MAX_RUNTIME_IDLE_SECONDS: usize = 5 * 60;

fn runtime_monitoring() -> impl Future<Item = (), Error = ()> + Send + 'static {
    Interval::new_interval(Duration::from_secs(15))
        .map_err(|e| error!("timer error: {}", e))
        .take_while(|_| Ok(unsafe { SELECTOR.is_some() }))
        .for_each(|_| {
            match unsafe { SELECTOR.as_ref().unwrap() }.runtimes.read() {
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
                                    let mut w = match unsafe { SELECTOR.as_ref().unwrap() }
                                        .runtimes
                                        .write()
                                    {
                                        Ok(w) => w,
                                        Err(poisoned) => {
                                            error!(
                                                "error getting write lock on runtime selector: {}",
                                                poisoned
                                            );
                                            poisoned.into_inner()
                                        }
                                    };
                                    match w.remove(&key) {
                                        None => {}
                                        Some(mut rt) => {
                                            rt.dispose();
                                        }
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
