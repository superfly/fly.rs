FROM node:10 as v8env
ADD v8env v8env

WORKDIR ./v8env
RUN yarn install
RUN ./node_modules/.bin/rollup -c

RUN ls -lah dist

FROM rust:1.29

WORKDIR /usr/src/myapp

ADD libfly libfly
ADD scripts scripts
ADD .git .git
ADD .gitmodules .gitmodules
RUN scripts/compile_v8.sh

COPY . .
RUN cargo build --release --bin create_snapshot

RUN ls -lah target/release

COPY --from=v8env v8env/dist/* v8env/dist/

RUN target/release/create_snapshot v8env/dist/v8env.js v8env.bin

RUN cargo build --release

RUN ls -lah target/release