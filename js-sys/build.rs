extern crate cc;

use std::env;

fn main() {
  let cp = env::var("CARGO_MANIFEST_DIR").unwrap();

  cc::Build::new()
    .file("src/binding.cc")
    .cpp(true)
    .warnings(true)
    .flag("--std=c++11")
    .compile("libflyv8.a");

  println!(
    "cargo:rustc-link-search=native={}/../third_party/v8/out.gn/x64.debug/",
    cp
  );

  println!("cargo:rustc-link-lib=dylib=v8");
  println!("cargo:rustc-link-lib=dylib=v8_libbase");
  println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  println!("cargo:rustc-link-lib=dylib=icui18n");
  println!("cargo:rustc-link-lib=dylib=icuuc");
}
