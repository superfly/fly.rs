# Fly Rust V8

## Installation

- `git submodule update --init`
- `cd third_party/v8`
  - `tools/dev/v8gen.py x64.debug`
  - `../depot_tools/gclient sync` (might fail, but could be ok)
  - `ninja -C out.gn/x64.debug`
  - go get coffee for about 30 minutes while your laptop flies off
  - `cd ../../`
- `cd fly/packages/v8env`
  - `tsc`
  - `rollup -c`
  - `cd ../../../`
- `cargo run`