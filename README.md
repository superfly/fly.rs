# Fly Rust V8

## Installation

- `git submodule update --init`
- `cd third_party/v8`
  - setup `.gclient` & `.gclient_entries` https://flyio.slack.com/archives/C504D8602/p1537975314000100
  - `../depot_tools/gclient sync` (might fail, but could be ok)
  - `tools/dev/v8gen.py x64.debug`
  - `ninja -C out.gn/x64.debug`
  - go get coffee for about 30 minutes while your laptop flies off
  - `cd ../../`
- `cd third_party/flatbuffers`
  - `cmakexbuild`
  - ensure `./third_party/flatbuffers/Debug` is in `$PATH`
- `cd fly/packages/v8env`
  - `yarn build`
  - `rollup -c`
  - `cd ../../../`
- `cargo run`
