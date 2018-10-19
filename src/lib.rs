extern crate flatbuffers;
extern crate hyper;

#[macro_use]
extern crate log;

#[macro_use]
extern crate futures;
extern crate libfly;
extern crate tokio;
extern crate url;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate lazy_static_include;

#[macro_use]
extern crate serde_derive;

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};

pub static NEXT_EVENT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub mod config;
pub mod errors;
pub mod msg;
pub mod ops;
pub mod redis_stream;
pub mod runtime;
pub mod utils;
