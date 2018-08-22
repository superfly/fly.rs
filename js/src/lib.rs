#![feature(test)]

extern crate js_sys;

use js_sys::*;
use std::mem;

use std::sync::Once;

#[macro_use]
extern crate log;
extern crate env_logger;

#[macro_use]
mod macros;

use std::ffi::{CStr, CString};
use std::slice;
use std::str;

#[derive(Debug)]
pub struct Runtime(*const js_runtime);
unsafe impl Send for Runtime {}

impl Runtime {
  pub fn new(snapshot: c_string) -> Runtime {
    unsafe { Runtime(js_runtime_new(snapshot)) }
  }

  pub fn global(&self) -> Object {
    let g = Value::from_raw(unsafe { js_global(self.as_raw()) });
    g.into_object().unwrap()
  }

  pub fn as_raw(&self) -> *const js_runtime {
    self.0
  }

  pub fn from_raw(raw: *const js_runtime) -> Self {
    Runtime(raw)
  }

  pub fn release(&self) {
    unsafe {
      js_runtime_release(self.as_raw());
    }
  }
}

pub struct Value(*const js_value);
unsafe impl Send for Value {}

impl Value {
  pub fn to_string(&self) -> ::std::string::String {
    let len = unsafe { js_value_string_utf8_len(self.as_raw()) } as usize;
    let mut buf = vec![0u8; len];
    unsafe {
      let ptr = mem::transmute(buf.as_mut_ptr());
      js_value_string_write_utf8(self.as_raw(), ptr, len as i32);
      ::std::string::String::from_utf8_unchecked(buf)
    }
  }

  pub fn to_i64(&self) -> i64 {
    unsafe { js_value_to_i64(self.as_raw()) }
  }

  pub fn call(&self, rt: Runtime) -> Self {
    Value::from_raw(unsafe { js_value_call(rt.as_raw(), self.as_raw()) })
  }

  pub fn is_object(&self) -> bool {
    unsafe { js_value_is_object(self.as_raw()) }
  }

  pub fn into_object(self) -> Option<Object> {
    if !self.is_object() {
      return None;
    }
    Some(Object(self))
  }

  pub fn is_function(&self) -> bool {
    unsafe { js_value_is_function(self.as_raw()) }
  }

  pub fn into_function(self) -> Option<Function> {
    if !self.is_function() {
      return None;
    }
    Some(Function(self))
  }

  pub fn from_raw(raw: *const js_value) -> Value {
    Value(raw)
  }

  pub fn as_raw(&self) -> *const js_value {
    self.0
  }
}

impl Drop for Value {
  fn drop(&mut self) {
    debug!("Dropping value!");
    unsafe { js_value_release(self.as_raw()) }
  }
}

pub struct Object(Value);
impl Object {
  pub fn set(&self, name: &str, v: &Value) -> bool {
    unsafe {
      js_value_set(
        self.as_raw(),
        CString::new(name).unwrap().as_ptr(),
        v.as_raw(),
      )
    }
  }

  pub fn as_raw(&self) -> *const js_value {
    self.0.as_raw()
  }
}

type Callback = extern "C" fn(*const js_callback_info);

pub struct Function(Value);
impl Function {
  pub fn new(rt: &Runtime, cb: Callback) -> Self {
    Value::from_raw(unsafe { js_function_new(rt.as_raw(), cb) })
      .into_function()
      .unwrap()
  }

  pub fn value(&self) -> &Value {
    &self.0
  }
}

pub struct CallbackInfo(*const js_callback_info);
impl CallbackInfo {
  pub fn length(&self) -> i32 {
    unsafe { js_callback_info_length(self.as_raw()) }
  }
  pub fn get(&self, i: i32) -> Option<Value> {
    let v = unsafe { js_callback_info_get(self.as_raw(), i) };
    if v.is_null() {
      return None;
    }
    Some(Value::from_raw(v))
  }
  pub fn runtime(&self) -> Runtime {
    Runtime::from_raw(unsafe { js_callback_info_runtime(self.as_raw()) })
  }
  pub fn as_raw(&self) -> *const js_callback_info {
    self.0
  }
  pub fn from_raw(raw: *const js_callback_info) -> Self {
    CallbackInfo(raw)
  }
}

static INIT: Once = Once::new();

pub fn init() {
  INIT.call_once(|| unsafe { js_init() });
}

pub mod sys {
  pub use js_sys::*;
}

#[cfg(test)]
mod tests {
  use super::*;

  extern crate test;
  use self::test::Bencher;

  #[bench]
  fn bench_to_string(b: &mut Bencher) {
    init();
    let snap = unsafe {
      let c_to_print = CString::new("").unwrap();
      js_snapshot_create(c_to_print.as_ptr())
    };
    let rt = Runtime::new(snap);

    let v = Value::from_raw(unsafe { js_global(rt.as_raw()) });

    b.iter(|| {
      v.to_string();
    })
  }

  #[bench]
  fn bench_get_global(b: &mut Bencher) {
    init();
    let snap = unsafe {
      let c_to_print = CString::new("").unwrap();
      js_snapshot_create(c_to_print.as_ptr())
    };
    let rt = Runtime::new(snap);
    b.iter(|| {
      rt.global();
    })
  }
}
