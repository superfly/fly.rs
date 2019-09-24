extern crate libc;
use libc::{c_char, c_int, c_void, size_t};

use std::ffi::CStr;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct fly_simple_buf {
    pub ptr: *const c_char,
    pub len: c_int,
}
unsafe impl Send for fly_simple_buf {}
unsafe impl Sync for fly_simple_buf {}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct fly_buf {
    pub alloc_ptr: *mut u8,
    pub alloc_len: usize,
    pub data_ptr: *mut u8,
    pub data_len: usize,
}
unsafe impl Send for fly_buf {}
unsafe impl Sync for fly_buf {}

#[repr(C)]
pub struct js_runtime_options {
    pub snapshot: fly_simple_buf,
    pub data: *mut c_void,
    pub recv_cb: RecvCb,
    pub print_cb: PrintCb,
    pub resolve_cb: ResolveCb,
    pub soft_memory_limit: size_t,
    pub hard_memory_limit: size_t,
}

#[repr(C)]
pub struct js_runtime {
    _unused: [u8; 0],
}

#[repr(C)]
pub struct js_module_data {
    pub origin_url: *const c_char,
    pub source_map_url: *const c_char,
    pub is_wasm: bool,
    pub source_code: fly_simple_buf,
}

#[repr(C)]
pub struct js_compiled_module {
    pub hash: c_int,
    pub data: js_module_data,
    pub ptr: *mut c_void,
}

#[repr(C)]
pub struct js_compile_module_result {
    pub compiled_module: js_compiled_module,
    pub success: bool,
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
    pub externally_allocated: size_t,
}

pub fn version() -> String {
    unsafe { CStr::from_ptr(js_version()).to_string_lossy().into_owned() }
}

type RecvCb = unsafe extern "C" fn(rt: *const js_runtime, buf: fly_buf, data_buf: fly_buf);
type PrintCb = unsafe extern "C" fn(rt: *const js_runtime, lvl: i8, msg: *const c_char);
type ResolveCb = unsafe extern "C" fn(
    rt: *const js_runtime,
    specifier: *const c_char,
    referer_identity_hash: i32,
) -> js_compiled_module;

extern "C" {
    pub fn js_init();
    pub fn js_version() -> *const c_char;
    pub fn js_runtime_new(options: js_runtime_options) -> *const js_runtime;
    pub fn js_runtime_dispose(rt: *const js_runtime) -> *const c_void;
    pub fn js_runtime_run_micro_tasks(rt: *const js_runtime) -> *const c_void;
    pub fn js_get_data(rt: *const js_runtime) -> *const c_void;
    pub fn js_set_response(rt: *const js_runtime, buf: fly_buf);
    pub fn js_send(rt: *const js_runtime, buf: fly_buf, raw: fly_buf) -> c_int;
    pub fn js_runtime_heap_statistics(rt: *const js_runtime) -> js_heap_stats;
    pub fn js_create_snapshot(filename: *const c_char, code: *const c_char) -> fly_simple_buf;
    pub fn js_dump_heap_snapshot(rt: *const js_runtime, filename: *const c_char) -> bool;

    pub fn js_eval(rt: *const js_runtime, filename: *const c_char, code: *const c_char) -> bool;
    pub fn js_run_module(rt: *const js_runtime, module_data: js_compiled_module) -> bool;
    pub fn js_compile_module(
        rt: *const js_runtime,
        module_data: js_module_data,
    ) -> js_compile_module_result;
}

#[no_mangle]
pub extern "C" fn free_fly_buf(buf: fly_buf) {
    unsafe { Box::from_raw(buf.data_ptr) };
}
