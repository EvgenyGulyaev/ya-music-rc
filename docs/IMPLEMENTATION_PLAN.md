# Ya Player MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for behavior changes. Shell commands in this workspace must be prefixed with `rtk`.

**Goal:** Build a minimal native Rust Yandex Music client that opens a lightweight window, lets the user enter a token, checks login, displays favorites and "My Wave" station data/tracks, and plays tracks from favorites or wave.

**Architecture:** The app is split into a testable core library and a thin `egui` desktop shell. The Yandex Music integration is isolated behind a small HTTP client interface so tests can cover parsing and request behavior without hitting the real service. Playback uses `rodio`; track bytes are staged in an unlinked temporary file to avoid keeping a full MP3 in memory.

**Tech Stack:** Rust 2024, `eframe/egui` for native UI, `reqwest` for HTTP, `serde`/`serde_json` for parsing, `directories` for config paths, `rodio` for playback.

**Skills Used:** `superpowers:test-driven-development` for behavior changes, `superpowers:requesting-code-review` for the post-implementation review pass, and `superpowers:verification-before-completion` before final handoff.

---

### Task 1: Core Domain And Config

**Files:**
- Create: `src/lib.rs`
- Create: `src/config.rs`
- Create: `tests/config_tests.rs`

- [x] Write failing tests for token config load/save and redaction.
- [x] Implement `AppConfig` and filesystem helpers.
- [x] Run `rtk cargo test config`.

### Task 2: Yandex Music API Adapter

**Files:**
- Create: `src/api.rs`
- Create: `tests/api_tests.rs`

- [x] Write failing tests for auth headers, account status parsing, liked-track parsing, and wave station parsing.
- [x] Implement `YandexMusicClient` with a mockable `HttpClient` trait.
- [x] Run `rtk cargo test api`.

### Task 3: Player State And Shortcuts

**Files:**
- Create: `src/player.rs`
- Create: `tests/player_tests.rs`

- [x] Write failing tests for queue navigation, play/pause toggle, next, previous, and shortcut mapping.
- [x] Implement minimal queue state.
- [x] Run `rtk cargo test player`.

### Task 4: Native UI Shell

**Files:**
- Modify: `src/main.rs`
- Create: `src/app.rs`

- [x] Add a compact `egui` interface for token entry, login check, favorites, wave, queue state, and shortcut hints.
- [x] Wire buttons to the API adapter.
- [x] Keep UI asset-free and WebView-free for low memory.

### Task 5: Playback From Favorites

**Files:**
- Create: `src/download.rs`
- Create: `src/audio.rs`
- Create: `tests/download_tests.rs`
- Modify: `src/api.rs`
- Modify: `src/app.rs`
- Modify: `src/player.rs`

- [x] Write failing tests for download-info XML parsing, signed MP3 URL generation, and playback URL resolution.
- [x] Resolve Yandex Music `download-info` entries into signed MP3 URLs.
- [x] Parse station track sequences from `rotor/station/{station}/tracks`.
- [x] Wire play/pause/next/previous to actual audio playback for favorite and wave tracks.
- [x] Use temporary files for downloaded track data instead of `Vec<u8>` buffers.

### Task 6: Docs And Verification

**Files:**
- Create: `README.md`

- [x] Document `rtk cargo run`, token handling, shortcuts, limitations, and next steps.
- [x] Run `rtk cargo fmt`.
- [x] Run `rtk cargo test`.
- [x] Run `rtk cargo check`.
- [ ] Optionally run the app and measure RSS with `ps`.
