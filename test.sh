#!/usr/bin/env bash
set -o errexit -o nounset -o pipefail -o xtrace

cargo test
cargo test --all-features
cargo test --no-default-features --no-run

(
    RUSTUP_DEFAULT=$(rustup default | awk -F"-" '{print $1}')
    cd ensure_no_std/
    rustup default nightly
    trap 'rustup default "${RUSTUP_DEFAULT}"' EXIT

    cargo rustc -- -C link-arg=-nostartfiles
)
