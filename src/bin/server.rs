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

use std::fs::File;
use std::io::Read;

use tokio::prelude::*;
use tokio::timer::Interval;

use std::time::Duration;

use fly::config::*;
use fly::runtime::*;

use env_logger::Env;

use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    static ref RUNTIMES: Mutex<HashMap<String, Box<Runtime>>> = Mutex::new(HashMap::new());
}

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "debug");

    info!("V8 version: {}", js_sys::version());

    env_logger::init_from_env(env);

    let mut file = File::open("fly.toml").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let decoded: Config = toml::from_str(&contents).unwrap();

    println!("toml: {:?}", decoded);

    for (name, app) in decoded.apps.unwrap().iter() {
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

    let addr = ([127, 0, 0, 1], decoded.port.unwrap()).into();

    let ln = tokio::net::TcpListener::bind(&addr).expect("unable to bind TCP listener");

    let server = ln.incoming().map_err(|_| unreachable!()).for_each(|_sock| {
        // hyper::server::conn::Http::new().serve_connection(
        //     sock,
        //     FlyServer {
        //         scheme: "http".to_string(),
        //     },
        // )
        Ok(())
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
