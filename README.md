# wsl-manager-tui (`wslm`)

A terminal UI (TUI) to manage Windows Subsystem for Linux (WSL) distributions:
list, start/stop, launch shells, monitor resources, and edit configuration —
all from a single screen.

> Status: early development. See
> [`docs/superpowers/specs/2026-06-06-wsl-manager-tui-design.md`](docs/superpowers/specs/2026-06-06-wsl-manager-tui-design.md)
> for the design and
> [the implementation plan](.) for milestones.

## Requirements

- Windows 10/11 with WSL2
- Rust toolchain **1.96+** (pinned via `rust-toolchain.toml`)

## Build & run

```sh
cargo run            # runs the `wslm` binary
cargo build --release
```

The release build produces a single `target/release/wslm.exe`.

## Development

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Integration tests that invoke the real `wsl.exe` are marked `#[ignore]` and are
not run by default.

## License

MIT — see [LICENSE](LICENSE).
