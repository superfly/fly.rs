use std::process::Command;

fn main() {
  println!("cargo:rerun-if-changed=msg.fbs");
  Command::new("./scripts/fbs.sh").spawn().unwrap();
}
