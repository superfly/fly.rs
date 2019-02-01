![superfly octokitty](https://user-images.githubusercontent.com/7375749/44759033-57b92780-aafd-11e8-880c-818b01c65ff3.png)

# Fly Edge Apps

[![Travis Status](https://travis-ci.com/superfly/fly.rs.svg?branch=master)](https://travis-ci.com/superfly/fly.rs)
[![Gitter chat](https://badges.gitter.im/superfly/fly.svg)](https://gitter.im/superfly/fly)
![Minimum rustc 1.31](https://img.shields.io/badge/rustc-1.31+-green.svg)

## DNS Applications

This is a DNS application server. It executes JavaScript to respond to DNS requests, and provides libraries for caching, global data storage, and outbound DNS/HTTP requests.

### Why would you want it?

You can use this project to build custom DNS services — both authoritative servers and resolvers. It's quicker and easier to do complicated DNS work with Fly than it is to build a DNS service from scratch, especially if you already know JavaScript. 

The real power is in running other peoples' code, however. It's designed to be deployed around the world, run untrusted applications built by not-you and make DNS development accessible to more developers.

### How it works

DNS application code runs in v8 isolates with [strict memory limits](https://github.com/superfly/fly.rs/blob/master/src/runtime.rs#L239-L245). The runtime accepts requests, parses them, and hands structured data over to application code.

### Installation

#### MacOS and Linux

[Download the latest release](https://github.com/superfly/fly.rs/releases) for your platform, ungzip and put the binary somewhere

#### Windows

Not yet done. Relevant issue: [#9](https://github.com/superfly/fly.rs/issues/9)

### Usage

```
fly-dns --port 8053 relative/path/to/file.js
```

### Examples

#### Simple proxy

```javascript
// Handle an event for a DNS request
addEventListener("resolv", event => {
  event.respondWith( // this function responds to the DNS request event
    resolv( // the resolv function resolves DNS queries
      event.request.name // requested record name
    )
  )
})
```

#### Static response

```javascript
addEventListener("resolv", event => {
  event.respondWith(function () { // can respond with a function
    return new DNSResponse([ // list of DNS answers
      {
        name: event.request.queries[0].name, // name of the DNS entry
        rrType: DNSRecordType.A, // record type
        ttl: 300, // time-to-live for the client
        data: {ip: "127.0.0.1"} // data for the record
      }
    ], { authoritative: true })
  })
})
```

## Fly & Deno

The Fly runtime was originally derived from [deno](https://github.com/denoland/deno) and shares some of the same message passing semantics. It has diverged quite a bit, but when possible we'll be contributing code back to deno.

There's an issue: [#5](https://github.com/superfly/fly.rs/issues/5)

## Development

### Prerequisites

- [rust](https://www.rust-lang.org/tools/install)
- [flatc](https://github.com/google/flatbuffers/releases) (or `brew install flatbuffers` on a mac)
- [node](https://nodejs.org)
- [yarn](https://yarnpkg.com/en/docs/install)

### Setup


- `wget -qO- https://github.com/superfly/libv8/releases/download/7.2.502.13/v8-osx-x64.tar.gz | tar xvz -C libfly`
- `cd v8env`
  - `yarn install`
  - `rollup -c`
  - `cd ..`
- `cargo run --bin dns hello-world.js`

### Running tests

1. Runtime tests:
```bash
cargo test
```

2. Javascript tests
```
cargo run --bin fly test "v8env/tests/**/*.spec.js"
```
