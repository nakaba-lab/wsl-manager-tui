//! Terminal wrapper (RAII) and the panic hook that restores the terminal.

use std::io::{self, Stdout};

use color_eyre::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};

/// Concrete backend: crossterm writing to stdout.
pub type Backend = CrosstermBackend<Stdout>;

/// Owns the ratatui terminal and manages entering/leaving the alternate screen
/// and raw mode. Restoration is idempotent and also runs on `Drop`, so the
/// terminal is left usable even on early return or panic.
pub struct Tui {
    terminal: Terminal<Backend>,
}

impl Tui {
    /// Create the terminal handle (does not yet switch into the TUI screen).
    pub fn new() -> Result<Self> {
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Self { terminal })
    }

    /// Enter raw mode + the alternate screen and hide the cursor.
    pub fn enter(&mut self) -> Result<()> {
        enable_raw_mode()?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        self.terminal.clear()?;
        Ok(())
    }

    /// Restore the terminal to its original state. Idempotent: safe to call from
    /// a panic hook, from `Drop`, and after [`Tui::suspend`].
    pub fn exit(&mut self) -> Result<()> {
        if is_raw_mode_enabled()? {
            disable_raw_mode()?;
            crossterm::execute!(io::stdout(), LeaveAlternateScreen, Show)?;
        }
        Ok(())
    }

    /// Leave the TUI so an external interactive process (e.g. a WSL shell) can
    /// take over the terminal. Wired into inline-shell launch in M4.
    pub fn suspend(&mut self) -> Result<()> {
        self.exit()
    }

    /// Re-enter the TUI after [`Tui::suspend`].
    pub fn resume(&mut self) -> Result<()> {
        self.enter()
    }

    /// Render a single frame.
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> Result<()> {
        self.terminal.draw(render)?;
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.exit();
    }
}

/// Install a panic hook that restores the terminal before the previously
/// installed hook (e.g. color-eyre's report) runs. Call *after*
/// `color_eyre::install`.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen, Show);
        previous(info);
    }));
}
