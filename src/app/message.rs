//! Core message types for the MVU loop.
//!
//! [`Event`] is what the runtime feeds in (terminal input + timer ticks).
//! [`Action`] is what the [`crate::app::update`] reducer consumes. Side-effecting
//! `Command`s returned by `update` are introduced in later milestones (M2+).

use crossterm::event::KeyEvent;

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
    /// Request to quit the application.
    Quit,
}
