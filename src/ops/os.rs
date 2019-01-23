use crate::msg;
use crate::runtime::Runtime;
use crate::utils::*;
use flatbuffers::FlatBufferBuilder;
use futures::Future;
use libfly::*;

pub fn op_exit(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_os_exit().unwrap();

    std::process::exit(msg.code())
}
