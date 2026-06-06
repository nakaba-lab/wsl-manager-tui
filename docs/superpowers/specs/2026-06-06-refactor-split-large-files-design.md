# Refactor: Split large files (pinpoint) — Design

**Date:** 2026-06-06
**Status:** Approved (brainstorming)
**Type:** Pure structural refactoring — **no behavior change**

## Goal & scope

Reduce the cognitive load of the two largest hot files by splitting along clean
responsibility seams, **without changing any external or observable behavior**. This is a
structure-only refactor: the public API, the MVU contract, all command/effect routing, and
all test assertions stay byte-for-byte equivalent in behavior.

The scope is deliberately narrow. A multi-agent analysis (6 candidate files) plus an
adversarial architecture review concluded that **only `update.rs` genuinely warrants a
structural split**; most 257–466-line files in this project are healthy Rust modules, and
splitting them would add `pub(super)` churn and duplicated test scaffolding for no clarity
gain. We therefore do the one high-value split plus one small, low-risk utility extraction,
and explicitly leave everything else alone.

### In scope

1. `src/app/update.rs` (1155 lines) → directory module `src/app/update/{mod.rs, modal.rs}`.
2. `src/ui/mod.rs` (699 lines) → extract pure utilities to `src/ui/util.rs`.

### Out of scope (deliberately not split)

- `src/runtime/mod.rs` (466) — async core is cohesive; `dispatch`/`handlers` share one
  signature protocol (one concern, not two). Splitting adds `MockBackend` visibility juggling
  and test migration for no behavior gain.
- `src/app/config_edit.rs` (292) — `RawEditor` is a component with `ConfigEditState` as its
  sole consumer, same domain. Not two concerns sharing a file.
- `src/app/modal.rs` (257) — already cohesive; the shared public type surface for `update`
  and `ui`.
- `src/i18n/mod.rs` (476) — a flat `(en, ja)` catalog; no responsibility seam.

If any of these later cross ~450 production lines or grow a second consumer, revisit. Until
then, section comments + consistent ordering are the right tool, not new modules.

## Change 1 — `src/app/update.rs` → `src/app/update/`

Convert the file into a directory module:

```
src/app/update/
  mod.rs     # reducer entry, event/list handling, open_* builders, ALL tests
  modal.rs   # modal key/submit handler cluster
```

### Moves to `update/modal.rs` (modal input-handling cluster, current L294–567)

| fn | current line |
|---|---|
| `handle_modal_key` | 294 |
| `handle_quit_key` | 312 |
| `handle_confirm_key` | 320 |
| `confirm_action` | 365 |
| `handle_form_key` | 375 |
| `submit_form` | 406 |
| `handle_progress_key` | 456 |
| `handle_install_key` | 470 |
| `handle_config_key` | 515 |
| `config_form_key` | 538 |
| `config_raw_key` | 556 |

This cluster is self-contained: `handle_modal_key` is the single entry point, and it dispatches
to the rest, which call each other (`handle_confirm_key`→`confirm_action`,
`handle_form_key`→`submit_form`, `handle_config_key`→`config_form_key`/`config_raw_key`) — all
within `modal.rs`.

### Stays in `update/mod.rs`

- Entry + event dispatch: `update` (pub), `update_event`, `handle_key`, `handle_filter_key`,
  `handle_list_key`.
- **`open_*` builders next to their list-key callers** (`open_export_form`, `open_import_form`,
  `open_confirm_terminate`, `open_confirm_shutdown`, `open_confirm_unregister`). These are
  invoked from `handle_list_key`, not from the modal *key* handlers, so they stay in `mod.rs`
  to keep "open a confirm" beside "decide to open it" and to minimise cross-file visibility.
- Supporting helpers: `load_config`, `load_wslconf`, `selected_name`, `start_selected`,
  `set_default_selected`, `launch_shell`, `launch_inline`, `launch_tab`.
- **`sample_inner_disk_if_needed`** — the once-per-distro `df` rule lives here and **stays in
  `mod.rs`**, so the df-once invariant is physically untouched.
- **All ~587 lines of tests** and their shared helpers (`key()`, `ch()`, `distro()`,
  `model_with()`, `online()`, `config_loaded()`, `running_distro()`). The pure-reducer test
  suite is the project's headline asset and exercises modal flows *through `update()` Actions*,
  not by calling the moved fns directly — so it does not need to move and must not be split.

### Visibility

- `handle_modal_key` becomes **`pub(super)`** (called by `handle_key` in `mod.rs`).
- Every other moved fn is called only from within `modal.rs` → stays private.
- `modal.rs` imports shared types (`Model`, `Action`, `Command`, `Event`, `KeyEvent`, the
  modal types, `ConfigEditState`, etc.) from `crate::app::*` as today.
- `mod.rs` adds `mod modal;`. No `lib.rs` or `app/mod.rs` change — `update` stays the only
  public item, re-exported exactly as before.

## Change 2 — `src/ui/mod.rs` → extract `src/ui/util.rs`

```
src/ui/
  mod.rs    # view() and all render_* (modal renderers stay here)
  util.rs   # centered_rect, truncate_width, human_size + their 2 tests
```

### Moves to `ui/util.rs`

| item | current line |
|---|---|
| `centered_rect` | 432 |
| `truncate_width` | 445 |
| `human_size` | 468 |
| test `truncate_width_respects_cjk_columns` | 671 |
| test `human_size_formats` | 680 |

Both moved tests assert against the pure functions directly and do **not** use the `render()`
/ `sample()` / `distro_named()` test helpers, so they move cleanly without duplicating any
scaffold. (`centered_rect` has no dedicated test — it stays covered indirectly by the modal
render tests that remain in `mod.rs`.)

Rationale: give `truncate_width` (the CJK display-width invariant) a named home with its own
tests. The three functions become `pub(super)`; `mod.rs` adds `mod util;` and
`use util::{centered_rect, truncate_width, human_size};` (or path-qualified calls).

**Not moved:** the 10 modal renderers (`render_modal`, `render_help`, `render_quit`,
`render_config_edit`, `render_config_form`, `render_config_raw`, `render_form`,
`render_progress`, `render_install_pick`, `render_confirm`, `render_error`). They are trivial
ratatui layout fns sharing the util helpers; extracting them would force the utilities public
*and* duplicate the render test scaffolding (`render()`/`sample()`) per file — churn, not
clarity. They stay in `mod.rs`.

## Invariants preserved (all changes)

- **Pure-reducer contract:** `update()` and every moved handler emit `Command`s only — no IO,
  no terminal, no async.
- **App layer stays crossterm-free** (its own `KeyCode`/`KeyMods` in `app/input.rs`).
- **UI never mutates state**; CJK width math (`truncate_width`) stays display-column based.
- **No circular deps:** parent `mod.rs` → child submodule only.
- **df-once rule** stays in `update/mod.rs` (`sample_inner_disk_if_needed`).
- `.gitattributes` LF normalisation and `tests/fixtures/*` are untouched.

## Sequencing & verification

Each step compiles and tests independently. Run all three gates green **before** moving on:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

1. **Step 1 — `ui/util.rs`** (smallest, lowest risk). Extract the 3 utilities + 2 tests; add
   `mod util;` + imports; adjust visibility to `pub(super)`. Verify gates green.
2. **Step 2 — `update.rs` → `update/{mod.rs, modal.rs}`** (largest test surface, done last so
   any regression is caught against an already-green tree). `git mv update.rs update/mod.rs`
   (so git tracks the rename), create `modal.rs`, move the cluster, mark `handle_modal_key`
   `pub(super)`, add `mod modal;`. Verify gates green.

Final check: full `cargo test --all` green, and `git diff` shows only moves + the minimal
`mod`/visibility lines — no logic changes.

## Risks & mitigations

- **Accidental over-exposure of a test helper** during migration → keep all `update` tests in
  `mod.rs` (no test moves there); only `ui/util` tests move, with their fns.
- **git treating the file→dir conversion as delete+add** → use `git mv` for `update.rs` →
  `update/mod.rs`; confirm rename detection in `git status`.
- **Hidden caller of a moved fn** → compiler catches it; resolve with the documented
  `pub(super)` (only `handle_modal_key` and the three ui utilities are expected to need it).
