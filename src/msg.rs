extern crate flatbuffers;
// import the generated code
#[path = "./msg_generated.rs"]
mod msg_generated;

pub use self::msg_generated::*;
