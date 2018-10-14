extern crate cbindgen;
extern crate cc;

use std::env;
use std::path::Path;

fn main() {
  let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

  println!("cargo:rerun-if-changed=binding.cc");
  println!("cargo:rerun-if-changed=src/lib.rs");
  println!("cargo:rerun-if-changed=third_party/v8/out.gn/x64.release/obj");

  // let config = cbindgen::Config {
  //   autogen_warning: Some(String::from("// Auto-generated, don't edit!")),
  //   include_version: true,
  //   include_guard: Some(String::from("libfly")),
  //   includes: vec![String::from("runtime.h")],
  //   export: cbindgen::ExportConfig {
  //     exclude: vec![String::from("js_runtime")],
  //     ..Default::default()
  //   },
  //   language: cbindgen::Language::Cxx,
  //   ..Default::default()
  // };

  // cbindgen::Builder::new()
  //   .with_crate(crate_dir.clone())
  //   .with_documentation(true)
  //   .with_config(config)
  //   .generate()
  //   .expect("Unable to generate bindings")
  //   .write_to_file("binding.h");

  cc::Build::new()
    .file("binding.cc")
    .include(Path::new("third_party/v8/include/"))
    .cpp(true)
    .static_flag(true)
    .extra_warnings(false)
    .flag("--std=c++14")
    .cpp_set_stdlib("c++")
    .compile("libfly.a");

  // DEBUG

  // println!(
  //   "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.debug/",
  //   crate_dir2
  // );

  // println!("cargo:rustc-link-lib=dylib=v8");
  // println!("cargo:rustc-link-lib=dylib=v8_libbase");
  // println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  // println!("cargo:rustc-link-lib=dylib=icui18n");
  // println!("cargo:rustc-link-lib=dylib=icuuc");

  // RELEASE, I THINK

  println!(
    "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.release/obj",
    crate_dir
  );
  println!("cargo:rustc-link-lib=static=v8_monolith");

  // if cfg!(any(target_os = "macos", target_os = "freebsd")) {
  // println!("cargo:rustc-link-lib=c++");
  // } else {
  //   println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
  //   println!("cargo:rustc-link-lib=static=c++");
  // }
}
