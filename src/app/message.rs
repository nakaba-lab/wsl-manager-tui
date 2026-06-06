//! Core message types for the MVU loop.
//!
//! [`Event`] is what the runtime feeds in (terminal input + timer ticks).
//! [`Action`] is what the [`crate::app::update`] reducer consumes. [`Command`]s
//! returned by `update` describe side effects the runtime then executes.

use crossterm::event::KeyEvent;

use crate::wsl::Distro;

/// Low-level input delivered by the runtime to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key press. The runtime filters out key repeat/release events (common
    /// on Windows) so a single press is never handled twice.
    Key(KeyEvent),
    /// Terminal was resized to (width, height) columns/rows.
    Resize(u16, u16),
    /// Periodic timer tick. Drives polling and animations.
    Tick,
}

/// A message consumed by [`crate::app::update`] to advance the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// An input event produced by the runtime.
    Event(Event),
    /// The distro list was refreshed successfully.
    Refreshed(Vec<Distro>),
    /// A refresh attempt failed; carries a human-readable message.
    RefreshFailed(String),
    /// A lifecycle operation finished successfully (carries a status message).
    OpDone(String),
    /// A lifecycle operation failed (carries an error message).
    OpFailed(String),
    /// Request to quit the application.
    Quit,
}

/// A side effect requested by [`crate::app::update`] and executed by the
/// runtime as an async task; its result comes back as an [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Re-read the distro list (plus registry and disk metadata).
    RefreshList,
    /// Run a distribution lifecycle operation.
    Lifecycle(LifecycleOp),
}

/// A distribution lifecycle operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleOp {
    /// Boot the distro (`wsl -d <name> -- true`).
    Start(String),
    /// Stop this distro (`wsl --terminate <name>`).
    Terminate(String),
    /// Stop the whole WSL VM (`wsl --shutdown`).
    Shutdown,
    /// Make this distro the default (`wsl --set-default <name>`).
    SetDefault(String),
    /// Unregister (permanently delete) this distro (`wsl --unregister <name>`).
    Unregister(String),
}

impl LifecycleOp {
    /// A short verb for prompts and progress messages.
    pub fn verb(&self) -> &'static str {
        match self {
            LifecycleOp::Start(_) => "Start",
            LifecycleOp::Terminate(_) => "Terminate",
            LifecycleOp::Shutdown => "Shutdown",
            LifecycleOp::SetDefault(_) => "Set default",
            LifecycleOp::Unregister(_) => "Unregister",
        }
    }

    /// A success message shown after the operation completes.
    pub fn success_label(&self) -> String {
        match self {
            LifecycleOp::Start(name) => format!("Started {name}"),
            LifecycleOp::Terminate(name) => format!("Terminated {name}"),
            LifecycleOp::Shutdown => "WSL shut down".to_string(),
            LifecycleOp::SetDefault(name) => format!("Set {name} as default"),
            LifecycleOp::Unregister(name) => format!("Unregistered {name}"),
        }
    }
}
