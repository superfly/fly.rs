extern crate cbindgen;
extern crate cc;

use std::env;
use std::path::Path;
use std::process::Command;

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
    ..Default::default()
  };

  cbindgen::Builder::new()
    .with_crate(crate_dir)
    .with_documentation(true)
    .with_config(config)
    .generate()
    .expect("Unable to generate bindings")
    .write_to_file("binding.h");

  cc::Build::new()
    .file("binding.cc")
    .include(Path::new("third_party/v8/include/"))
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

  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();
  let out_dir = env::var("OUT_DIR").unwrap();

  Command::new("flatc")
    .arg("--rust")
    .arg("-o")
    .arg(out_dir)
    .arg(format!("{}/msg.fbs", crate_root))
    .spawn()
    .unwrap();

  Command::new("flatc")
    .arg("--ts")
    .arg("--no-fb-import")
    .arg("--gen-mutable")
    .arg("-o")
    .arg(format!("{}/fly/packages/v8env/src", crate_root))
    .arg(format!("{}/msg.fbs", crate_root))
    .spawn()
    .unwrap();
}
