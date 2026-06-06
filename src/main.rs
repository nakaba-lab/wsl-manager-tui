//! `wslm` — entry point for the WSL Manager TUI.
//!
//! Kept intentionally thin: it installs error/panic handling and hands off to
//! the library runtime. The real logic lives in the `wsl_manager_tui` crate.

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    // The interactive runtime is wired up in milestone M1.
    Ok(())
}
