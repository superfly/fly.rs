use crate::msg;
use crate::runtime::Runtime;
use crate::utils::*;
use libfly::*;

pub fn op_exit(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    if let Err(e) = rt.permissions.check_os() {
        return odd_future(e);
    }

    let msg = base.msg_as_os_exit().unwrap();

    std::process::exit(msg.code())
}
