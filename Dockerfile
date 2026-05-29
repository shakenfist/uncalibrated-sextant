FROM rust:1.88-slim AS build

RUN rustup component add rust-src rustfmt clippy \
 && rustup target add x86_64-unknown-uefi \
 && rustup target add x86_64-unknown-linux-gnu

WORKDIR /work
