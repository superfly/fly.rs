FROM node:10-stretch as v8env

COPY --from=neomantra/flatbuffers:20180924 /usr/local/bin/flatc /usr/local/bin/flatc

WORKDIR /v8env
COPY v8env/package.json package.json
RUN yarn install

ADD v8env/ .

ADD msg.fbs .
RUN flatc --ts -o src --no-fb-import --gen-mutable msg.fbs

RUN ./node_modules/.bin/rollup -c

RUN ls -lah dist

FROM flyio/v8:7.1 as v8

FROM liuchong/rustup:1.29.1 as builder

RUN apt-get update -qq \
  && apt-get install -y --no-install-recommends \
  ca-certificates build-essential pkg-config git curl python libxml2 libxml2-dev \
  clang-3.8 libc++-dev libc++abi-dev \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/myapp

COPY --from=neomantra/flatbuffers:20180924 /usr/local/bin/flatc /usr/local/bin/flatc

COPY libfly libfly
COPY --from=v8 /v8/lib libfly/third_party/v8/out.gn/x64.release/obj
COPY . .

RUN ls -l third_party/flatbuffers

RUN touch v8env.bin && mkdir -p v8env/dist && touch v8env/dist/v8env.js.map
RUN cargo build --release --bin create_snapshot

RUN ls -lah target/release

COPY --from=v8env v8env/dist v8env/dist

RUN target/release/create_snapshot v8env/dist/v8env.js v8env.bin

RUN cargo build --release

RUN ls -lah target/release

RUN ldd target/release/server

FROM liuchong/rustup:1.29.1 as bin
COPY --from=builder /usr/src/myapp/target/release/server /app/server
COPY --from=builder /usr/src/myapp/target/release/dns /app/dns