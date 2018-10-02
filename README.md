# Fly Rust V8

## Installation

- `brew install ccache` (this will make your life faster)
- `git submodule update --init`
- `cd libfly/third_party/v8`
  - setup `.gclient` & `.gclient_entries` https://flyio.slack.com/archives/C504D8602/p1537975314000100
  - ensure `./libfly/third_party/depot_tools` is in `$PATH` (prepending works best)
  - `../depot_tools/gclient sync` (might fail, but could be ok)
  - `tools/dev/v8gen.py x64.release`
  - edit `out.gn/x64.release/args.gn`:
    ```
    is_debug = false
    target_cpu = "x64"
    cc_wrapper = "ccache"
    is_official_build = true
    v8_deprecation_warnings = false
    v8_enable_gdbjit = false
    v8_enable_i18n_support = false
    v8_experimental_extra_library_files = []
    v8_extra_library_files = []
    v8_imminent_deprecation_warnings = false
    v8_monolithic = true
    v8_untrusted_code_mitigations = false
    v8_use_external_startup_data = false
    v8_use_snapshot = true
    ```
  - `ninja -C out.gn/x64.release`
  - go get coffee for about 30 minutes while your laptop flies off
  - `cd ../../../`
- `cd third_party/flatbuffers`
  - `cmake -G "Xcode" -DCMAKE_BUILD_TYPE=Release`
  - `cmakexbuild`
  - ensure `./third_party/flatbuffers/Debug` is in `$PATH`
  - `cd ../../`
- `cd fly/packages/v8env`
  - `yarn install && yarn build`
  - `rollup -c`
  - `cd ../../../`
- `cargo build --bin create_snapshot`
  - `target/debug/create_snapshot fly/packages/v8env/dist/v8env.js v8env.bin`
- `cargo run --bin server`

## TODO

- [ ] Send `print` (all `console.x` calls) back into Rust to handle in various ways
  - [ ] Send errors to stderr
  - [ ] Use envlogger (`debug!`, `info!`, etc. macros) for messages
  - [ ] Allow sending to graylog or something external
- [ ] Builder
  - [ ] TypeScript support
  - [ ] HTTP imports!
- [ ] CI builds + releases
  - [ ] Mac
  - [ ] Linux
  - [ ] Windows
- HTTP
  - [ ] Actually use the config hostnames and correct app
  - [ ] Add `Server` header for Fly and current version
- Stability / Resilience
  - [ ] do not use `unwrap` (that will panic and exit the process). Solution is to handle them and return or print proper errors
  - [ ] Get rid of all warnings
  - [ ] Tests!