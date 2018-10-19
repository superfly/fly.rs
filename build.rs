use std::env;
use std::process::Command;

fn main() {
  let crate_root = env::var("CARGO_MANIFEST_DIR").unwrap();

  Command::new("./scripts/fbs.sh").spawn().unwrap();
}
