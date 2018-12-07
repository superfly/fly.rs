use std::process::Command;

fn main() {
  // let mut conf = cmake::Config::new("third_party/flatbuffers");
  // conf.generator("Unix Makefiles");
  // eprintln!("built in: {:?}", conf.build());

  println!("cargo:rerun-if-changed=msg.fbs");
  let status = Command::new("./scripts/fbs.sh")
    .status()
    .expect("failed to generated flatbuffer messages");
  assert!(status.success());
}
