//! Modal dialog state overlaid on the main list.

use super::message::LifecycleOp;

/// An overlay dialog on top of the main list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// Confirm a (possibly destructive) operation.
    Confirm(Confirm),
    /// Show an error; dismissed by any key.
    Error {
        /// The error text to display.
        message: String,
    },
}

/// A confirmation dialog. Destructive operations may require typing the distro
/// name to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    /// The operation to run if confirmed.
    pub op: LifecycleOp,
    /// The prompt shown to the user.
    pub prompt: String,
    /// When set, the user must type the expected name to proceed.
    pub require_typed: Option<TypedConfirm>,
}

/// State for a "type the name to confirm" guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedConfirm {
    /// The exact text that must be typed (the distro name).
    pub expected: String,
    /// What the user has typed so far.
    pub input: String,
}

impl TypedConfirm {
    /// True when the typed input matches the expected name.
    pub fn matches(&self) -> bool {
        self.input == self.expected
    }
}
