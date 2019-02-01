#!/bin/bash

set -e

cargo run -p create_snapshot v8env/dist/v8env.js v8env.bin
cargo build --features openssl_vendored --bin fly --release
cargo build --features openssl_vendored -p distributed-fly --release
