default:
    @just --list

verify: fmt clippy test

fmt:
    cargo fmt --all -- --check

clippy:
    cargo clippy --all-targets -- -D warnings

test:
    cargo test

fix:
    cargo fmt --all
    cargo clippy --all-targets --fix --allow-dirty
