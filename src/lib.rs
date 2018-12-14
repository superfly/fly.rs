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

pub mod errors;
pub mod msg;
pub mod ops;
pub mod runtime;

pub mod utils;

pub mod cache_store;
pub mod data_store;
pub mod fs_store;

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
mod sqlite_cache;
mod sqlite_data;
