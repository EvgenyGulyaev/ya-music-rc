# Dependency Refresh Design

## Goal

Bring every direct Rust dependency to its latest stable release available on
2026-08-08, refresh transitive dependencies, keep behavior unchanged, and leave
the repository clean under its existing checks.

## Chosen approach

Update the direct version requirements in `Cargo.toml` only where the latest
stable release is outside the current requirement, then regenerate `Cargo.lock`.
The lockfile, rather than patch-level pins in the manifest, records exact
versions. Apply only compiler-required API migrations and semantics-preserving
Clippy simplifications.

Latest direct releases checked through the official Cargo registry client:

- `directories` 6.0.0
- `eframe` and `egui` 0.36.1
- `global-hotkey` 0.8.0
- `md5` 0.8.1
- `open` 5.4.1
- `reqwest` 0.13.4
- `rodio` 0.22.2
- `serde` 1.0.229
- `serde_json` 1.0.151
- `souvlaki` 0.8.3
- `tempfile` 3.27.0

## Scope

- Move `eframe` and `egui` from 0.33 to 0.36.
- Move `reqwest` from 0.12 to 0.13.
- Refresh all compatible direct and transitive versions in `Cargo.lock`.
- Remove the redundant `default-features = true` setting from `eframe`.
- Fix the four current Clippy findings in `src/app.rs` without changing UI or
  playback behavior.
- Make no speculative refactors and add no dependencies.

## Compatibility and error handling

Compilation errors from new APIs will be handled at their existing call sites.
Network, authentication, playback, configuration, hotkey, and media-control
error behavior must remain unchanged. Platform feature sets remain enabled as
they are today; feature pruning is excluded because all supported desktop
targets cannot be exercised locally.

## Verification

Run, in order:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-targets --all-features`
4. `cargo build --release`

The update is ready to push only when all four commands exit successfully and
the final diff contains no unrelated changes.
