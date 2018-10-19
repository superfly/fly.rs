use flatbuffers::FlatBufferBuilder;
use libfly::*;
use msg;

use errors::FlyError;

use runtime::{Buf, Op};

use futures::future;

pub fn serialize_response(
  cmd_id: u32,
  builder: &mut FlatBufferBuilder,
  mut args: msg::BaseArgs,
) -> Buf {
  args.cmd_id = cmd_id;
  let base = msg::Base::create(builder, &args);
  msg::finish_base_buffer(builder, base);
  let data = builder.finished_data();
  let vec = data.to_vec();
  Some(vec.into_boxed_slice())
}

pub fn build_error(cmd_id: u32, err: FlyError) -> Buf {
  let builder = &mut FlatBufferBuilder::new();
  let errmsg_offset = builder.create_string(&format!("{}", err));
  serialize_response(
    cmd_id,
    builder,
    msg::BaseArgs {
      error: Some(errmsg_offset),
      error_kind: err.kind(),
      ..Default::default()
    },
  )
}

pub fn null_buf() -> fly_buf {
  fly_buf {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: 0 as *mut u8,
    data_len: 0,
  }
}

pub fn ok_future(buf: Buf) -> Box<Op> {
  Box::new(future::ok(buf))
}

pub fn odd_future(err: FlyError) -> Box<Op> {
  Box::new(future::err(err))
}

pub fn fly_buf_from(x: Box<[u8]>) -> fly_buf {
  let len = x.len();
  let ptr = Box::into_raw(x);
  fly_buf {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: ptr as *mut u8,
    data_len: len,
  }
}
