#![feature(duration_as_u128)]

use std::ffi::{CStr, CString};

extern crate js;
use js::sys::{js_callback_info, js_eval, js_snapshot_create, js_version};

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate tokio;
use env_logger::Env;
use tokio::prelude::*;
use tokio::timer::Delay;

use std::time::{Duration, Instant};

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "debug");

    env_logger::init_from_env(env);

    js::init();

    unsafe {
        info!("Hello, world! {:?}", CStr::from_ptr(js_version()));
        let c_to_print = CString::new("").unwrap();
        let snap = js_snapshot_create(c_to_print.as_ptr());
        info!("created snapshot");
        let rt = js::Runtime::new(snap);
        info!("inited {:?}", rt);

        let g = rt.global();
        println!("got global");
        let f = js::Function::new(&rt, set_timeout);
        println!("got fn");
        let s = Instant::now();
        println!("worked? {}", g.set("setTimeout", f.value()));
        info!("set fn took: {}ns", s.elapsed().as_nanos());
        g.set("log", js::Function::new(&rt, log).value());

        tokio::run(future::lazy(move || -> Result<(), ()> {
            // info!("global! {:?}", g);
            js_eval(
                rt.as_raw(),
                CString::new(
                    "(function l() {
                        log('hello l')
                        setTimeout(l, 2000)
                    })()",
                ).unwrap()
                .as_ptr(),
            );
            Ok(())
        }))
    }
}

extern "C" fn set_timeout(raw_info: *const js_callback_info) {
    info!("set timeout called!");
    let info = js::CallbackInfo::from_raw(raw_info);
    let rt = info.runtime();
    if let Some(fnv) = info.get(0) {
        info!("got a fn: {}", fnv.to_string());
        if let Some(msv) = info.get(1) {
            info!("got some ms! {}", msv.to_i64());
            let when = Instant::now() + Duration::from_millis(msv.to_i64() as u64);
            let task = Delay::new(when)
                .and_then(move |_| {
                    info!("in delayed closure");
                    let res = fnv.call(rt);
                    info!("call got: {}", res.to_string());
                    Ok(())
                }).map_err(|e| panic!("delay errored; err={:?}", e));

            tokio::spawn(task);
        }
    }
    info!("set_timeout done");
}

extern "C" fn log(raw_info: *const js_callback_info) {
    let info = js::CallbackInfo::from_raw(raw_info);
    for i in 0..info.length() {
        if let Some(v) = info.get(i) {
            info!("log: {}", v.to_string());
        }
    }
}
