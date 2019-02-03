use std::process::Command;

fn main() {
    let output = Command::new("sh").arg("./scripts/build-version.sh").output().unwrap();
    println!("cargo:rustc-env=BUILD_VERSION={}", String::from_utf8(output.stdout).unwrap());
}