use libfly::*;

use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::slice;

use std::os::raw::c_uint;

#[no_mangle]
pub unsafe extern "C" fn c_get_next_stream_id() -> c_uint {
    0
}

fn main() {
    unsafe { js_init() };

    let args: Vec<String> = env::args().collect();
    let filename = args[1].as_str();
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let cfilename = CString::new(filename).unwrap();
    let ccontents = CString::new(contents).unwrap();
    let snap = unsafe { js_create_snapshot(cfilename.as_ptr(), ccontents.as_ptr()) };

    let bytes: Vec<u8> =
        unsafe { slice::from_raw_parts(snap.ptr as *const u8, snap.len as usize) }.to_vec();

    let out = &args[2];
    let mut outfile = File::create(&Path::new(out.as_str())).unwrap();

    outfile.write_all(bytes.as_slice()).unwrap();
}
