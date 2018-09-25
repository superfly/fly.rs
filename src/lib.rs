extern crate flatbuffers;
#[macro_use]
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate lazy_static;

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};

pub static NEXT_STREAM_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub mod libfly;
pub mod msg;
pub mod redis_stream;
pub mod runtime;
