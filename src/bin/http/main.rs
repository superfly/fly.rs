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
use hyper::rt::{poll_fn, Future, Stream};
use hyper::service::{service_fn, Service};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

#[macro_use]
extern crate futures;
use futures::sync::oneshot;
use std::sync::mpsc::RecvError;

use tokio::prelude::*;

use fly::runtime::*;
use fly::utils::*;

use env_logger::Env;

extern crate flatbuffers;
use flatbuffers::FlatBufferBuilder;

use std::alloc::System;
#[global_allocator]
static A: System = System;

use std::sync::atomic::Ordering;

pub static mut RUNTIME: Option<Box<Runtime>> = None;

pub struct FlyServer;

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

        let req_id = fly::NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst);

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
                has_body: !body.is_end_stream(),
                ..Default::default()
            },
        );

        let rt = unsafe { RUNTIME.as_ref().unwrap() };
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
            let spawnres = rt.event_loop.lock().unwrap().spawn(future::lazy(move || {
                rtptr.send(to_send, None);
                Ok(())
            }));
            if let Err(err) = spawnres {
                error!("error spawning: {}", err);
            }
        }

        if !body.is_end_stream() {
            let spawnres = rt.event_loop.lock().unwrap().spawn(
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
            if let Err(err) = spawnres {
                error!("error spawning: {}", err);
            }
        }

        Box::new(c.and_then(|res: JsHttpResponse| {
            let (mut parts, mut body) = Response::<Body>::default().into_parts();
            parts.headers = res.headers;
            parts.status = res.status;

            if let Some(js_body) = res.body {
                body = match js_body {
                    JsHttpResponseBody::Stream(s) => Body::wrap_stream(s.map_err(|_| RecvError {})),
                    JsHttpResponseBody::Static(b) => Body::from(b),
                };
            }

            future::ok(Response::from_parts(parts, body))
        }))
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

    let runtime = {
        let rt = Runtime::new(None);
        rt.eval_file(matches.value_of("input").unwrap());
        rt
    };
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
