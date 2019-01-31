use crate::msg;

use crate::runtime::Runtime;
use libfly::*;

use crate::utils::*;

use std::slice;

pub fn op_stream_chunk(rt: &mut Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
    debug!("handle stream chunk {:?}", raw);
    let msg = base.msg_as_stream_chunk().unwrap();
    let stream_id = msg.id();

    let mut streams = rt.streams.lock().unwrap();
    if raw.data_len > 0 {
        match streams.get_mut(&stream_id) {
            Some(sender) => {
                let bytes = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();
                match sender.unbounded_send(bytes.to_vec()) {
                    Err(e) => error!("error sending chunk: {}", e),
                    _ => debug!("chunk streamed"),
                }
            }
            None => unimplemented!(),
        };
    }
    if msg.done() {
        streams.remove(&stream_id);
    }

    ok_future(None)
}
