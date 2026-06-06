# wsl-manager-tui (`wslm`)

A terminal UI to manage Windows Subsystem for Linux (WSL) distributions —
list, start/stop, launch shells, monitor memory, transfer, and edit
configuration — all from a single screen.

```
┌ WSL Manager (wslm) · EN ─────────────────────────────────────┐
│  NAME           STATE        VER  DEFAULT  DISK               │
│ ▶Debian         ● Running     2    ★       4.2 GB             │
│  Ubuntu-24.04   ○ Stopped     2            8.1 GB             │
├ Detail: Debian ──────────────────────────────────────────────┤
│ State: Running    Default: ★                                 │
│ Version: 2        Disk: 4.2 GB                                │
│ Path: C:\Users\…\Debian                                      │
│ VM Mem: 1.8 GB / 8.0 GB (vmmemWSL, shared by all distros)    │
│ ▁▂▃▅▇▆▄▃▂▃▅▆                                                 │
├──────────────────────────────────────────────────────────────┤
│ j/k · Enter shell · s start · x stop · … · q quit            │
└──────────────────────────────────────────────────────────────┘
```

## Features

- **List** every registered distro with state, WSL version, default flag and
  on-disk size. State is detected without parsing localized strings, so it is
  correct under any Windows display language.
- **Lifecycle**: start, stop (terminate), shut down the whole VM, set default,
  and unregister (with a type-the-name confirmation).
- **Shells**: open a shell inline (the TUI suspends and resumes on exit) or in
  a new Windows Terminal tab.
- **Transfer**: export to / import from a `.tar`, and install new distros from
  the online catalog — long operations show a cancellable progress dialog.
- **Resource monitor**: WSL VM memory with a live sparkline (the WSL2 VM is
  shared by all distros).
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
| `e` | Export to a `.tar` backup |
| `m` | Import from a `.tar` |
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

```sh
cargo run            # build and run `wslm` from source
cargo build --release
```

The release build produces a single, dependency-free `target/release/wslm.exe`.

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

## License

MIT — see [LICENSE](LICENSE).
