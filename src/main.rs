//! `wslm` — entry point for the WSL Manager TUI.
//!
//! Kept intentionally thin: it parses CLI flags, loads preferences, installs
//! error/panic handling, and hands off to the library runtime.

use clap::Parser;
use wsl_manager_tui::i18n::Lang;
use wsl_manager_tui::{prefs, runtime};

/// A terminal UI to manage WSL distributions.
#[derive(Parser)]
#[command(name = "wslm", version, about)]
struct Cli {
    /// UI language (overrides the saved preference).
    #[arg(long, value_enum)]
    lang: Option<Lang>,
    /// Polling interval in seconds (overrides the saved preference).
    #[arg(long)]
    poll_interval: Option<u64>,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();
    let mut prefs = prefs::load();
    if let Some(lang) = cli.lang {
        prefs.lang = Some(lang);
    }
    if let Some(interval) = cli.poll_interval {
        prefs.poll_interval_secs = interval;
    }

    runtime::install_panic_hook();
    runtime::run(prefs).await
}
