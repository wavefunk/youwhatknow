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

# Test the update-cargo-hash recipe's regex patterns and transformations (no nix build required)
test-update-cargo-hash:
    #!/usr/bin/env bash
    set -euo pipefail

    PASS=0
    FAIL=0
    report() {
        if [ "$1" -eq 0 ]; then
            echo "  PASS: $2"
            PASS=$((PASS + 1))
        else
            echo "  FAIL: $2"
            FAIL=$((FAIL + 1))
        fi
    }

    echo "=== update-cargo-hash recipe tests ==="

    # --- Test 1: Bash syntax validity ---
    # Extract the recipe body and check it parses without errors
    SCRIPT=$(sed -n '/^update-cargo-hash:/,/^[^ \t]/{
        /^update-cargo-hash:/d
        /^[^ \t]/d
        s/^    //
        p
    }' justfile)
    echo "$SCRIPT" | bash -n 2>/dev/null
    report $? "recipe script is syntactically valid bash"

    # --- Test 2: sed blank-regex matches cargoHash line ---
    # The blanking sed must match exactly 1 line in flake.nix
    COUNT=$(grep -c 'cargoHash = "sha256-[A-Za-z0-9+/]\+=*"' flake.nix)
    [ "$COUNT" -eq 1 ]
    report $? "sed blank-regex matches exactly 1 line in flake.nix (got $COUNT)"

    # --- Test 3: sed blank-regex does NOT match beads-latest hash line ---
    COUNT=$(echo '            hash = "sha256-z3EDtaBHB3ltPRT7vuBFURD7UwgAJBXAPozRnkjejeU=";' \
        | grep -c 'cargoHash = "sha256-[A-Za-z0-9+/]\+=*"' || true)
    [ "$COUNT" -eq 0 ]
    report $? "sed blank-regex does not match 'hash = \"sha256-...\"'"

    # --- Test 4: sed blank-regex does NOT match beads-latest vendorHash line ---
    COUNT=$(echo '          vendorHash = "sha256-1BJsEPP5SYZFGCWHLn532IUKlzcGDg5nhrqGWylEHgY=";' \
        | grep -c 'cargoHash = "sha256-[A-Za-z0-9+/]\+=*"' || true)
    [ "$COUNT" -eq 0 ]
    report $? "sed blank-regex does not match 'vendorHash = \"sha256-...\"'"

    # --- Test 5: sed correctly blanks cargoHash ---
    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT
    cp flake.nix "$TMPDIR/flake.nix"
    sed -i 's/cargoHash = "sha256-[A-Za-z0-9+/]\+=*"/cargoHash = ""/' "$TMPDIR/flake.nix"
    BLANK_COUNT=$(grep -c 'cargoHash = "";' "$TMPDIR/flake.nix")
    [ "$BLANK_COUNT" -eq 1 ]
    report $? "sed correctly blanks cargoHash to empty string"

    # --- Test 6: sed blanking preserves other hashes ---
    HASH_BEFORE=$(grep -c 'hash = "sha256-' flake.nix)
    HASH_AFTER=$(grep -c 'hash = "sha256-' "$TMPDIR/flake.nix")
    [ "$HASH_BEFORE" -eq "$HASH_AFTER" ]
    report $? "sed blanking preserves beads-latest 'hash' line ($HASH_BEFORE -> $HASH_AFTER)"

    VHASH_BEFORE=$(grep -c 'vendorHash = "sha256-' flake.nix)
    VHASH_AFTER=$(grep -c 'vendorHash = "sha256-' "$TMPDIR/flake.nix")
    [ "$VHASH_BEFORE" -eq "$VHASH_AFTER" ]
    report $? "sed blanking preserves beads-latest 'vendorHash' line ($VHASH_BEFORE -> $VHASH_AFTER)"

    # --- Test 7: sed write-back correctly inserts a new hash ---
    NEW="sha256-l4mUMRlhlzWohy/YlUbCGxDLu9UxI5Yn4fON92yvo9E="
    sed -i "s|cargoHash = \"\"|cargoHash = \"${NEW}\"|" "$TMPDIR/flake.nix"
    RESTORED=$(grep -c "cargoHash = \"${NEW}\"" "$TMPDIR/flake.nix")
    [ "$RESTORED" -eq 1 ]
    report $? "sed write-back correctly inserts new hash"

    # --- Test 8: Round-trip blank-then-restore is lossless ---
    # Blank, then write back the original hash -- result should be identical to original
    cp flake.nix "$TMPDIR/roundtrip.nix"
    ORIG_HASH=$(grep -oP 'cargoHash = "\Ksha256-[A-Za-z0-9+/]+=*' flake.nix)
    sed -i 's/cargoHash = "sha256-[A-Za-z0-9+/]\+=*"/cargoHash = ""/' "$TMPDIR/roundtrip.nix"
    sed -i "s|cargoHash = \"\"|cargoHash = \"${ORIG_HASH}\"|" "$TMPDIR/roundtrip.nix"
    diff -q flake.nix "$TMPDIR/roundtrip.nix" >/dev/null 2>&1
    report $? "round-trip (blank then restore original hash) produces identical file"

    # --- Test 9: grep regex extracts hash from realistic nix build error output ---
    NIX_ERROR='error: hash mismatch in fixed-output derivation '"'"'/nix/store/abc-youwhatknow-0.0.1-vendor-staging.drv'"'"':
         specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
            got:    sha256-l4mUMRlhlzWohy/YlUbCGxDLu9UxI5Yn4fON92yvo9E='
    EXTRACTED=$(echo "$NIX_ERROR" | grep -oP 'got:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)
    [ "$EXTRACTED" = "sha256-l4mUMRlhlzWohy/YlUbCGxDLu9UxI5Yn4fON92yvo9E=" ]
    report $? "grep regex extracts hash from nix error output"

    # --- Test 10: grep regex handles tab-indented nix output ---
    NIX_ERROR_TABS=$'         specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n\t\t    got:\tsha256-ABC123def456+/ghi789jkl012mno345pqr678stu9w='
    EXTRACTED_TABS=$(echo "$NIX_ERROR_TABS" | grep -oP 'got:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)
    [ "$EXTRACTED_TABS" = "sha256-ABC123def456+/ghi789jkl012mno345pqr678stu9w=" ]
    report $? "grep regex handles tab-indented nix output"

    # --- Test 11: grep regex does NOT match the 'specified:' hash ---
    SPECIFIED=$(echo "$NIX_ERROR" | grep -oP 'specified:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)
    GOT=$(echo "$NIX_ERROR" | grep -oP 'got:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)
    [ "$SPECIFIED" != "$GOT" ]
    report $? "grep regex for 'got:' does not return the 'specified:' hash"

    # --- Test 12: grep regex returns empty on output with no hash mismatch ---
    NO_HASH_OUTPUT="error: builder for derivation failed with exit code 1
    some random build log output
    no hash mismatch here"
    EXTRACTED_NONE=$(echo "$NO_HASH_OUTPUT" | grep -oP 'got:\s+\Ksha256-[A-Za-z0-9+/]+=*' || true)
    [ -z "$EXTRACTED_NONE" ]
    report $? "grep regex returns empty when no hash mismatch in output"

    # --- Test 13: sed regex matches hashes without trailing padding (no '=') ---
    NO_PAD='          cargoHash = "sha256-BOovGtXjqJ2ZEI1IQeF6SEZB5QNs8br46mrpMRqIabc";'
    COUNT=$(echo "$NO_PAD" | grep -c 'cargoHash = "sha256-[A-Za-z0-9+/]\+=*"')
    [ "$COUNT" -eq 1 ]
    report $? "sed regex matches hashes without trailing '=' padding"

    # --- Test 14: flake.nix.bak is in .gitignore ---
    grep -q 'flake\.nix\.bak' .gitignore
    report $? "flake.nix.bak is listed in .gitignore"

    # --- Summary ---
    echo ""
    echo "Results: $PASS passed, $FAIL failed out of $((PASS + FAIL)) tests"
    [ "$FAIL" -eq 0 ]
