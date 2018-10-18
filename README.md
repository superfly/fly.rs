# Fly Edge Runtime

## A multi-tenant, v8 based runtime for building Edge Apps

This is the next generation version of [fly](superfly/fly), and replaces the Node / isolated-vm portions of the runtime with a native Rust + v8 binary. It's much faster. And much more concurrent. Plus it's Rust so it's more fun.

## Installation

- `wget -qO- https://github.com/superfly/libv8/releases/download/7.1.321/v8-osx-x64.tar.gz | tar xvz -C libfly`
- `git submodule update --init`
- `cd third_party/flatbuffers`
  - `cmake -G "Unix Makefiles"`
  - `make flatc`
  - ensure `./third_party/flatbuffers` is in `$PATH`
  - `cd ../../`
- `cd v8env`
  - `yarn install`
  - `rollup -c`
  - `cd ..`
- `cargo run --bin server`

## Running v8env tests

```
cargo run --bin test "v8env/tests/**/*.spec.js"
```

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
  - [ ] Testing
- [ ] Runtime
  - [ ] Lifecycle management
    - [ ] Gracefully "replace" if running out of heap
  - [ ] Handle promise rejection (trash the runtime? just log?)
  - [ ] Handle uncaught error
- [ ] Builder
  - [ ] TypeScript support
  - [ ] HTTP imports!
  - [ ] Source maps
    - [ ] Handle source maps in the rust hook
- [ ] CI builds + releases
  - [x] Mac
  - [x] Linux
  - [ ] Windows
- HTTP
  - [ ] Actually use the config hostnames and correct app
  - [x] Spawn multiple runtime instances for the same app (n cpus? configurable?)
  - [ ] Add `Server` header for Fly and current version (maybe?)
  - [ ] Fetch request bodies
- [ ] Observability
  - [ ] Exception reporting (via Sentry probably)
  - [ ] Metrics (prometheus)
- Stability / Resilience
  - [ ] do not use `unwrap` (that will panic and exit the process). Solution is to handle them and return or print proper errors
  - [x] Get rid of all warnings
  - [ ] Tests!