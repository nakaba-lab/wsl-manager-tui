# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`wslm` — a terminal UI (ratatui) to manage WSL distributions. **Windows-only**: it shells out to
`wsl.exe`, reads the `HKCU\...\Lxss` registry hive, and launches Windows Terminal. It will not build
meaningfully or run on Linux/macOS. CI runs on `windows-latest`.

## Commands

The toolchain is **pinned to 1.96.0** via `rust-toolchain.toml` (with `rustfmt` + `clippy`); rustup
installs it automatically.

```sh
cargo run                                   # build and run the wslm binary
cargo build --release                       # single stripped LTO exe → target/release/wslm.exe
cargo fmt --all -- --check                  # format check (CI gate)
cargo clippy --all-targets -- -D warnings   # lint; WARNINGS ARE ERRORS (CI gate)
cargo test --all                            # all non-ignored tests
```

Running one test or module (tests are colocated with code in `#[cfg(test)]` blocks):

```sh
cargo test parses_japanese_identically      # by test-fn name (substring)
cargo test --lib app::update::tests         # a whole module's tests
```

Tests that touch the real machine (`wsl.exe`, registry, process list) are `#[ignore]`d so default
`cargo test` and CI skip them. Run them explicitly on a real WSL host:

```sh
cargo test -- --ignored                     # all ignored tests
cargo test --test real_wsl -- --ignored     # the integration test only
```

A `v*` git tag triggers `release.yml`, which builds and publishes `wslm.exe` as a GitHub release.

## Architecture

### Crate split

A library crate `wsl_manager_tui` (`src/lib.rs`) holds **all** the logic; the `wslm` binary
(`src/main.rs`) is intentionally thin — parse CLI flags, load prefs, install error/panic hooks, call
`runtime::run`. The lib's module doc lists the layers that **must stay terminal-/IO-independent**:
`wsl`, `registry`, `metrics`, `config`, `app`, `i18n`, `prefs`. Only `runtime` and `ui` may know about
the terminal.

### MVU loop — the central contract

The app is a Model–View–Update loop. Three message types (`src/app/message.rs`) define the boundary:

- **`Event`** — what the runtime feeds in: `Key` / `Resize` / `Tick` / `Frame`.
- **`Action`** — what the reducer consumes: an `Event`, *or* the result of a finished side effect
  (`Refreshed`, `MetricsSampled`, `InnerDiskSampled`, `OnlineList`, `ConfigLoaded`, `OpDone`,
  `OpFailed`, `Quit`).
- **`Command`** — a side effect *described* by the reducer for the runtime to execute.

`update(&mut Model, Action) -> Vec<Command>` (`src/app/update.rs`) is a **pure reducer**: no IO, no
terminal, no async. It is the main test surface and is exhaustively unit-tested. Never put IO or
terminal calls here — emit a `Command` instead and let the runtime perform the effect; its result
returns as an `Action`.

```
crossterm/timer ─► Event ─┐
async task result ───► Action ─► update(model, action) ─► Vec<Command> ─► runtime executes
        ▲                                                                        │
        └──────────────── result Action sent back over mpsc channel ◄────────────┘
```

### Runtime event loop (`src/runtime/mod.rs`)

`event_loop` runs `tokio::select!` over four sources: the crossterm `EventStream`, a **poll `tick`**
(every `poll_interval` secs → `Event::Tick`, which emits `RefreshList` + `SampleMetrics`), a **`frame`
tick** (120 ms → `Event::Frame`, which only advances the progress spinner — *not* a poll), and an
unbounded mpsc `Action` channel that carries async results back.

Most `Command`s are run by `dispatch`, which spawns a tokio task and sends the resulting `Action` back
over the channel. **Some commands are handled inline in the loop instead** (the dispatcher returns
`{}` for them):

- `LaunchInlineShell` — needs the console, so it `suspend`s the TUI, runs `wsl -d <name>`, then
  `resume`s. Runs in the loop, awaited, not spawned.
- `Export` / `Import` / `Install` — long-running and cancellable; spawned with an `AbortHandle` kept in
  `current_op`. `CancelOp` (or a superseding op) aborts it; the child is killed via `kill_on_drop`.
- `LaunchTabShell`, `SavePrefs` — small synchronous effects done in place.

### `wsl.exe` backend abstraction (`src/wsl/backend.rs`)

All `wsl.exe` interaction goes through the `WslBackend` **trait**. `RealWslBackend` shells out;
tests drive the app with a `MockBackend`. The runtime holds it as `Arc<dyn WslBackend>`. To add a WSL
operation, add a trait method + real impl + a `Command` variant + reducer wiring + runtime dispatch.

### Locale-independent state detection (a headline feature)

WSL's CLI output is **localized**, so the code never trusts displayed status text. This is spread
across three files and must be preserved:

- `src/wsl/decode.rs` — `wsl.exe`'s own output is **UTF-16LE** (BOM or heuristic) → `decode_wsl_output`.
  In-distro command output (`wsl -d <d> -- ...`) is the Linux side's **UTF-8** → `decode_utf8`. Don't
  mix these up.
- `src/wsl/parse.rs` — `parse_list_verbose` **ignores the STATE column entirely**, skips the header by
  requiring the last token to be a numeric version, and reads the default flag from a leading `*`.
- `src/wsl/collect.rs` — running state is derived **separately**, from membership in the
  `wsl --list --running -q` set, then merged with parsed rows + registry data in `collect_distros`
  (pure; the main test surface for this layer). `refresh` adds the IO.

Registry data (`src/registry/mod.rs`, read-only `HKCU\...\Lxss`) supplies GUID, base path, and the
vhdx filename → used for the on-disk size (`metrics::disk_size` = vhdx file length) and path display.
Registry failures are tolerated (distros just lack paths/sizes).

### In-distro `df` is sampled at most ONCE per distro — do not regress this

Running `df` inside a distro on every poll made the app sluggish (see commits `40cf668`, `f996033`).
The fix: `Model.inner_disk_attempted` (a `HashSet`) plus `sample_inner_disk_if_needed` in the reducer
only request `SampleInnerDisk` the first time a running distro is selected, and the value is carried
forward across refreshes. Keep polling cheap — never add per-poll in-distro commands.

### Config editing (`src/config/` + `src/app/config_edit.rs`)

`IniDoc` (`src/config/ini.rs`) is a comment-/blank-/unknown-key-/order-preserving INI parser that
round-trips on parse→render; `set` with an empty value *removes* the key. The editor modal
(`ConfigEditState`) has a **Form** view (known-key schema fields from `config::schema`) and a **Raw**
multi-line view, kept in sync through the `IniDoc` when toggled. Saving always preserves unknown keys
and comments.

- `.wslconfig` lives Windows-side (`%USERPROFILE%\.wslconfig`), read/written via the filesystem;
  backed up to `.wslconfig.bak`.
- `wsl.conf` lives in-distro (`/etc/wsl.conf`), read/written **by the backend as root** (`cat`/`tee`);
  backed up to `/etc/wsl.conf.bak`.

### i18n (`src/i18n/mod.rs`)

All user-facing text is keyed by the `Key` enum; `entry()` returns an `(en, ja)` tuple; `t()` returns
the static string and `tf()` does positional `{}` substitution. **Never hardcode UI strings** — add a
`Key` and route through `t`/`tf`. `Key::ALL` + the `every_key_has_both_languages` test enforce that
both languages are filled in for every key; keep `Key::ALL` in sync when adding keys. Language toggles
at runtime (`L`) and is persisted.

### Preferences (`src/prefs/mod.rs`)

Persisted to `%APPDATA%\wsl-manager-tui\config.toml` (TOML, serde with `#[serde(default)]` so missing
fields fall back to defaults). `lang: Option<Lang>` where `None` = auto-detect from env. CLI flags in
`main.rs` override loaded prefs before the loop starts.

### Terminal lifecycle & crossterm decoupling

- `src/runtime/tui.rs` — RAII `Tui` wrapper. `exit()` is idempotent (guarded by
  `is_raw_mode_enabled`) and restoration runs both on `Drop` and from a **panic hook** installed after
  `color_eyre::install`. `panic = "unwind"` is kept deliberately (see `Cargo.toml`) so these guards
  fire. `suspend`/`resume` wrap the inline-shell handoff.
- The **`app` layer must not depend on crossterm** (spec 4.2, commit `fa2d219`). It defines its own
  `KeyCode`/`KeyMods`/`KeyPress` in `src/app/input.rs`; the runtime's `convert_key` translates
  crossterm events into them. `map_event` forwards only `KeyEventKind::Press` so Windows key
  repeat/release events aren't handled twice.
- `src/ui/mod.rs` renders `Model` → ratatui and **never mutates state**. Width math is CJK-aware via
  `unicode-width` (`truncate_width`, commit `adfc1fb`) — use display columns, not byte/char counts,
  for any terminal-width truncation. UI tests render against ratatui's `TestBackend`.

## Conventions

- `.gitattributes` normalizes all text to **LF**; `tests/fixtures/*.bin` are captured UTF-16LE
  `wsl.exe` output stored as **binary** — never re-encode or "fix" them.
- The library error type is `WslError` (`src/error.rs`, `thiserror`), with `Result<T>` aliased to
  `Result<T, WslError>`; the binary surfaces these as `color_eyre` reports.
- The full design spec lives at `docs/superpowers/specs/2026-06-06-wsl-manager-tui-design.md`.
