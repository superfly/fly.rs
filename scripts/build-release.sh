#!/bin/bash

set -e

cargo run -p create_snapshot v8env/dist/v8env.js v8env.bin
cargo build --bin fly --release
cargo build -p distributed-fly --release