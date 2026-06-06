//! The application model (state). It grows with each milestone.

use std::collections::HashSet;
use std::path::PathBuf;

use super::modal::Modal;
use crate::i18n::Lang;
use crate::metrics::MetricsHistory;
use crate::prefs::{KeybindStyle, ShellLaunch};
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
    /// Index of the selected row within the *visible* (filtered) list.
    pub selected: usize,
    /// Case-insensitive name filter (empty = no filter).
    pub filter: String,
    /// Whether keystrokes are currently editing the filter.
    pub filter_mode: bool,
    /// The most recent background error (e.g. a failed refresh).
    pub last_error: Option<String>,
    /// A transient status message (e.g. the result of an operation).
    pub status: Option<String>,
    /// The active modal overlay, if any.
    pub modal: Option<Modal>,
    /// Ring-buffer history of resource samples (drives the sparkline).
    pub metrics: MetricsHistory,
    /// Distro names we've already attempted an in-distro disk sample for, so we
    /// fetch it at most once per distro (no per-poll `df`).
    pub inner_disk_attempted: HashSet<String>,
    /// The current UI language.
    pub lang: Lang,
    /// How navigation keys behave.
    pub keybind_style: KeybindStyle,
    /// What `Enter` launches on the list.
    pub default_shell_launch: ShellLaunch,
    /// False until the first refresh completes (drives a "loading" hint).
    pub loaded: bool,
    /// Root of the managed export/import folder (resolved from prefs at startup).
    pub manage_dir: PathBuf,
}

impl Model {
    /// The distributions matching the current filter (all, if no filter).
    pub fn visible_distros(&self) -> Vec<&Distro> {
        if self.filter.is_empty() {
            self.distros.iter().collect()
        } else {
            let needle = self.filter.to_ascii_lowercase();
            self.distros
                .iter()
                .filter(|distro| distro.name.to_ascii_lowercase().contains(&needle))
                .collect()
        }
    }

    /// The currently selected distribution within the visible list.
    pub fn selected_distro(&self) -> Option<&Distro> {
        self.visible_distros().get(self.selected).copied()
    }

    /// Move the selection down by one, clamped to the last visible row.
    pub(crate) fn select_next(&mut self) {
        let count = self.visible_distros().len();
        if count > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        }
    }

    /// Move the selection up by one, clamped to the first row.
    pub(crate) fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Keep the selection index within the bounds of the visible list.
    pub(crate) fn clamp_selection(&mut self) {
        let count = self.visible_distros().len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }
}
