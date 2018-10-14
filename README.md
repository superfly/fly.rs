# Fly Edge Runtime

## A multi-tenant, v8 based runtime for building Edge Apps

This is the next generation version of [fly](superfly/fly), and replaces the Node / isolated-vm portions of the runtime with a native Rust + v8 binary. It's much faster. And much more concurrent. Plus it's Rust so it's more fun.

## Installation

- `brew install ccache` (this will make your life faster)
- `git submodule update --init`
- `cd libfly/third_party/v8`
  - setup `.gclient` & `.gclient_entries` https://gist.github.com/mrkurt/f2faac7e0b591c6f5faf0562e4a0b167
  - ensure `./libfly/third_party/depot_tools` is in `$PATH` (prepending works best)
  - `../depot_tools/gclient sync` (might fail, but could be ok)
  - run
    ```
    gn gen out.gn/x64.release --args='
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
        v8_use_snapshot = true'
    ```
  - `ninja -C out.gn/x64.release v8_monolith`
  - go get coffee for about 30 minutes while your laptop flies off
  - `cd ../../../`
- `cd third_party/flatbuffers`
  - `cmake -G "Unix Makefiles"`
  - `make flatc`
  - ensure `./third_party/flatbuffers` is in `$PATH`
  - `cd ../../`
- `cd v8env`
  - `yarn install && yarn build`
  - `rollup -c`
  - `cd ../../../`
- `cargo run --bin server`

## Fly & Deno

The Fly runtime was originally derived from [deno](denoland/deno) and shares some of the same message passing semantics. It has diverged quite a bit, but when possible we'll be contributing code back to deno.

## TODO

- [x] Send `print` (all `console.x` calls) back into Rust to handle in various ways
  - [x] Send errors to stderr
  - [x] Use envlogger (`debug!`, `info!`, etc. macros) for messages
  - [ ] Allow sending to graylog or something external
- [ ] Feature-parity
  - [ ] Image API
  - [ ] Cache
    - [ ] Expire (set ttl)
    - [ ] TTL (get ttl)
    - [ ] Tags / purge
    - [ ] global.purgeTag / del
- [ ] Builder
  - [ ] TypeScript support
  - [ ] HTTP imports!
- [ ] CI builds + releases
  - [x] Mac
  - [x] Linux
  - [ ] Windows
- HTTP
  - [ ] Actually use the config hostnames and correct app
  - [ ] Spawn multiple runtime instances for the same app (n cpus? configurable?)
  - [ ] Add `Server` header for Fly and current version
- Stability / Resilience
  - [ ] do not use `unwrap` (that will panic and exit the process). Solution is to handle them and return or print proper errors
  - [ ] Get rid of all warnings
  - [ ] Tests!