default:
    @just --list

check:
    cargo check

test:
    cargo test

clippy:
    cargo clippy -- -D warnings

fmt:
    cargo fmt

watch:
    bacon

run:
    cargo run

serve:
    cargo run

build:
    cargo build --release
