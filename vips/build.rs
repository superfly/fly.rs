extern crate bindgen;
extern crate pkg_config;

use bindgen::builder;
use std::env;
use std::path::PathBuf;

fn main() {
  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();

  let glib = pkg_config::Config::new()
    .atleast_version("2.0.0")
    .probe("gobject-2.0")
    .unwrap();

  // Configure and generate bindings.
  let mut bindings = builder()
    .header("wrapper.h")
    .whitelist_type("vips_.*")
    .whitelist_function("vips_.*")
    .whitelist_var("vips_.*")
    .whitelist_type("g_object.*")
    .whitelist_function("g_object.*")
    .whitelist_var("g_object.*")
    .derive_debug(true)
    .derive_hash(true)
    .derive_eq(true)
    .derive_partialeq(true);

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
