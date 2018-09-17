extern crate libc;
use self::libc::{c_char, c_int, c_void, size_t};

use std::ffi::CStr;

pub mod msg;
pub use msg::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct fly_buf {
  pub ptr: *const c_char,
  pub len: size_t,
}
unsafe impl Send for fly_buf {}
unsafe impl Sync for fly_buf {}

#[repr(C)]
pub struct js_runtime {
  _unused: [u8; 0],
}

#[repr(C)]
pub struct js_value {
  _unused: [u8; 0],
}

#[repr(C)]
pub struct js_callback_info {
  _unused: [u8; 0],
}

#[repr(C)]
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct fly_bytes {
  pub alloc_ptr: *mut u8,
  pub alloc_len: usize,
  pub data_ptr: *mut u8,
  pub data_len: usize,
}

#[repr(C)]
#[derive(Debug)]
pub struct js_heap_stats {
  pub total_heap_size: size_t,
  pub total_heap_size_executable: size_t,
  pub total_physical_size: size_t,
  pub total_available_size: size_t,
  pub used_heap_size: size_t,
  pub heap_size_limit: size_t,
  pub malloced_memory: size_t,
  pub peak_malloced_memory: size_t,
  pub number_of_native_contexts: size_t,
  pub number_of_detached_contexts: size_t,
  pub does_zap_garbage: bool,
}

pub fn version() -> String {
  unsafe { CStr::from_ptr(js_version()).to_string_lossy().into_owned() }
}

extern "C" {
  pub fn js_init(natives: fly_buf, snapshot: fly_buf);
  pub fn js_version() -> *const c_char;
  pub fn js_runtime_new(snapshot: fly_buf, data: *mut c_void) -> *const js_runtime;
  pub fn js_get_data(rt: *const js_runtime) -> *const c_void;
  pub fn js_set_response(rt: *const js_runtime, cmd_id: c_int, argc: c_int, argv: *const Value);
  pub fn js_set_return_value(rt: *const js_runtime, v: Value);
  pub fn js_send(rt: *const js_runtime, kind: MessageKind, cmd: Value) -> Value;
  pub fn js_runtime_heap_statistics(rt: *const js_runtime) -> js_heap_stats;
  pub fn js_create_snapshot(filename: *const c_char, code: *const c_char) -> fly_buf;

  pub fn js_eval(rt: *const js_runtime, filename: *const c_char, code: *const c_char);
}

extern "C" {
  pub fn testy(rt: *const js_runtime, msg: Message) -> Value;
}

#[repr(C)]
#[derive(Debug)]
pub struct KeyValue {
  pub key: *const c_char,
  pub val: *const Value,
}
unsafe impl Send for KeyValue {}
unsafe impl Sync for KeyValue {}

#[repr(C, u8)]
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Value {
  None,
  Int32(i32),
  String(*const c_char),
  Object { len: i32, pairs: *const KeyValue },
  Array { len: size_t, values: *const Value },
  ArrayBuffer(fly_buf),
  Uint8Array(fly_bytes),
}

unsafe impl Send for Value {}
unsafe impl Sync for Value {}
