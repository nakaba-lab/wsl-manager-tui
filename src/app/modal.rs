//! Modal dialog state overlaid on the main list.

use super::config_edit::ConfigEditState;
use super::message::Command;
use crate::wsl::OnlineDistro;

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
    /// A text-entry form (export/import).
    Form(FormState),
    /// An indeterminate progress dialog for a long-running operation.
    Progress(ProgressState),
    /// A filterable picker of installable distributions.
    InstallPick(InstallPickState),
    /// The configuration editor (`.wslconfig` / `wsl.conf`).
    ConfigEdit(ConfigEditState),
    /// The keybinding help overlay.
    Help,
    /// The quit confirmation dialog.
    Quit,
}

/// A confirmation dialog. Destructive operations may require typing the distro
/// name to proceed. On confirmation, [`Confirm::on_confirm`] commands are run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    /// The prompt shown to the user.
    pub prompt: String,
    /// When set, the user must type the expected name to proceed.
    pub require_typed: Option<TypedConfirm>,
    /// Commands to dispatch when confirmed.
    pub on_confirm: Vec<Command>,
    /// If set, a progress dialog with this title opens on confirmation.
    pub progress_title: Option<String>,
    /// If set, this status message is shown on confirmation.
    pub status: Option<String>,
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

/// A single-line text input (cursor fixed at the end).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    /// The current text value.
    pub value: String,
}

impl TextField {
    /// A field pre-filled with `initial`.
    pub fn new(initial: impl Into<String>) -> Self {
        Self {
            value: initial.into(),
        }
    }

    /// Append a character.
    pub fn insert(&mut self, c: char) {
        self.value.push(c);
    }

    /// Remove the last character.
    pub fn backspace(&mut self) {
        self.value.pop();
    }
}

/// Which form is being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    /// Export the named distro.
    Export {
        /// The distro being exported.
        distro: String,
    },
    /// Import a new distro.
    Import,
}

/// State for a multi-field text form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormState {
    /// Which form this is.
    pub kind: FormKind,
    /// Field labels (parallel to [`FormState::fields`]).
    pub labels: Vec<&'static str>,
    /// The editable fields.
    pub fields: Vec<TextField>,
    /// Index of the focused field.
    pub focus: usize,
}

impl FormState {
    /// An export form with a pre-filled output path.
    pub fn export(distro: String, default_path: String) -> Self {
        Self {
            kind: FormKind::Export { distro },
            labels: vec!["Output .tar path"],
            fields: vec![TextField::new(default_path)],
            focus: 0,
        }
    }

    /// An import form (name, install directory, source tar).
    pub fn import() -> Self {
        Self {
            kind: FormKind::Import,
            labels: vec!["New distro name", "Install directory", "Source .tar path"],
            fields: vec![
                TextField::default(),
                TextField::default(),
                TextField::default(),
            ],
            focus: 0,
        }
    }

    /// Move focus to the next field (wrapping).
    pub fn focus_next(&mut self) {
        self.focus = (self.focus + 1) % self.fields.len();
    }

    /// Move focus to the previous field (wrapping).
    pub fn focus_prev(&mut self) {
        self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
    }

    /// The focused field.
    pub fn current_mut(&mut self) -> &mut TextField {
        &mut self.fields[self.focus]
    }

    /// The trimmed value of field `index`.
    pub fn value(&self, index: usize) -> &str {
        self.fields[index].value.as_str()
    }
}

/// State for an indeterminate progress dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressState {
    /// The operation description (e.g. "Exporting 'Debian'").
    pub title: String,
    /// Animation frame counter.
    pub frame: usize,
}

impl ProgressState {
    /// A new progress dialog with the given title.
    pub fn new(title: String) -> Self {
        Self { title, frame: 0 }
    }

    /// Advance the spinner animation.
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The current spinner glyph.
    pub fn spinner(&self) -> char {
        const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        FRAMES[self.frame % FRAMES.len()]
    }
}

/// State for the install picker (filterable list of installable distros).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPickState {
    /// All available distributions.
    pub items: Vec<OnlineDistro>,
    /// The current filter text.
    pub filter: String,
    /// Selected index into the filtered list.
    pub selected: usize,
}

impl InstallPickState {
    /// A picker over `items`.
    pub fn new(items: Vec<OnlineDistro>) -> Self {
        Self {
            items,
            filter: String::new(),
            selected: 0,
        }
    }

    /// The items matching the current filter (case-insensitive substring).
    pub fn filtered(&self) -> Vec<&OnlineDistro> {
        let needle = self.filter.to_ascii_lowercase();
        self.items
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || item.name.to_ascii_lowercase().contains(&needle)
                    || item.friendly.to_ascii_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Move selection down within the filtered list.
    pub fn select_next(&mut self) {
        let count = self.filtered().len();
        if count > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        }
    }

    /// Move selection up within the filtered list.
    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// The install id of the selected item, if any.
    pub fn selected_name(&self) -> Option<String> {
        self.filtered()
            .get(self.selected)
            .map(|item| item.name.clone())
    }

    /// Append to the filter and reset the selection.
    pub fn push_filter(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    /// Remove the last filter character and reset the selection.
    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }
}
