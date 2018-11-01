# Fly DNS Apps

## Handling DNS requests with JavaScript

This is a programmable DNS server. You can write JavaScript to handle DNS queries any way you want.

## Installation

### MacOS and Linux

[Download the latest release](https://github.com/superfly/fly.rs/releases) for your platform, ungzip and put the binary somewhere

### Windows

Not yet done. Relevant issue: [#9](https://github.com/superfly/fly.rs/issues/9)

## Usage

```
fly-dns --port 8053 relative/path/to/file.js
```

## Examples

### Simple proxy

```javascript
// Handle an event for a DNS request
addEventListener("resolv", event => {
  event.respondWith( // this function responds to the DNS request event
    resolv( // the resolv function resolves DNS queries
      event.request.queries[0] // picks the first DNSQuery in the request
    )
  )
})
```

### Static response

```javascript
addEventListener("resolv", event => {
  event.respondWith(function () { // can respond with a function
    return {
      authoritative: true, // hopefully you know what you're doing
      answers: [ // list of DNS answers
        {
          name: event.request.queries[0].name, // name of the DNS entry
          rrType: DNSRecordType.A, // record type
          ttl: 300, // time-to-live for the client
          data: {ip: "127.0.0.1"} // data for the record
        }
      ]
    }
  })
})
```

## Fly & Deno

The Fly runtime was originally derived from [deno](/denoland/deno) and shares some of the same message passing semantics. It has diverged quite a bit, but when possible we'll be contributing code back to deno.

There's an issue: [#5](https://github.com/superfly/fly.rs/issues/5)

## Development

### Setup

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
- `cargo run --bin dns`

### Running v8env tests

```
cargo run --bin test "v8env/tests/**/*.spec.js"
```
