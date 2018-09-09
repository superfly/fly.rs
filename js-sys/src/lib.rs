extern crate libc;
use libc::{c_char, c_int, c_void, size_t};

use std::ffi::CStr;

#[repr(C)]
pub struct fly_buf {
  pub ptr: *const u8,
  pub len: usize,
}

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
#[derive(PartialEq)]
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
  pub fn js_runtime_new(data: *const c_void) -> *const js_runtime;
  pub fn js_get_data(rt: *const js_runtime) -> *const c_void;
  pub fn js_set_response(rt: *const js_runtime, buf: fly_bytes);
  pub fn js_send(rt: *const js_runtime, buf: fly_bytes) -> c_int;
  // pub fn js_isolate_new(s: fly_buf) -> Isolate;
  pub fn js_snapshot_create(s: *const c_char) -> fly_buf;
  pub fn js_runtime_heap_statistics(rt: *const js_runtime) -> js_heap_stats;
  // pub fn js_context_new(iso: Isolate) -> PersistentContext;
  // pub fn js_global(rt: *const js_runtime) -> *const js_value;
  // pub fn js_value_set(v: *const js_value, name: *const c_char, prop: *const js_value) -> bool;
  // pub fn js_function_new(
  //   rt: *const js_runtime,
  //   cb: extern "C" fn(*const js_callback_info),
  // ) -> *const js_value;

  pub fn js_eval(rt: *const js_runtime, filename: *const c_char, code: *const c_char);

// pub fn js_callback_info_runtime(info: *const js_callback_info) -> *const js_runtime;
// pub fn js_callback_info_length(info: *const js_callback_info) -> i32;
// pub fn js_callback_info_get(info: *const js_callback_info, i: i32) -> *const js_value;
// pub fn js_value_to_string(v: *const js_value) -> fly_buf;
// pub fn js_value_is_function(v: *const js_value) -> bool;
// pub fn js_value_call(rt: *const js_runtime, v: *const js_value) -> *const js_value;
// pub fn js_value_to_i64(v: *const js_value) -> i64;

// pub fn js_runtime_release(rt: *const js_runtime);
// pub fn js_value_release(v: *const js_value);

// pub fn js_value_is_object(v: *const js_value) -> bool;

// pub fn js_value_string_utf8_len(v: *const js_value) -> c_int;
// pub fn js_value_string_write_utf8(v: *const js_value, buf: *mut c_char, len: c_int);
}
