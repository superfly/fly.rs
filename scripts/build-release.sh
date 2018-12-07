#!/bin/bash

set -e

touch v8env.bin
cargo build --release --bin create_snapshot
ls -lah target/release
target/release/create_snapshot v8env/dist/v8env.js v8env.bin
cargo build --bin dns --release
strip target/release/dns
cp target/release/dns fly-dns
cargo build -p distributed-fly --release
strip target/release/distributed-fly
cp target/release/distributed-fly fly-dist
tar czf $RELEASE_FILENAME fly-dns fly-dist