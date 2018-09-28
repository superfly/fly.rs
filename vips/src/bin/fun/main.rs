extern crate libc;
extern crate vips;
use vips::*;

use std::env;

fn main() {
  let args: Vec<String> = env::args().collect();
  println!("args: {:?}", args);
  init(args[0].clone());
  let img = Image::from_file(args[1].clone()).unwrap();
  println!("image width: {}", img.width().unwrap());
}
