use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
  let out_dir = env::var("OUT_DIR").unwrap();

  // note that there are a number of downsides to this approach, the comments
  // below detail how to improve the portability of these commands.
  let cargo_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
  Command::new("flatc")
    .args(&["--rust", "-o", &out_dir])
    .arg(&format!("{}/msg.fbs", cargo_dir))
    .status()
    .unwrap();
}
