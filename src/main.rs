//! `wslm` — entry point for the WSL Manager TUI.
//!
//! Kept intentionally thin: it installs error/panic handling and hands off to
//! the library runtime. The real logic lives in the `wsl_manager_tui` crate.

use wsl_manager_tui::runtime;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    runtime::install_panic_hook();
    runtime::run().await
}
