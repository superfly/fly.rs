use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();
  let out_dir = env::var("OUT_DIR").unwrap();

  println!("cargo:rerun-if-changed=msg.fbs");

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
