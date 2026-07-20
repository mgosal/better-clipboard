# Improvements in this branch (v0.2.4)

A review + fix pass. Verified with `cargo test` (8/8, two new tests) and
`cargo clippy --all-targets` (0 warnings).

## 1. Orphaned image files on disk (src/main.rs — `HistoryStore`)

**What:** Two paths dropped history entries without deleting their stored
PNGs: `push` replaced a same-hash duplicate via `retain`, and `load` trimmed
over-limit histories via `truncate`.

**Why it mattered:** Re-copying the same image repeatedly leaked one PNG per
copy into the data directory, forever. Shrinking the history limit leaked the
trimmed entries' files. Only `prune_to_limit` cleaned up correctly.

**How it was fixed:** `push` now drains and filters items, removing the
replaced entry's image file; `load` routes its limit enforcement through
`prune_to_limit`, which already deletes files, and persists when it pruned.

## 2. Colon text misclassified as URLs (src/main.rs — `classify_text`)

**What:** Classification used bare `Url::parse(...).is_ok()`.

**Why it mattered:** `Url::parse` accepts any `word:rest` pair —
`note:remember-this` parses with scheme `note` — so ordinary text got the
`URL` chip and an "Open URL" action that hands junk to `open`.

**How it was fixed:** New `is_probable_url` requires a host or a well-known
host-less scheme (`mailto`, `tel`, `sms`, `facetime`). Tests cover
`note:remember-this`, `todo:refactor`, `mailto:`, and `http://localhost:8080`.

## 3. Defused a future footgun in `HistoryStore::load`

**What:** The normalize step used
`fold(false, |changed, item| normalize_loaded_item(item) || changed)`, which
clippy suggests rewriting as `.any(...)`.

**Why it mattered:** `any` short-circuits — applying that suggestion (e.g. via
a blanket `cargo clippy --fix`) would silently stop normalizing items after
the first change. The current code was correct but one autofix away from a bug.

**How it was fixed:** Rewritten as an explicit loop with a comment stating the
no-short-circuit requirement.

## 4. Smaller cleanups

- Dropped a redundant clone of the entire history on every egui frame in
  `update()`.
- Remaining clippy lints fixed: collapsible ifs via let-chains, derived
  `Default` for `ThemeMode`, entry-style map lookups.
- Version bumped 0.2.3 → 0.2.4 in `Cargo.toml` and the app-bundle plist,
  per AGENTS.md (patch for fixes). The GitHub release step is left to the
  maintainer.
