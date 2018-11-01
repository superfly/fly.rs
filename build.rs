use std::process::Command;

fn main() {
  println!("cargo:rerun-if-changed=msg.fbs");
  let status = Command::new("./scripts/fbs.sh")
    .status()
    .expect("failed to generated flatbuffer messages");
  assert!(status.success());
}
