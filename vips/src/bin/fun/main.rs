extern crate libc;
extern crate vips;
use vips::*;

use std::env;
use std::ffi::{CStr, CString};
use std::ptr;

fn main() {
  let args: Vec<String> = env::args().collect();
  println!("args: {:?}", args);
  init(args[0].clone());
  let img = Image::from_file(args[1].clone()).unwrap();
  println!("image width: {}", img.width().unwrap());
  // unsafe {

  //   assert!(vips_init(CString::new(args[0].as_str()).unwrap().as_ptr()) == 0);
  //   let img = vips_image_new_from_file(
  //     CString::new(args[1].as_str()).unwrap().as_ptr(),
  //     ptr::null() as *const libc::c_int,
  //   );
  //   let err_buf = vips_error_buffer();
  //   println!(
  //     "error? {}, {}",
  //     err_buf.is_null(),
  //     CStr::from_ptr(err_buf).to_string_lossy().into_owned()
  //   );
  //   println!("width: {}", vips_image_get_width(img));
  // };
}
