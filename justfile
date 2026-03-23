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

# Update cargoHash in flake.nix after Cargo.lock changes
update-cargo-hash:
    #!/usr/bin/env bash
    set -euo pipefail

    FLAKE="flake.nix"

    # Save original so we can restore on failure
    cp "$FLAKE" "$FLAKE.bak"
    trap 'mv "$FLAKE.bak" "$FLAKE"; echo "error: restored $FLAKE from backup" >&2' ERR

    # 1. Temporarily set cargoHash to empty string
    sed -i 's/cargoHash = "sha256-[A-Za-z0-9+/]\+=*"/cargoHash = ""/' "$FLAKE"

    # 2. Build and capture the hash mismatch error
    echo "Building to determine correct cargoHash (this downloads and vendors all cargo dependencies)..."
    BUILD_OUTPUT=$(nix build 2>&1 || true)
    NEW_HASH=$(echo "$BUILD_OUTPUT" | grep -oP 'got:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)

    if [ -z "$NEW_HASH" ]; then
        echo "error: could not extract hash from nix build output" >&2
        echo "$BUILD_OUTPUT" >&2
        mv "$FLAKE.bak" "$FLAKE"
        trap - ERR
        exit 1
    fi

    # 3. Write the correct hash back
    sed -i "s|cargoHash = \"\"|cargoHash = \"${NEW_HASH}\"|" "$FLAKE"
    rm "$FLAKE.bak"
    trap - ERR

    echo "Updated cargoHash to ${NEW_HASH}"
