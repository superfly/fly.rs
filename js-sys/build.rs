extern crate cc;

fn main() {
  cc::Build::new()
    .file("src/binding.cc")
    .cpp(true)
    .warnings(true)
    .flag("--std=c++11")
    .flag("-fkeep-inline-functions")
    .compile("libflyv8.a");
  println!("cargo:rustc-link-search=native=/Users/jerome/v8/v8/out/x64.debug/");

  println!("cargo:rustc-link-lib=dylib=v8");
  println!("cargo:rustc-link-lib=dylib=v8_libbase");
  println!("cargo:rustc-link-lib=dylib=v8_libplatform");
  println!("cargo:rustc-link-lib=dylib=icui18n");
  println!("cargo:rustc-link-lib=dylib=icuuc");
}
