#[macro_use]
extern crate log;

extern crate clap;

extern crate env_logger;
extern crate fly;
extern crate tokio;

extern crate libfly;

extern crate hyper;
use hyper::body::Payload;
use hyper::header;
use hyper::rt::{Future, Stream};
use hyper::service::{service_fn, Service};
use hyper::{Body, Request, Response, Server, StatusCode};

extern crate futures;
use futures::sync::mpsc;
use futures::sync::oneshot;
use std::sync::mpsc::RecvError;

use tokio::prelude::*;

use fly::runtime::*;
use fly::settings::SETTINGS;

use env_logger::Env;

extern crate flatbuffers;

use std::alloc::System;
#[global_allocator]
static A: System = System;

use std::sync::atomic::Ordering;

pub static mut RUNTIME: Option<Box<Runtime>> = None;

pub struct FlyServer;

impl Service for FlyServer {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = futures::Canceled;
    type Future = Box<dyn Future<Item = Response<Body>, Error = Self::Error> + Send>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
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

        // TODO: match host with appropriate runtime when multiple apps are supported.
        let rt = unsafe { RUNTIME.as_ref().unwrap() };

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
                url: url,
                headers: parts.headers.clone(),
                body: if body.is_end_stream() {
                    None
                } else {
                    let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
                    let spawnres = rt.event_loop.lock().unwrap().spawn(
                        body.map_err(|e| error!("error reading body chunk: {}", e))
                            .for_each(move |chunk| {
                                let sendres = tx.unbounded_send(chunk.into_bytes().to_vec());
                                if let Err(e) = sendres {
                                    error!("error sending js body chunk: {}", e);
                                }
                                Ok(())
                            }),
                    );
                    if let Err(e) = spawnres {
                        error!("error spawning body stream: {}", e);
                    }
                    Some(JsBody::Stream(rx))
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
                        JsBody::Static(b) => Body::from(b),
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
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");

    env_logger::init_from_env(env);

    let matches = clap::App::new("fly-http")
        .version("0.0.1-alpha")
        .about("Fly HTTP server")
        .arg(
            clap::Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true),
        ).arg(
            clap::Arg::with_name("bind")
                .short("b")
                .long("bind")
                .takes_value(true),
        ).arg(
            clap::Arg::with_name("input")
                .help("Sets the input file to use")
                .required(true)
                .index(1),
        ).get_matches();

    info!("V8 version: {}", libfly::version());

    let mut main_el = tokio::runtime::Runtime::new().unwrap();
    unsafe {
        EVENT_LOOP_HANDLE = Some(main_el.executor());
    };

    let mut runtime = Runtime::new(None, &SETTINGS.read().unwrap());
    runtime
        .main_eval_file(matches.value_of("input").unwrap())
        .unwrap();
    unsafe {
        RUNTIME = Some(runtime);
    };

    let bind = match matches.value_of("bind") {
        Some(b) => b,
        None => "127.0.0.1",
    };
    let port: u16 = match matches.value_of("port") {
        Some(pstr) => pstr.parse::<u16>().unwrap(),
        None => 8080,
    };
    let addr = format!("{}:{}", bind, port).parse().unwrap();
    info!("Listening on {}", addr);

    let server = Server::bind(&addr)
        .serve(move || service_fn(move |req| FlyServer {}.call(req)))
        .map_err(|e| eprintln!("server error: {}", e));

    let _ = main_el.block_on(server);
    main_el.shutdown_on_idle();
}
