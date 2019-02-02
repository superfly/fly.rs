use std::env;

fn main() {
    println!("cargo:rustc-env=BUILD_VERSION={}", env::var("BUILD_VERSION").unwrap_or(env::var("TRAVIS_COMMIT").unwrap_or(env::var("BUILDKITE_COMMIT").unwrap_or("unknown".to_string())).chars().take(7).collect()));
}