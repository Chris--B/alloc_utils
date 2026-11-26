#!/bin/bash

set -ex

cargo clippy --tests
RUSTFLAGS="--cfg asan" cargo clippy --tests 
cargo fmt
RUSTFLAGS="--cfg asan" cargo doc

cargo nextest run
cargo +nightly miri nextest run

RUSTFLAGS="-Zsanitizer=address --cfg asan"        \
    RUSTDOCFLAGS="-Zsanitizer=address --cfg asan" \
    cargo +nightly nextest run --release
