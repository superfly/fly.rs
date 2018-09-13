extern crate cc;

use std::env;

fn main() {
  let cp = env::var("CARGO_MANIFEST_DIR").unwrap();

  cc::Build::new()
    .file("src/libfly/binding.cc")
    .cpp(true)
    .warnings(true)
    .flag("--std=c++11")
    .compile("libfly.a");

  println!(
    "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.debug/",
    cp
  );

  println!("cargo:rustc-link-lib=dylib=v8");
  println!("cargo:rustc-link-lib=dylib=v8_libbase");
  println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  println!("cargo:rustc-link-lib=dylib=icui18n");
  println!("cargo:rustc-link-lib=dylib=icuuc");

  // RELEASE, I THINK

  // println!(
  //   "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.release/obj",
  //   cp
  // );
  // println!(
  //   "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.release/obj/third_party/icu",
  //   cp
  // );

  // // println!("cargo:rustc-link-lib=dylib=v8_init");
  // // println!("cargo:rustc-link-lib=dylib=v8_initializers");
  // println!("cargo:rustc-link-lib=dylib=v8_libsampler");
  // println!("cargo:rustc-link-lib=dylib=v8_external_snapshot");
  // println!("cargo:rustc-link-lib=dylib=v8_base");
  // println!("cargo:rustc-link-lib=dylib=v8_nosnapshot");
  // println!("cargo:rustc-link-lib=dylib=v8_libbase");
  // println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  // println!("cargo:rustc-link-lib=dylib=icui18n");
  // println!("cargo:rustc-link-lib=dylib=icuuc");
}
