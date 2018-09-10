#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

#[macro_use]
extern crate lazy_static;

extern crate env_logger;
extern crate fly;
extern crate js_sys;
extern crate tokio;
extern crate tokio_io_pool;
extern crate toml;

extern crate hyper;
use hyper::rt::Future;
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, StatusCode};

extern crate futures;
use futures::future::FutureResult;

use std::fs::File;
use std::io::Read;

use tokio::prelude::*;
use tokio::timer::Interval;

use std::time::Duration;

use fly::runtime::*;

use env_logger::Env;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

mod config;
use config::*;

extern crate flatbuffers;
use flatbuffers::FlatBufferBuilder;
use fly::msg;

use std::sync::atomic::{AtomicUsize, Ordering};

lazy_static! {
    pub static ref RUNTIMES: Mutex<HashMap<String, Box<Runtime>>> = Mutex::new(HashMap::new());
    static ref NEXT_REQ_ID: AtomicUsize = AtomicUsize::new(0);
}

pub struct FlyServer {
    // config: Config,
}

impl Service for FlyServer {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = FutureResult<Response<Body>, hyper::Error>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let h = match req.headers().get("host") {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(e) => {
                    error!("error stringifying host: {}", e);
                    return future::ok(
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .unwrap(),
                    );
                }
            },
            None => {
                return future::ok(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                )
            }
        };

        // info!("host: {}", h);

        let mut builder = &mut FlatBufferBuilder::new();
        let headers: Vec<_> = req
            .headers()
            .iter()
            .map(|(key, value)| {
                let key = builder.create_string(key.as_str());
                // TODO: don't unwrap
                let value = builder.create_string(value.to_str().unwrap());

                msg::HeaderPair::create(
                    builder,
                    &msg::HeaderPairArgs {
                        key: Some(key),
                        value: Some(value),
                        ..Default::default()
                    },
                )
            })
            .collect();
        // let url = builder.create_string(&format!("http://{}", h));
        // let headers_fbs = builder.create_vector(&headers);
        // let id = NEXT_REQ_ID.fetch_add(1, Ordering::SeqCst) as u32;
        // let msg = msg::HttpRequest::create(
        //     builder,
        //     &msg::HttpRequestArgs {
        //         id: id,
        //         url: Some(url),
        //         method: match req.method() {
        //             &Method::GET => msg::HttpMethod::Get,
        //             &Method::HEAD => msg::HttpMethod::Head,
        //             // TODO: more.
        //             _ => panic!("unsupported http method"),
        //         },
        //         headers: Some(headers_fbs),
        //         ..Default::default()
        //     },
        // );

        // let guard = RUNTIMES.lock().unwrap();
        // let rt = guard.values().next().unwrap();

        // let resfut = ResponseFuture::new();

        // {
        //     let mut resguard = rt.responses.lock().unwrap();
        //     resguard.insert(id, resfut);
        // }

        // send_base(
        //     rt.ptr.0,
        //     &mut builder,
        //     &msg::BaseArgs {
        //         msg: Some(msg.as_union_value()),
        //         msg_type: msg::Any::HttpRequest,
        //         ..Default::default()
        //     },
        // );

        // Arc::new(rt.responses.lock().unwrap().get(&id).unwrap())
        future::ok(Response::new(Body::from("ok")))
    }
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");

    info!("V8 version: {}", js_sys::version());

    env_logger::init_from_env(env);

    let mut file = File::open("fly.toml").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let conf: Config = toml::from_str(&contents).unwrap();

    println!("toml: {:?}", conf);

    for (name, app) in conf.apps.unwrap().iter() {
        let rt = Runtime::new();
        info!("inited rt");
        rt.eval_file("fly/packages/v8env/dist/bundle.js");
        let filename = app.filename.as_str();
        rt.eval_file(filename);

        {
            let mut rts = RUNTIMES.lock().unwrap();
            rts.insert(name.to_string(), rt);
        };
    }

    let task = Interval::new_interval(Duration::from_secs(5))
        .for_each(move |_| {
            match RUNTIMES.lock() {
                Ok(rts) => {
                    for (key, rt) in rts.iter() {
                        info!(
                            "memory usage for {0}: {1:.2}MB",
                            key,
                            rt.used_heap_size() as f64 / (1024_f64 * 1024_f64)
                        );
                    }
                }
                Err(e) => error!("error locking runtimes: {}", e),
            };
            Ok(())
        })
        .map_err(|e| panic!("interval errored; err={:?}", e));

    let mut main_el = tokio_io_pool::Runtime::new();

    main_el.spawn(task).unwrap();

    let addr = ([127, 0, 0, 1], conf.port.unwrap()).into();

    let ln = tokio::net::TcpListener::bind(&addr).expect("unable to bind TCP listener");

    let server = ln
        .incoming()
        .map_err(|_| unreachable!())
        .for_each(move |sock| {
            hyper::server::conn::Http::new().serve_connection(sock, FlyServer {})
        });

    let _ = main_el.block_on(server);
    main_el.shutdown_on_idle();
}

// #[no_mangle]
// pub extern "C" fn set_timeout(raw_info: *const js_callback_info) {
//     info!("set timeout called!");
//     let info = js::CallbackInfo::from_raw(raw_info);
//     let rt = info.runtime();
//     if let Some(fnv) = info.get(0) {
//         info!("got a fn: {}", fnv.to_string());
//         if let Some(msv) = info.get(1) {
//             info!("got some ms! {}", msv.to_i64());
//             let when = Instant::now() + Duration::from_millis(msv.to_i64() as u64);
//             let task = Delay::new(when)
//                 .and_then(move |_| {
//                     info!("in delayed closure");
//                     let res = fnv.call(rt);
//                     info!("call got: {}", res.to_string());
//                     Ok(())
//                 })
//                 .map_err(|e| panic!("delay errored; err={:?}", e));

//             tokio::spawn(task);
//         }
//     }
//     info!("set_timeout done");
// }

// extern "C" fn log(raw_info: *const js_callback_info) {
//     let info = js::CallbackInfo::from_raw(raw_info);
//     for i in 0..info.length() {
//         if let Some(v) = info.get(i) {
//             info!("log: {}", v.to_string());
//         }
//     }
// }
