#!/bin/bash

set -ex

cargo clippy --tests
cargo fmt
cargo doc

cargo nextest run
cargo +nightly miri nextest run
