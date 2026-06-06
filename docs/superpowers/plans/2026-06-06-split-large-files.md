# Split Large Files (Pinpoint) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the two largest hot files along clean responsibility seams with zero behavior change: extract `src/ui/util.rs`, then convert `src/app/update.rs` into `src/app/update/{mod,modal}.rs`.

**Architecture:** Pure structural refactoring. Code moves verbatim; only module declarations, `use` lines, and the minimal visibility markers change. The existing test suite is the characterization safety net — it must stay green at every step. No new tests, no logic edits.

**Tech Stack:** Rust 1.96.0 (pinned), ratatui, tokio, `cargo fmt`/`clippy`/`test` gates (clippy warnings are errors).

**Spec:** `docs/superpowers/specs/2026-06-06-refactor-split-large-files-design.md`

---

## File Structure

After this plan:

```
src/ui/
  mod.rs    # view() + all render_* (modal renderers stay) — loses 3 utils + 2 tests
  util.rs   # NEW: centered_rect, truncate_width, human_size + 2 tests

src/app/update/        # was src/app/update.rs
  mod.rs    # reducer entry, event/list handlers, open_* builders, ALL tests
  modal.rs  # NEW: modal key/submit handler cluster (handle_modal_key + 10 helpers)
```

Established codebase patterns used:
- Child test modules already use `use super::*;` (e.g. `ui/mod.rs:485`, `update.rs:571`) — new submodules follow the same glob-from-parent idiom for domain types.
- `pub(super)` for cross-file-within-module visibility (minimal surface; public API unchanged).
- `git mv` for the file→directory conversion so history follows the rename.

---

## Task 1: Extract `src/ui/util.rs`

Lowest-risk step, done first. Moves three pure helper fns and their two tests out of
`ui/mod.rs`. `centered_rect`, `truncate_width`, `human_size` are called from many `render_*`
fns that stay in `mod.rs`, so they become `pub(super)`.

**Files:**
- Create: `src/ui/util.rs`
- Modify: `src/ui/mod.rs` (add `mod util;` + `use`; fix `unicode_width` import; delete moved items)

- [ ] **Step 1: Confirm a green baseline**

Run:
```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
Expected: all three succeed (clean tree on branch `refactor/split-large-files`). Do not proceed if anything fails — the safety net must be green before moving code.

- [ ] **Step 2: Create `src/ui/util.rs`**

Create the file with the three fns (verbatim from `ui/mod.rs`, made `pub(super)`) plus the two moved tests:

```rust
//! Pure UI helpers: geometry and display-width/size formatting. No state, no
//! rendering — just functions shared by the renderers in the parent module.

use ratatui::layout::Rect;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A centered rectangle of the given size, clamped to `area`.
pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Truncate `s` to at most `max` display columns (CJK-aware), appending `…`
/// when characters are dropped.
pub(super) fn truncate_width(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1; // leave a column for the ellipsis
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + cw > budget {
            break;
        }
        out.push(c);
        width += cw;
    }
    out.push('…');
    out
}

/// Human-readable byte size using binary units.
pub(super) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_width_respects_cjk_columns() {
        assert_eq!(truncate_width("short", 10), "short");
        assert_eq!(truncate_width("abcdef", 4), "abc…");
        // Each CJK glyph is two columns: 3 glyphs = 6 columns; budget 5 -> 2
        // glyphs (4 cols) + ellipsis.
        assert_eq!(truncate_width("あいう", 5), "あい…");
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GB");
        assert_eq!(human_size(1536), "1.5 KB");
    }
}
```

- [ ] **Step 3: In `src/ui/mod.rs`, register the module and import the fns**

Add directly after the existing `use` block (after the `unicode_width` line, ~L21). The
`mod util;` declaration plus a `use` so the unqualified calls (`centered_rect(...)`,
`truncate_width(...)`, `human_size(...)`) keep compiling unchanged:

```rust
mod util;
use util::{centered_rect, human_size, truncate_width};
```

- [ ] **Step 4: In `src/ui/mod.rs`, narrow the `unicode_width` import**

`UnicodeWidthChar` was only used by `truncate_width` (now moved). `UnicodeWidthStr` is still
used by `vm_mem_line` (`ui/mod.rs:66`). Change line 21 from:

```rust
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
```
to:
```rust
use unicode_width::UnicodeWidthStr;
```

- [ ] **Step 5: In `src/ui/mod.rs`, delete the three moved fn definitions**

Delete `centered_rect`, `truncate_width`, and `human_size` (the contiguous block, current
L431–481, including their doc comments — from `/// A centered rectangle…` through the closing
`}` of `human_size`). They now live in `util.rs`.

- [ ] **Step 6: In `src/ui/mod.rs`, delete the two moved tests**

Inside `#[cfg(test)] mod tests`, delete `truncate_width_respects_cjk_columns` and
`human_size_formats` (current L670–684, with their `#[test]` attributes). Leave every other
test and the `render()`/`sample()`/`distro_named()` helpers untouched.

- [ ] **Step 7: Run the gates**

Run:
```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
Expected: all green. Same test count as baseline minus 0 (the 2 tests moved, not deleted — total unchanged). If clippy flags an unused import, re-check Step 4. If `cargo fmt --all -- --check` fails, run `cargo fmt --all` and re-verify the diff is whitespace-only.

- [ ] **Step 8: Verify it is a pure move**

Run:
```sh
git add -A
git diff --cached --stat
```
Expected: `src/ui/util.rs` created (~95 lines), `src/ui/mod.rs` reduced by ~95 lines. No other files. Skim `git diff --cached src/ui/mod.rs` to confirm only deletions + the `mod util;`/`use`/import-narrowing lines changed.

- [ ] **Step 9: Commit**

```sh
git commit -m "refactor(ui): extract centered_rect/truncate_width/human_size to ui/util.rs

Pure move, no behavior change. The two utility tests move with their fns.
UnicodeWidthChar import drops from mod.rs (only truncate_width used it).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Split `src/app/update.rs` → `src/app/update/{mod,modal}.rs`

Convert the file into a directory module and move the modal input-handling cluster
(`handle_modal_key` + its 10 helpers, current L294–567) into `modal.rs`. Only
`handle_modal_key` is called from outside the cluster (by `handle_key` at L97), so it alone
becomes `pub(super)`. **All tests and the `open_*` builders stay in `mod.rs`.**

**Files:**
- Rename: `src/app/update.rs` → `src/app/update/mod.rs` (via `git mv`)
- Create: `src/app/update/modal.rs`
- Modify: `src/app/update/mod.rs` (add `mod modal;` + `use`; remove the moved cluster)

- [ ] **Step 1: Confirm a green baseline**

Run:
```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
Expected: all green (Task 1 committed). Do not proceed otherwise.

- [ ] **Step 2: Convert the file into a directory module**

Create the directory and move the file so git tracks the rename:
```sh
mkdir -p src/app/update
git mv src/app/update.rs src/app/update/mod.rs
```
(PowerShell equivalent: `New-Item -ItemType Directory -Force src/app/update` then the same `git mv`.) After this, only `src/app/update/mod.rs` exists — no stray `update.rs`.

- [ ] **Step 3: Verify the tree still builds (no code moved yet)**

Run:
```sh
cargo test --all
```
Expected: green. The rename alone changes nothing — `crate::app::update` now resolves to the directory's `mod.rs`. This isolates "did the rename work" from "did the move work."

- [ ] **Step 4: Create `src/app/update/modal.rs`**

Create the file with this header, then move the cluster into it:

```rust
//! Modal input handling for the reducer: dispatches keys for the active modal
//! (confirm, form, progress, install picker, config editor, quit) and the
//! resulting state transitions. Pure — emits Commands only, like the rest of
//! the reducer. Entered via `handle_modal_key`, called from `super::handle_key`.

use super::*;
```

Then **move verbatim** the entire contiguous block currently at `src/app/update/mod.rs`
lines 294–567 — `handle_modal_key`, `handle_quit_key`, `handle_confirm_key`, `confirm_action`,
`handle_form_key`, `submit_form`, `handle_progress_key`, `handle_install_key`,
`handle_config_key`, `config_form_key`, `config_raw_key` — into `modal.rs` after the `use`
line. Make exactly one change to the moved code: the signature

```rust
fn handle_modal_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
```
becomes
```rust
pub(super) fn handle_modal_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
```
Leave the other ten fns as private `fn` (they are only called from within `modal.rs`). The
`use super::*;` brings in every type and alias the cluster needs (`Model`, `Command`,
`KeyEvent`, `KeyCode`, `Confirm`, `FormState`, `ProgressState`, `InstallPickState`,
`ConfigEditState`, `EditMode`, `LifecycleOp`, etc.) from the parent module.

- [ ] **Step 5: In `src/app/update/mod.rs`, delete the moved cluster**

Delete the same block (the eleven fns, current L294–567) from `mod.rs`. It now lives in
`modal.rs`. The deletion sits between `open_confirm_unregister`'s closing `}` (~L292) and the
`#[cfg(test)] mod tests` block (~L569).

- [ ] **Step 6: In `src/app/update/mod.rs`, register the submodule and import the entry point**

Add after the existing `use` block (after the `use crate::wsl::DistroState;` line, ~L15):
```rust
mod modal;
use modal::handle_modal_key;
```
This keeps the unchanged call site `handle_modal_key(model, key)` in `handle_key` (L97)
compiling. No other call site exists (verified: the cluster's other fns are only called from
within the cluster; the test module reaches modal flows through `update()` Actions, never by
calling these fns directly).

- [ ] **Step 7: Run the gates**

Run:
```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
Expected: all green, **identical test count to the Task 1 baseline** (no test moved or
removed; all stay in `mod.rs`). Likely clippy snags and fixes:
- "unused import" in `mod.rs` for a type now only used by the moved cluster → remove just that
  name from `mod.rs`'s `use super::{...}` list (e.g. `TypedConfirm` if only the modal code used
  it). Add it to nothing in `modal.rs` — `use super::*;` already covers it.
- "private item `handle_modal_key` … leaked"-style errors → confirm Step 4's `pub(super)`.

- [ ] **Step 8: Verify it is a pure move**

Run:
```sh
git add -A
git diff --cached --stat
```
Expected: `src/app/update.rs` → `src/app/update/mod.rs` shown as rename + deletions; new
`src/app/update/modal.rs` (~280 lines). No third file (unless a `use` line was trimmed in
Step 7). Skim `git diff --cached -M src/app/update/mod.rs` to confirm only the cluster
deletion + the `mod modal;`/`use` lines (and any trimmed import) changed.

- [ ] **Step 9: Commit**

```sh
git commit -m "refactor(app): split modal key handlers into update/modal.rs

Convert update.rs into a directory module; move the modal input-handling
cluster (handle_modal_key + 10 helpers) to update/modal.rs. Pure move, no
behavior change. handle_modal_key is pub(super) (called by handle_key);
open_* builders and all reducer tests stay in update/mod.rs.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Final verification

- [ ] **Step 1: Full gate run on the final tree**

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
Expected: all green, test count equal to the original baseline (no tests added or removed
across the whole plan — only relocated within their crates).

- [ ] **Step 2: Confirm line-count reduction and seams**

```sh
wc -l src/app/update/mod.rs src/app/update/modal.rs src/ui/mod.rs src/ui/util.rs
```
Expected (approx): `update/mod.rs` ~880, `update/modal.rs` ~280, `ui/mod.rs` ~600,
`ui/util.rs` ~95. No file regressed in behavior; the two large files are now split.

- [ ] **Step 3: Confirm invariants held**

Skim-verify:
- `update/mod.rs` and `update/modal.rs` contain no IO/terminal/async calls (still emit
  `Command`s only).
- `sample_inner_disk_if_needed` is still in `update/mod.rs` (df-once rule intact).
- No `crossterm` import appeared in the `app` layer.
- `git log --oneline -3` shows the two refactor commits + the spec commit.

---

## Self-Review

**Spec coverage:**
- Spec §"Change 1" (update.rs → update/{mod,modal}.rs, move L294–567, `handle_modal_key`
  `pub(super)`, open_* + tests stay) → Task 2. ✓
- Spec §"Change 2" (extract ui/util.rs: centered_rect/truncate_width/human_size + 2 tests) →
  Task 1. ✓
- Spec §"Out of scope" (runtime/config_edit/modal/i18n untouched) → no task touches them. ✓
- Spec §"Sequencing" (ui/util first, update second, gates between) → Task order 1→2, gates in
  every task. ✓
- Spec §"Invariants" → Task 3 Step 3 checklist. ✓
- Spec §"Risks" (git rename, no test moves in update, pub(super) only where needed) → Task 2
  Steps 2/6/7. ✓

**Placeholder scan:** No TBD/TODO/"handle edge cases". Verbatim-move steps cite exact files,
line ranges, and the single signature edit; full code shown for the genuinely-new `util.rs`.

**Type consistency:** `centered_rect`/`truncate_width`/`human_size` signatures identical to
source; `handle_modal_key(model: &mut Model, key: KeyEvent) -> Vec<Command>` matches the call
site at `handle_key`. `use util::{centered_rect, human_size, truncate_width}` matches the three
defined fns. `mod modal; use modal::handle_modal_key;` matches the `pub(super)` definition.
