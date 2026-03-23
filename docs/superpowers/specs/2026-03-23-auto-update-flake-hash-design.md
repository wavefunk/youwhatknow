# Auto-Update flake.nix Cargo Hash

**Date:** 2026-03-23
**Status:** Design

## Problem

The `flake.nix` uses `buildRustPackage` which requires a `cargoHash` -- an SRI hash of the vendored Cargo dependencies. When `Cargo.toml` or `Cargo.lock` changes (adding, removing, or updating a dependency), the hash becomes stale and `nix build` fails with a consistency error. The current fix is a manual four-step ritual:

1. Edit `flake.nix`, set `cargoHash = "";`
2. Run `nix build`, wait for it to fail
3. Copy the `got: sha256-...` hash from the error output
4. Paste it back into `cargoHash = "sha256-...";`

This is tedious, error-prone, and breaks flow. We want a single command that does all four steps.

## Goals

- One command (`just update-cargo-hash`) that updates `cargoHash` in `flake.nix` to the correct value.
- No new dependencies beyond `nix` and standard POSIX tools already available in the dev shell.
- Idempotent: running it when the hash is already correct is a no-op (or at worst, re-derives the same hash).
- Works from the project root or any worktree.

## Non-Goals

- Automatically running on every commit or dependency change. The developer runs it when they know dependencies changed. Automation via git hooks adds latency to every commit for a problem that only occurs when `Cargo.lock` changes.
- Updating the `beads-latest` hash. That's a separate concern (pinned to a specific GitHub release, not derived from local sources).
- Supporting `cargoSha256` (deprecated Nix attribute). We use `cargoHash` exclusively.
- Making this work outside the nix dev shell. The `nix` CLI must be available.

## Design Decisions

### Just recipe over git hook or standalone script

**Rationale:** The project already uses `justfile` for common commands (`check`, `test`, `clippy`, `build`, `serve`). A just recipe is discoverable (`just --list`), documented inline, and consistent with existing workflow. A git hook (pre-commit) would add latency to every commit -- vendor downloads take 10-30 seconds even when cached, and most commits don't change `Cargo.lock`. A standalone shell script would be a new file to maintain and remember; a just recipe is where developers already look.

### Build-fail-extract over nix-prefetch or custom FOD expression

**Rationale:** The build-fail-extract cycle (set hash to empty, build, parse error) is the approach recommended by Nix documentation and `buildRustPackage` itself. It works with any version of Nix and requires no additional tooling. A `nix-prefetch` approach would require either a third-party tool (`nix-prefetch-github` doesn't cover cargo vendoring) or a custom Nix expression that replicates the `buildRustPackage` vendor derivation logic -- fragile and tightly coupled to nixpkgs internals. The build-fail approach is self-correcting: it uses the exact same code path that `nix build` uses, so the hash is always correct by construction.

### sed for in-place replacement over a Nix overlay or passthru

**Rationale:** The hash lives in a single line in `flake.nix` with a deterministic format (`cargoHash = "sha256-...";`). A sed replacement is the simplest correct transformation. More sophisticated approaches (e.g., exposing the hash via a `passthru` attribute or reading it from a separate file) add indirection that makes the flake harder to read and review for a marginal ergonomic gain that the just recipe already provides.

## Implementation

### Just recipe: `update-cargo-hash`

```just
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
    echo "Building to determine correct cargoHash..."
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
```

### Behavior

| Scenario | Result |
|---|---|
| `Cargo.lock` changed, hash stale | Builds vendor derivation, extracts new hash, updates `flake.nix` |
| Hash already correct | Builds vendor derivation (cache hit if store populated), writes same hash back -- effectively a no-op diff |
| `nix` not available | `sed` succeeds but `nix build` fails with "command not found"; backup restored, full output printed |
| `flake.nix` has no `cargoHash` line | `sed` no-ops (no match), build fails for other reasons, backup restored, script exits 1 |
| Build fails for non-hash reasons (e.g., syntax error in Rust) | Hash mismatch never appears in output, `grep` finds nothing, backup restored, full output printed, exits 1 |
| Network unavailable | Vendor download fails, no hash in output, backup restored, exits 1 |

### Error handling

The script creates a backup of `flake.nix` before modification and sets an ERR trap to restore it on any unexpected failure. Two explicit failure paths:

1. **Hash extraction fails** (no `got:` line in build output): restore `flake.nix` from backup, print the full build output to stderr for diagnosis, exit 1. This covers network failures, Nix store corruption, and any build error unrelated to hash mismatch.

2. **`set -e` failures** (e.g., `sed` or `cp` fail): the ERR trap restores the backup automatically.

On success, the backup is removed. This approach is safer than `git checkout` because it preserves any uncommitted changes the user may have in `flake.nix` outside the `cargoHash` line.

Note: `nix build` is expected to fail (exit non-zero) with the empty hash. The script captures its output and continues regardless (`|| true`). The `grep` also uses `|| true` since a missing match is handled explicitly by the `-z` check.

### Integration with justfile

The recipe is added to the existing `justfile` after the `build` recipe. It uses a `#!/usr/bin/env bash` shebang with `set -euo pipefail` because it needs multi-line logic with variables, conditionals, and error handling that just's default line-by-line execution cannot express.

### Dependencies

- `nix` CLI (available in the dev shell and on any NixOS/nix-enabled system)
- `sed` with `-i` flag (GNU sed, standard on Linux)
- `grep` with `-oP` (Perl-compatible regex, standard with GNU grep)
- `git` (for `git checkout` on failure)
- `bash` (for the shebang)

All of these are available in the project's nix dev shell and on any standard Linux system.

## File Changes

| File | Action | What |
|---|---|---|
| `justfile` | Modify | Add `update-cargo-hash` recipe |

No new files. No Rust code changes. No flake.nix structural changes.

## Testing

Manual verification only -- this is a developer tooling recipe, not application code.

1. Change a dependency in `Cargo.toml`, run `cargo update` to update `Cargo.lock`
2. Run `just update-cargo-hash`
3. Verify `flake.nix` has the new hash
4. Run `nix build` to confirm it succeeds
5. Run `just update-cargo-hash` again to verify idempotency

## Future Considerations

If the team adopts CI that runs `nix build`, the CI will catch stale hashes regardless of whether the developer ran `just update-cargo-hash`. The recipe is a convenience, not a gate. If it becomes clear that developers consistently forget, a CI check that diffs the current `cargoHash` against the computed one (without modifying files) could be added later.
