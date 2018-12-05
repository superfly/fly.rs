extern crate r2d2;
extern crate r2d2_redis;
extern crate redis;

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
use fly::runtime::*;

use std::sync::RwLock;

use std::collections::HashMap;
use std::sync::mpsc::RecvError;

use std::time::Duration;
use tokio::timer::Interval;

extern crate hyper;
use futures::sync::oneshot;
use futures::{future, Future, Stream};

use hyper::body::Payload;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{header, Body, Request, Response, Server, StatusCode};

use std::sync::atomic::Ordering;

use env_logger::Env;

use std::net::SocketAddr;

mod release;
use release::Release;

mod kms;

extern crate rusoto_core;
extern crate rusoto_credential;

use rusoto_credential::{AwsCredentials, EnvironmentProvider, ProvideAwsCredentials};

lazy_static! {
    static ref RUNTIMES: RwLock<HashMap<String, Box<Runtime>>> = RwLock::new(HashMap::new());
    pub static ref AWS_CREDENTIALS: AwsCredentials =
        EnvironmentProvider::default().credentials().wait().unwrap();
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);

    let addr = "127.0.0.1:8888".parse().unwrap();

    tokio::run(future::lazy(move || {
        tokio::spawn(
            Interval::new_interval(Duration::from_secs(30))
                .map_err(|e| error!("timer error: {}", e))
                .for_each(|_| {
                    RUNTIMES.read().unwrap().iter().for_each(|(k, rt)| {
                        info!("{} {:?}", k, rt.heap_statistics());
                    });
                    Ok(())
                }),
        );

        let server = Server::bind(&addr)
            .serve(make_service_fn(|conn: &AddrStream| {
                let remote_addr = conn.remote_addr();
                service_fn(move |req| server_fn(req, remote_addr))
            })).map_err(|e| eprintln!("server error: {}", e));

        info!("Listening on http://{}", addr);

        server
    }));
}

fn server_fn(
    req: Request<Body>,
    remote_addr: SocketAddr,
) -> Box<Future<Item = Response<Body>, Error = futures::Canceled> + Send> {
    let (parts, body) = req.into_parts();
    let host = {
        match parts.headers.get(header::HOST) {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(e) => {
                    error!("error stringifying host: {}", e);
                    return Box::new(future::ok(
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::from("Bad host header"))
                            .unwrap(),
                    ));
                }
            },
            None => {
                return Box::new(future::ok(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                ))
            }
        }
    };

    let rel = Release::get(host).unwrap().unwrap(); // TODO: handle Err and None
    let key = format!("{}:{}", rel.app_id, rel.version);

    {
        if !RUNTIMES.read().unwrap().contains_key(&key) {
            use fly::settings::*;
            let settings = Settings {
                data_store: Some(DataStore::Sqlite(SqliteStoreConfig {
                    filename: format!("data_{}.db", rel.app_id),
                })), // TODO: use postgres store
                cache_store: Some(CacheStore::Sqlite(SqliteStoreConfig {
                    filename: format!("cache_{}.db", rel.app_id),
                })), // TODO: use redis store
            };
            let mut rt = Runtime::new(Some(rel.app.clone()), &settings);
            let merged_conf = rel.clone().parsed_config().unwrap();
            rt.eval(
                "<app config>",
                &format!("window.app = {{ config: {} }};", merged_conf),
            );
            rt.eval("app.js", &rel.source);
            let app = rel.app;
            let app_id = rel.app_id;
            let version = rel.version;

            // TODO: ughh, refactor!
            let key2 = key.clone();
            tokio::spawn(rt.run().then(move |res: Result<(), _>| {
                if let Err(_) = res {
                    error!("app: {} ({}) v{} ended abruptly", app, app_id, version);
                }
                RUNTIMES.write().unwrap().remove(&key2);
                Ok(())
            }));
            {
                debug!("writing runtime in hashmap");
                RUNTIMES.write().unwrap().insert(key.clone(), rt);
            }
        }
    }

    let runtimes = RUNTIMES.read().unwrap(); // TODO: no unwrap
    let rt = runtimes.get(&key).unwrap();

    if rt.fetch_events.is_none() {
        return Box::new(future::ok(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
        ));
    }

    let req_id = fly::NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let url = format!("http://{}{}", host, parts.uri.path_and_query().unwrap());

    // double checking, could've been removed since last check (maybe not? may need a lock)
    if let Some(ref ch) = rt.fetch_events {
        let rx = {
            let (tx, rx) = oneshot::channel::<JsHttpResponse>();
            rt.responses.lock().unwrap().insert(req_id, tx);
            rx
        };
        let sendres = ch.unbounded_send(JsHttpRequest {
            id: req_id,
            method: parts.method,
            remote_addr: remote_addr,
            url: url,
            headers: parts.headers.clone(),
            body: if body.is_end_stream() {
                None
            } else {
                Some(JsBody::HyperBody(body))
            },
        });

        if let Err(e) = sendres {
            error!("error sending js http request: {}", e);
            return Box::new(future::ok(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap(),
            ));
        }

        Box::new(rx.and_then(|res: JsHttpResponse| {
            let (mut parts, mut body) = Response::<Body>::default().into_parts();
            parts.headers = res.headers;
            parts.status = res.status;

            if let Some(js_body) = res.body {
                body = match js_body {
                    JsBody::Stream(s) => Body::wrap_stream(s.map_err(|_| RecvError {})),
                    JsBody::BytesStream(s) => {
                        Body::wrap_stream(s.map_err(|_| RecvError {}).map(|bm| bm.freeze()))
                    }
                    JsBody::Static(b) => Body::from(b),
                    _ => unimplemented!(),
                };
            }

            future::ok(Response::from_parts(parts, body))
        }))
    } else {
        Box::new(future::ok(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
        ))
    }
}
