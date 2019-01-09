// I know this is 2018 edition, but having these globally is very useful.
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate lazy_static_include;

#[macro_use]
extern crate prometheus;

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};

pub static NEXT_EVENT_ID: AtomicUsize = ATOMIC_USIZE_INIT;
pub static NEXT_FUTURE_ID: AtomicUsize = ATOMIC_USIZE_INIT;

use std::os::raw::c_uint;

#[no_mangle]
pub unsafe extern "C" fn c_get_next_stream_id() -> c_uint {
    get_next_stream_id()
}

pub fn get_next_stream_id() -> u32 {
    NEXT_EVENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u32
}

pub mod errors;
pub mod msg;
pub mod ops;
pub mod runtime;

pub mod utils;

pub mod cache_store;
pub mod data_store;
pub mod fs_store;
pub mod acme_store;

pub mod settings;

pub mod runtime_selector;
pub use crate::runtime_selector::{RuntimeSelector, SelectorError};

pub mod dns_server;
pub mod fixed_runtime_selector;
pub mod http_server;

pub mod metrics;

mod compiler;
mod disk_fs;
mod postgres_data;
mod redis_cache;
mod redis_fs;
mod redis_acme;
mod sqlite_cache;
mod sqlite_data;

