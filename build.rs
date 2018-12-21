use std::process::Command;

fn main() {
  let output = Command::new("git").args(&["rev-parse", "HEAD"]).output().unwrap();
  let git_hash = String::from_utf8(output.stdout).unwrap();
  println!("cargo:rustc-env=GIT_HASH={}", git_hash[..7].to_string());

  println!("cargo:rerun-if-changed=msg.fbs");
  let status = Command::new("./scripts/fbs.sh")
    .status()
    .expect("failed to generated flatbuffer messages");
  assert!(status.success());
}
