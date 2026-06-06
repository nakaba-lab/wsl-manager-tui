//! The application model (state). It grows with each milestone.

use crate::wsl::Distro;

/// The full application state. Rendered by [`crate::ui`] and mutated only by
/// [`crate::app::update`].
#[derive(Debug, Default, Clone)]
pub struct Model {
    /// Set to true when the app should exit its event loop.
    pub should_quit: bool,
    /// Number of timer ticks observed (drives polling).
    pub ticks: u64,
    /// The registered distributions, as of the last refresh.
    pub distros: Vec<Distro>,
    /// Index of the selected row in [`Model::distros`].
    pub selected: usize,
    /// The most recent error message, if any.
    pub last_error: Option<String>,
    /// False until the first refresh completes (drives a "loading" hint).
    pub loaded: bool,
}

impl Model {
    /// The currently selected distribution, if the list is non-empty.
    pub fn selected_distro(&self) -> Option<&Distro> {
        self.distros.get(self.selected)
    }

    /// Move the selection down by one, clamped to the last row.
    pub(crate) fn select_next(&mut self) {
        if !self.distros.is_empty() {
            self.selected = (self.selected + 1).min(self.distros.len() - 1);
        }
    }

    /// Move the selection up by one, clamped to the first row.
    pub(crate) fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Keep the selection index within the bounds of the current list.
    pub(crate) fn clamp_selection(&mut self) {
        if self.distros.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.distros.len() {
            self.selected = self.distros.len() - 1;
        }
    }
}
