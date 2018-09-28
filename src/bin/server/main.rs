#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

#[macro_use]
extern crate lazy_static;

extern crate env_logger;
extern crate fly;
extern crate tokio;
extern crate tokio_io_pool;
extern crate toml;

extern crate libfly;

extern crate hyper;
use hyper::body::Payload;
use hyper::header;
use hyper::rt::{poll_fn, Future, Stream};
use hyper::service::{service_fn, Service};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

#[macro_use]
extern crate futures;
use futures::stream;
use futures::sync::{mpsc, oneshot};
use std::sync::mpsc::RecvError;

use std::fs::File;
use std::io::Read;

use tokio::prelude::*;
use tokio::timer::Interval;

use std::time::Duration;

use fly::runtime::*;

use env_logger::Env;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

mod config;
use config::*;

use std::alloc::System;

extern crate flatbuffers;
use flatbuffers::FlatBufferBuilder;

#[global_allocator]
static A: System = System;

use std::sync::atomic::Ordering;

lazy_static! {
    pub static ref RUNTIMES: RwLock<HashMap<String, Box<Runtime>>> = RwLock::new(HashMap::new());
}

pub struct FlyServer {
    // config: Config,
}

extern crate libc;

use fly::msg;

impl Service for FlyServer {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = futures::Canceled;
    type Future = Box<dyn Future<Item = Response<Body>, Error = Self::Error> + Send>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let (parts, mut body) = req.into_parts();
        let url = {
            format!(
                "http://{}{}",
                match parts.headers.get(header::HOST) {
                    Some(v) => match v.to_str() {
                        Ok(s) => s,
                        Err(e) => {
                            error!("error stringifying host: {}", e);
                            return Box::new(future::ok(
                                Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::empty())
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
                },
                parts.uri.path_and_query().unwrap()
            )
        };

        let builder = &mut FlatBufferBuilder::new();

        let req_id = fly::NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        let req_url = builder.create_string(url.as_str());

        let req_method = match parts.method {
            Method::GET => msg::HttpMethod::Get,
            Method::POST => msg::HttpMethod::Post,
            _ => unimplemented!(),
        };

        let headers: Vec<_> = parts
            .headers
            .iter()
            .map(|(key, value)| {
                let key = builder.create_string(key.as_str());
                let value = builder.create_string(value.to_str().unwrap());
                msg::HttpHeader::create(
                    builder,
                    &msg::HttpHeaderArgs {
                        key: Some(key),
                        value: Some(value),
                        ..Default::default()
                    },
                )
            }).collect();

        let req_headers = builder.create_vector(&headers);

        let req_msg = msg::HttpRequest::create(
            builder,
            &msg::HttpRequestArgs {
                id: req_id as u32,
                method: req_method,
                url: Some(req_url),
                headers: Some(req_headers),
                body: !body.is_end_stream(),
                ..Default::default()
            },
        );

        let guard = RUNTIMES.read().unwrap();
        let rt = guard.values().next().unwrap();
        let rtptr = rt.ptr;

        let to_send = fly_buf_from(
            serialize_response(
                0,
                builder,
                msg::BaseArgs {
                    msg: Some(req_msg.as_union_value()),
                    msg_type: msg::Any::HttpRequest,
                    ..Default::default()
                },
            ).unwrap(),
        );

        let (p, c) = oneshot::channel::<JsHttpResponse>();
        {
            rt.responses.lock().unwrap().insert(req_id as u32, p);
        }

        {
            let rtptr = rtptr.clone();
            rt.rt.lock().unwrap().spawn(future::lazy(move || {
                unsafe { libfly::js_send(rtptr.0, to_send, null_buf()) };
                Ok(())
            }));
        }

        if !body.is_end_stream() {
            rt.rt.lock().unwrap().spawn(
                poll_fn(move || {
                    while let Some(chunk) = try_ready!(body.poll_data()) {
                        let mut bytes = chunk.into_bytes();
                        let builder = &mut FlatBufferBuilder::new();
                        // let fb_bytes = builder.create_vector(&bytes);
                        let chunk_msg = msg::StreamChunk::create(
                            builder,
                            &msg::StreamChunkArgs {
                                id: req_id as u32,
                                // bytes: Some(fb_bytes),
                                done: body.is_end_stream(),
                            },
                        );
                        let to_send = fly_buf_from(
                            serialize_response(
                                0,
                                builder,
                                msg::BaseArgs {
                                    msg: Some(chunk_msg.as_union_value()),
                                    msg_type: msg::Any::StreamChunk,
                                    ..Default::default()
                                },
                            ).unwrap(),
                        );
                        unsafe {
                            libfly::js_send(
                                rtptr.0,
                                to_send,
                                libfly::fly_buf {
                                    alloc_ptr: 0 as *mut u8,
                                    alloc_len: 0,
                                    data_ptr: (*bytes).as_ptr() as *mut u8,
                                    data_len: bytes.len(),
                                },
                            )
                        };
                    }
                    Ok(Async::Ready(()))
                }).map_err(|e: hyper::Error| println!("hyper server error: {}", e)),
            );
        }

        Box::new(c.and_then(|res: JsHttpResponse| {
            let (mut parts, mut body) = Response::<Body>::default().into_parts();
            parts.headers = res.headers;
            parts.status = res.status;

            if let Some(bytes) = res.bytes {
                body = Body::wrap_stream(bytes.map_err(|_| RecvError {}));
            }

            future::ok(Response::from_parts(parts, body))
        }))
    }
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");

    println!("V8 version: {}", libfly::version());

    env_logger::init_from_env(env);

    let mut file = File::open("fly.toml").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let conf: Config = toml::from_str(&contents).unwrap();

    println!("toml: {:?}", conf);

    for (name, app) in conf.apps.unwrap().iter() {
        let rt = Runtime::new();
        info!("inited rt");
        // rt.eval_file("fly/packages/v8env/dist/bundle.js");
        let filename = app.filename.as_str();
        rt.eval_file(filename);

        {
            let mut rts = RUNTIMES.write().unwrap();
            rts.insert(name.to_string(), rt);
        };
    }

    let task = Interval::new_interval(Duration::from_secs(5))
        .for_each(move |_| {
            match RUNTIMES.read() {
                Ok(rts) => {
                    for (key, rt) in rts.iter() {
                        let stats = rt.heap_statistics();
                        info!(
                            "[heap stats for {0}] used: {1:.2}MB | total: {2:.2}MB | alloc: {3:.2}MB | malloc: {4:.2}MB | peak malloc: {5:.2}MB",
                            key,
                            stats.used_heap_size as f64 / (1024_f64 * 1024_f64),
                            stats.total_heap_size as f64 / (1024_f64 * 1024_f64),
                            stats.externally_allocated as f64 / (1024_f64 * 1024_f64),
                            stats.malloced_memory as f64 / (1024_f64 * 1024_f64),
                            stats.peak_malloced_memory as f64 / (1024_f64 * 1024_f64),
                        );
                    }
                }
                Err(e) => error!("error locking runtimes: {}", e),
            };
            Ok(())
        }).map_err(|e| panic!("interval errored; err={:?}", e));

    let mut main_el = tokio_io_pool::Runtime::new();

    main_el.spawn(task).unwrap();

    let addr = ([127, 0, 0, 1], conf.port.unwrap()).into();

    // FAST, one at a time:

    // let ln = tokio::net::TcpListener::bind(&addr).expect("unable to bind TCP listener");
    // let server = ln
    //     .incoming()
    //     .map_err(|_| unreachable!())
    //     .for_each(move |sock| {
    //         println!("got sock");
    //         hyper::server::conn::Http::new()
    //             .serve_connection(sock, FlyServer {})
    //             .map_err(|e| {
    //                 eprintln!("error serving conn: {}", e);
    //                 // sock.shutdown(net::Shutdown::Both);
    //             })
    //     });

    // SLOW, TOO MUCH CONCURRENCY but simpler...
    let server = Server::bind(&addr)
        .serve(move || service_fn(move |req| FlyServer {}.call(req)))
        .map_err(|e| eprintln!("server error: {}", e));

    unsafe {
        EVENT_LOOP_HANDLE = Some(Arc::new(Mutex::new(main_el.handle().clone())));
    };
    let _ = main_el.block_on(server);
    main_el.shutdown_on_idle();

    // let el = EVENT_LOOP.read().unwrap();
}
