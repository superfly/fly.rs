use std::env;
use std::process::Command;

fn main() {
  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();

  println!("cargo:rerun-if-changed=msg.fbs");

  Command::new("flatc")
    .arg("--rust")
    .arg("-I")
    .arg(format!("{}/src/ops", crate_root))
    .arg("-o")
    .arg(format!("{}/src", crate_root))
    .arg(format!("{}/msg.fbs", crate_root))
    .spawn()
    .unwrap();

  Command::new("flatc")
    .arg("--ts")
    .arg("--no-fb-import")
    .arg("--gen-mutable")
    .arg("-I")
    .arg(format!("{}/src/ops", crate_root))
    .arg("-o")
    .arg(format!("{}/v8env/src", crate_root))
    .arg(format!("{}/msg.fbs", crate_root))
    .spawn()
    .unwrap();
}
