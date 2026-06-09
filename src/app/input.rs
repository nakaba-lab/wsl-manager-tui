//! Terminal-independent key input types. The runtime translates crossterm
//! events into these so the application layer never depends on the terminal
//! backend (keeping the `app` module backend-agnostic and easily testable).

/// A key code (the subset the application acts on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// A character key.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Any other key the app does not act on.
    Other,
}

/// Active keyboard modifiers (only the ones the app cares about).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyMods {
    /// Control held.
    pub ctrl: bool,
    /// Shift held.
    pub shift: bool,
}

impl KeyMods {
    /// No modifiers.
    pub const NONE: KeyMods = KeyMods {
        ctrl: false,
        shift: false,
    };
    /// Control only.
    pub const CONTROL: KeyMods = KeyMods {
        ctrl: true,
        shift: false,
    };
    /// Shift only.
    pub const SHIFT: KeyMods = KeyMods {
        ctrl: false,
        shift: true,
    };

    /// Whether every modifier set in `other` is also set in `self`.
    pub fn contains(self, other: KeyMods) -> bool {
        (!other.ctrl || self.ctrl) && (!other.shift || self.shift)
    }
}

/// A key press: a code plus the active modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    /// The key code.
    pub code: KeyCode,
    /// The active modifiers.
    pub modifiers: KeyMods,
}

impl KeyPress {
    /// Construct a key press.
    pub fn new(code: KeyCode, modifiers: KeyMods) -> Self {
        Self { code, modifiers }
    }
}
