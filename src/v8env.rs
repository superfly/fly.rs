use libfly::*;
use std::ffi::CString;
use std::fs::File;
use std::io::Read;
use std::slice;

#[cfg(not(debug_assertions))]
const V8ENV_SNAPSHOT: &'static [u8] = include_bytes!("../v8env.bin");

#[cfg(debug_assertions)]
lazy_static! {
  static ref V8ENV_SNAPSHOT: Box<[u8]> = {
    let filename = "v8env/dist/v8env.js";
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let snap = unsafe {
      let cfilename = CString::new(filename).unwrap();
      let ccontents = CString::new(contents).unwrap();
      js_create_snapshot(cfilename.as_ptr(), ccontents.as_ptr())
    };
    let bytes: Vec<u8> =
      unsafe { slice::from_raw_parts(snap.ptr as *const u8, snap.len as usize) }.to_vec();
    bytes.into_boxed_slice()
  };
}

lazy_static! {
  pub static ref FLY_SNAPSHOT: fly_simple_buf = fly_simple_buf {
    ptr: V8ENV_SNAPSHOT.as_ptr() as *const i8,
    len: V8ENV_SNAPSHOT.len() as i32
  };
}

lazy_static_include_bytes!(pub V8ENV_SOURCEMAP, "v8env/dist/v8env.js.map");

lazy_static_include_str!(pub DEV_TOOLS_SOURCE, "v8env/dist/dev-tools.js");
