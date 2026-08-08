# Dependency Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update every direct dependency to its latest stable release, refresh the lockfile, and clear the current Clippy findings without changing behavior.

**Architecture:** Keep the existing crate structure and dependency features. Change only version requirements, the generated lockfile, and three semantics-preserving expressions in `src/app.rs`.

**Tech Stack:** Rust 2024, Cargo, eframe/egui, reqwest, Clippy.

## Global Constraints

- Latest releases are the versions recorded in `docs/superpowers/specs/2026-08-08-dependency-refresh-design.md`.
- Add no dependencies and make no unrelated refactors.
- Preserve the existing desktop feature set and application behavior.
- Push only after format, Clippy, tests, and release build all succeed.

---

### Task 1: Update dependency requirements and lockfile

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**
- Consumes: the existing Cargo package and feature declarations.
- Produces: the same application behavior compiled against eframe/egui 0.36 and reqwest 0.13.

- [x] **Step 1: Record the outdated baseline**

Run: `cargo update --dry-run`

Expected: compatible lockfile updates are listed while eframe/egui remain on 0.33 and reqwest remains on 0.12 because of manifest requirements.

- [x] **Step 2: Update the three breaking version requirements**

Use this dependency declaration:

```toml
eframe = "0.36"
egui = "0.36"
reqwest = { version = "0.13", default-features = false, features = ["blocking", "json", "rustls"] }
```

The shorter `eframe` declaration intentionally removes the redundant `default-features = true` setting.
Reqwest 0.13 removed `rustls-tls`; its supported `rustls` replacement keeps TLS
without enabling Reqwest's other default features and uses the platform verifier.

- [x] **Step 3: Refresh exact versions**

Run: `cargo update`

Expected: `Cargo.lock` records eframe/egui 0.36.1, reqwest 0.13.4, md5 0.8.1, open 5.4.1, serde 1.0.229, serde_json 1.0.151, and current compatible transitive releases.

- [x] **Step 4: Check API compatibility**

Run: `cargo check --all-targets --all-features`

Expected red check: eframe 0.36 reports the removed `App::update` method and
egui reports that panels now consume the root `Ui`.

- [x] **Step 5: Apply the eframe/egui 0.36 root-UI migration**

Change the app entry point to:

```rust
fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
```

Pass `ui.ctx()` to `handle_shortcuts`, replace the bottom panel constructor
with `egui::Panel::bottom("player_bar").exact_size(player_bar_height())`, and
pass the root `ui` to both panel `show` calls.

- [x] **Step 6: Verify the green compatibility check**

Run: `cargo check --all-targets --all-features`

Expected: exit 0.

### Task 2: Clear the existing Clippy failures

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: existing login visibility, shortcut dispatch, and seek behavior.
- Produces: identical behavior expressed with minimal Rust boolean and let-chain syntax.

- [x] **Step 1: Verify the existing red check**

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: failure for `nonminimal_bool` at the login form and `collapsible_if` in shortcut and seek handling.

- [x] **Step 2: Simplify the login-form condition**

Replace it with:

```rust
if self.account.is_none() && (!self.busy || self.token_input.trim().is_empty()) {
```

- [x] **Step 3: Collapse shortcut dispatch**

Use one let-chain:

```rust
if input.key_pressed(key)
    && let Some(command) =
        Shortcut::from_key(name, input.modifiers.ctrl, input.modifiers.command)
{
    commands.push(command);
}
```

- [x] **Step 4: Collapse seek handling**

Use one let-chain while preserving the existing error/status behavior:

```rust
if let Some(seek_position) =
    track_progress_capsule(ui, track_text, position, duration, capsule_width)
    && let Some(audio) = &self.audio
{
    if let Err(err) = audio.seek(seek_position) {
        self.status = err;
    } else {
        self.update_system_media_state();
    }
}
```

- [x] **Step 5: Verify the green check**

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: exit 0 with no warnings.

Cargo may still print the upstream future-incompatibility notice for
`block 0.1.6`, pulled by the latest `souvlaki 0.8.3`; `cargo tree --invert
block@0.1.6` must confirm that it is not a project-owned warning.

### Task 3: Verify, review, commit, and push

**Files:**
- Review: `Cargo.toml`
- Review: `Cargo.lock`
- Review: `src/app.rs`
- Review: `docs/superpowers/plans/2026-08-08-dependency-refresh.md`

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: a verified commit on `main`, pushed to `origin/main`.

- [x] **Step 1: Run all verification gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```

Expected: all commands exit 0; tests report 57 passing tests or more. The only
accepted diagnostic is the documented upstream `block 0.1.6` notice.

- [x] **Step 2: Review the final diff**

Run: `git diff --check && git diff --stat && git status --short --branch`

Expected: no whitespace errors and no unrelated files.

- [ ] **Step 3: Commit the implementation**

Run:

```bash
git add Cargo.toml Cargo.lock src/app.rs docs/superpowers/plans/2026-08-08-dependency-refresh.md
git commit -m "chore: update Rust dependencies"
```

- [ ] **Step 4: Push the verified branch**

Run: `git push origin main`

Expected: `origin/main` advances to the new implementation commit.
