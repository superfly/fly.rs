extern crate cbindgen;
extern crate cc;

use std::env;
use std::path::PathBuf;

fn main() {
  let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
  let crate_dir2 = crate_dir.clone();

  let config = cbindgen::Config {
    autogen_warning: Some(String::from("// Auto-generated, don't edit!")),
    include_version: true,
    include_guard: Some(String::from("libfly")),
    includes: vec![String::from("runtime.h")],
    export: cbindgen::ExportConfig {
      exclude: vec![String::from("js_runtime")],
      ..Default::default()
    },
    language: cbindgen::Language::Cxx,
    // enumeration: cbindgen::EnumConfig {
    //   derive_helper_methods: true,
    //   ..Default::default()
    // },
    // structure: cbindgen::StructConfig {
    //   derive_eq: true,
    //   ..Default::default()
    // },
    ..Default::default()
  };

  cbindgen::Builder::new()
    // .with_src(&PathBuf::from(format!("{}/src", crate_dir)))
    .with_crate(crate_dir)
    .with_documentation(true)
    .with_config(config)
    .generate()
    // .generate_with_config(&crate_dir, config)
    .expect("Unable to generate bindings")
    .write_to_file("binding.h");

  cc::Build::new()
    .file("binding.cc")
    .cpp(true)
    .warnings(true)
    .flag("--std=c++11")
    .compile("libfly.a");

  println!(
    "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.debug/",
    crate_dir2
  );

  println!("cargo:rustc-link-lib=dylib=v8");
  println!("cargo:rustc-link-lib=dylib=v8_libbase");
  println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  println!("cargo:rustc-link-lib=dylib=icui18n");
  println!("cargo:rustc-link-lib=dylib=icuuc");

  // RELEASE, I THINK

  // println!(
  //   "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.release/obj",
  //   crate_dir
  // );
  // println!(
  //   "cargo:rustc-link-search=native={}/third_party/v8/out.gn/x64.release/obj/third_party/icu",
  //   crate_dir
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
