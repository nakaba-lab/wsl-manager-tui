# wsl-manager-tui (`wslm`)

[![CI](https://github.com/nakaba-lab/wsl-manager-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/nakaba-lab/wsl-manager-tui/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-orange)

## Sponsor

`wslm` is free and open source. If it saves you time, you can support continued
development:

- **[GitHub Sponsors](https://github.com/sponsors/nakata5577)** — monthly or one-time
- **[Ko-fi](https://ko-fi.com/nakata5577)** — buy me a coffee (one-time)

A terminal UI to manage Windows Subsystem for Linux (WSL) distributions —
list, start/stop, launch shells, monitor memory and CPU, transfer, and edit
configuration — all from a single screen.

```text
╭ wslm — WSL Manager ────────────────────────────────────  EN ╮
│  NAME           STATE       VER  DEFAULT  DISK               │
│ ▌Debian         ● Running    2      ★      4.2 GB            │
│  Ubuntu-24.04   ○ Stopped    2             8.1 GB            │
╰─────────────────────────────────────────────────────────────╯
╭ Debian ─────────────────────────────────────────────────────╮
│ State    ● Running              Default  ★                   │
│ Version  2                      Disk     4.2 GB              │
│ Path     C:\Users\me\…\Debian\LocalState                    │
│ VM Mem   1.8 GB / 8.0 GB  ▕██████░░░░░░░░▏ 23%               │
│ VM CPU   4.7 %            ▕█░░░░░░░░░░░░░▏                    │
│ Trend    Mem ▁▂▃▅▇▆▄▃     CPU ▁▁▂▁▃▂▁▁                       │
╰─────────────────────────────────────────────────────────────╯
 ⏎ shell · w tab · x stop · d default · e export · m import
 ↑↓/jk move · / filter · ? help · q quit
```

The detail pane, footer, and theme are illustrative — colors (a teal accent,
green/amber/red gauges) don't show here. The bottom two lines are a
**context footer** (the actions available for the selected distro's state) and
a **global hint line**.

## Features

- **List** every registered distro with state, WSL version, default flag and
  on-disk size. State is detected without parsing localized strings, so it is
  correct under any Windows display language.
- **Lifecycle**: start, stop (terminate), shut down the whole VM, set default,
  and unregister (with a type-the-name confirmation).
- **Shells**: open a shell inline (the TUI suspends and resumes on exit) or in
  a new Windows Terminal tab.
- **Managed export/import**: `e` exports the selected distro into
  `%USERPROFILE%\wsl-manager\exports\` (the file extension selects the format:
  `.tar`, `.tar.gz`, `.tar.xz`, or `.vhdx`); `m` opens a picker over that
  folder — press `c` for a custom archive path or `d` to delete an entry.
  Imported distros are stored under `installed\<name>\` inside the same managed
  root (configurable via `manage_dir` in `config.toml`). Long operations show a
  cancellable progress dialog. Install new distros from the online catalog with
  `i`.
- **Resource monitor**: WSL VM memory and CPU usage as inline gauges with live
  sparklines (the WSL2 VM is shared by all distros, so these are machine-wide).
  When a running distro is selected, its in-distro disk usage (`df`) is sampled
  once and shown too.
- **Config editor**: edit `.wslconfig` and a distro's `wsl.conf` with a
  known-key form or raw text; unknown keys and comments are preserved, and the
  previous file is backed up to `*.bak`.
- **Bilingual UI**: switch between English and Japanese at runtime (`L`).
- **Filter** the list incrementally (`/`) and an in-app help overlay (`?`).

## Keybindings

| Key | Action |
| --- | --- |
| `j` / `k`, `↑` / `↓` | Move selection |
| `/` | Filter the list (`Esc` clears) |
| `Enter` | Open an inline shell (exit returns to `wslm`) |
| `w` / `Shift+Enter` | Open a shell in a new Windows Terminal tab |
| `s` | Start (boot) the distro |
| `x` | Stop (terminate) the distro |
| `X` | Shut down the whole WSL VM |
| `d` | Set as default |
| `u` | Unregister — delete (type the name to confirm) |
| `e` | Export to the managed folder (extension selects format) |
| `m` | Import — pick from the managed folder (`c` custom path, `d` delete) |
| `i` | Install from the online catalog |
| `c` / `C` | Edit `.wslconfig` / `wsl.conf` |
| `L` | Toggle English / Japanese |
| `r` | Refresh now |
| `?` | Help |
| `q` | Quit (`Ctrl+C` forces quit) |

In the config editor: `Tab` toggles the form/raw view, `Ctrl+S` saves.

## Requirements

- Windows 10/11 with WSL2
- Rust toolchain **1.96+** (pinned via `rust-toolchain.toml`)

## Install / build

### Download a release

Grab the prebuilt `wslm.exe` from the
[Releases](https://github.com/nakaba-lab/wsl-manager-tui/releases) page (each
`v*` tag publishes one), drop it anywhere on your `PATH`, and run `wslm`. It is
a single self-contained executable with no install step.

### Install with Scoop

This repository doubles as its own [Scoop](https://scoop.sh) bucket:

```powershell
scoop bucket add wslm https://github.com/nakaba-lab/wsl-manager-tui
scoop install wslm
```

`scoop update wslm` upgrades to the latest release.

### Build from source

```sh
cargo run            # build and run `wslm` from source
cargo build --release
```

The release build produces a single, dependency-free `target/release/wslm.exe`
(stripped, LTO).

### Options

```text
wslm --lang <en|ja>          # override the saved UI language
wslm --poll-interval <secs>  # override the refresh interval
```

Preferences (language, poll interval, history length, keybinding style, and the
default `Enter` shell-launch mode) are saved to
`%APPDATA%\wsl-manager-tui\config.toml`. For example:

```toml
lang = "ja"
poll_interval_secs = 2
keybind_style = "both"          # both | arrows-only | vim-only
default_shell_launch = "inline" # inline | new-tab
```

## Development

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Integration tests that invoke the real `wsl.exe` / registry are marked
`#[ignore]`; run them with `cargo test -- --ignored`.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as below, without any additional terms or conditions.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
