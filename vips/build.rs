extern crate bindgen;
extern crate pkg_config;

use bindgen::builder;
use std::env;
use std::path::PathBuf;

fn main() {
  println!("cargo:rerun-if-changed=wrapper.h");

  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();

  let glib = pkg_config::Config::new()
    .atleast_version("2.0.0")
    .probe("gobject-2.0")
    .unwrap();

  // Configure and generate bindings.
  let mut bindings = builder()
    .header("wrapper.h")
    .ctypes_prefix("libc")
    .rustified_enum(".*")
    .blacklist_type("max_align_t")
    .blacklist_type("FP_NAN")
    .blacklist_type("FP_INFINITE")
    .blacklist_type("FP_ZERO")
    .blacklist_type("FP_SUBNORMAL")
    .blacklist_type("FP_NORMAL")
    .derive_debug(true)
    .derive_hash(true)
    .derive_eq(true)
    .derive_partialeq(true)
    .clang_arg("-isystem/usr/include")
    .clang_arg("-isysroot/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk");

  // .whitelist_type("vips_.*")
  // .whitelist_function("vips_.*")
  // .whitelist_var("vips_.*")
  // .whitelist_type("VIPS_.*")
  // .whitelist_function("VIPS_.*")
  // .whitelist_var("VIPS_.*")
  // .whitelist_type("g_object.*")
  // .whitelist_function("g_object.*")
  // .whitelist_var("g_object.*")
  // .whitelist_type("g_value.*")
  // .whitelist_function("g_value.*")
  // .whitelist_var("g_value.*")

  bindings = bindings.clang_arg(format!(
    "-I{}{}",
    crate_root, "/third_party/libvips/libvips/include"
  ));
  for ipath in glib.include_paths {
    bindings = bindings.clang_arg(format!("-I{}", ipath.to_string_lossy()));
  }

  for lpath in glib.link_paths {
    bindings = bindings.clang_arg(format!("-L{}", lpath.to_string_lossy()));
  }

  for lib in glib.libs {
    println!("cargo:rustc-link-lib=dylib={}", lib);
  }

  // Write the generated bindings to an output file.
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  bindings
    .generate()
    .unwrap()
    .write_to_file(out_path.join("bindings.rs"))
    .expect("Couldn't write bindings!");

  println!(
    "cargo:rustc-link-search=native={}/third_party/libvips/libvips/.libs/",
    crate_root
  );

  println!("cargo:rustc-link-lib=dylib=vips");
}
