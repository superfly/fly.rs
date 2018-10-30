use std::process::Command;

fn main() {
  Command::new("./scripts/fbs.sh").spawn().unwrap();
}
