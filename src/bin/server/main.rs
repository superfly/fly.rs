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

use fly::libfly;

extern crate hyper;
use hyper::rt::Future;
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, StatusCode};

extern crate futures;
use futures::future::FutureResult;
use futures::sync::oneshot;

use std::fs::File;
use std::io::Read;

use tokio::prelude::*;
use tokio::timer::Interval;

use std::time::Duration;

use fly::runtime::*;

use env_logger::Env;

use std::collections::HashMap;
use std::sync::Mutex;

mod config;
use config::*;

use std::alloc::System;

#[global_allocator]
static A: System = System;

lazy_static! {
    pub static ref RUNTIMES: Mutex<HashMap<String, Box<Runtime>>> = Mutex::new(HashMap::new());
}

pub struct FlyServer {
    // config: Config,
}

extern crate libc;

use std::ffi::{CStr, CString};

impl Service for FlyServer {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = futures::Canceled;
    type Future = Box<dyn Future<Item = Response<Body>, Error = Self::Error> + Send>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let h = match req.headers().get("host") {
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
        };

        // info!("host: {}", h);

        let headers: Vec<(CString, CString)> = req
            .headers()
            .iter()
            .map(|(key, value)| {
                // // TODO: don't unwrap
                // let value = builder.create_string(value.to_str().unwrap());

                // msg::HeaderPair::create(
                //     builder,
                //     &msg::HeaderPairArgs {
                //         key: Some(key),
                //         value: Some(value),
                //         ..Default::default()
                //     },
                // )
                (
                    CString::new(key.as_str()).unwrap(),
                    CString::new(value.to_str().unwrap()).unwrap(),
                )
                // libfly::KeyValue {
                //     key: .as_ptr(),
                //     val: &libfly::Value::String(value.to_str().unwrap().as_ptr()),
                // }
            }).collect();

        let headers2: Vec<(*const libc::c_char, libfly::Value)> = headers
            .iter()
            .map(|(key, value)| (key.as_ptr(), libfly::Value::String(value.as_ptr())))
            .collect();

        let headers3: Vec<libfly::KeyValue> = headers2
            .iter()
            .map(|(key, value)| libfly::KeyValue {
                key: *key,
                val: value,
            }).collect();

        let url = CString::new(format!("http://{}", h)).unwrap();
        let args: Vec<libfly::Value> = vec![
            libfly::Value::Int32(0),
            libfly::Value::String(url.as_ptr()),
            libfly::Value::KeyValues {
                len: headers3.len() as i32,
                pairs: headers3.as_ptr(),
            },
        ];

        let guard = RUNTIMES.lock().unwrap();
        let rt = guard.values().next().unwrap();

        let (p, c) = oneshot::channel::<Vec<libfly::Value>>();

        match rt.send(0, String::from("http_request"), args) {
            libfly::Value::Int32(i) => {
                println!("got val: {:?}", i);
                rt.responses.lock().unwrap().insert(i, p);
            }
            _ => println!("unexpected return value"),
        }
        // println!("sent message..");

        Box::new(c.and_then(|args: Vec<libfly::Value>| {
            // let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
            // let base = msg::get_root_as_base(bytes);

            // let res = base.msg_as_http_response().unwrap();
            // println!("GOT RESPONSE: {:?}", res);

            let body = args[0];
            // println!("body: {:?}", body);

            match body {
                libfly::Value::String(s) => {
                    // println!("it a string! {}", unsafe {
                    //     CStr::from_ptr(s).to_str().unwrap()
                    // });
                    future::ok(Response::new(Body::from(unsafe {
                        CStr::from_ptr(s).to_str().unwrap()
                    })))
                }
                _ => future::ok(Response::new(Body::from("got nothing"))),
            }
        }))

        // Box::new(future::ok(Response::new(Body::from("ok"))))
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
        }).map_err(|e| panic!("interval errored; err={:?}", e));

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
