extern crate flatbuffers;
extern crate futures;
extern crate js_sys;
extern crate tokio;

extern crate msg as msgfbs;
pub use msgfbs::fly as msg;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

pub mod config;
pub mod runtime;
