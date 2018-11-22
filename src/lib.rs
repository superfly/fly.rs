extern crate flatbuffers;
extern crate hyper;

extern crate serde;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

extern crate futures;
extern crate libfly;
extern crate tokio;
extern crate url;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate lazy_static_include;

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};

pub static NEXT_EVENT_ID: AtomicUsize = ATOMIC_USIZE_INIT;
pub static NEXT_FUTURE_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub mod errors;
pub mod msg;
pub mod ops;
pub mod runtime;

pub mod utils;

pub mod cache_store;
pub mod data_store;

pub mod settings;

mod compiler;
mod postgres_data;
mod redis_cache;
mod sqlite_cache;
mod sqlite_data;
