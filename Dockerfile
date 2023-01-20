FROM rust as builder
RUN rustup toolchain add nightly
RUN rustup default nightly
RUN cargo +nightly install -f cargo-fuzz

ADD . /nil
WORKDIR /nil/crates/syntax/fuzz

RUN cargo fuzz build parser

# Package Stage
FROM ubuntu:20.04

COPY --from=builder /nil/crates/syntax/fuzz/target/x86_64-unknown-linux-gnu/release/parser /
