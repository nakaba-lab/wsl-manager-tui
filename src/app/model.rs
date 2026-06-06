//! The application model (state). It grows with each milestone.

use std::collections::HashSet;

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
    /// A transient status message (e.g. the result of an operation). Auto-expires
    /// via [`Model::status_frames_left`] and is cleared by the next key press.
    pub status: Option<String>,
    /// Frames (~120 ms each) the current [`Model::status`] still shows before it
    /// auto-expires; `0` means no countdown is active. Set by [`Model::set_status`]
    /// and counted down once per `Event::Frame`.
    pub status_frames_left: u16,
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
}

impl Model {
    /// How long a transient status message stays visible: ~4 s at the runtime's
    /// 120 ms frame tick.
    pub const STATUS_TTL_FRAMES: u16 = 33;

    /// Show a transient status message. It auto-expires after
    /// [`Model::STATUS_TTL_FRAMES`] frames and is also dismissed by the next key
    /// press (see [`crate::app::update`]).
    pub fn set_status(&mut self, message: String) {
        self.status = Some(message);
        self.status_frames_left = Self::STATUS_TTL_FRAMES;
    }

    /// Clear the transient status message immediately.
    pub fn clear_status(&mut self) {
        self.status = None;
        self.status_frames_left = 0;
    }

    /// Count the status message one frame closer to expiry, clearing it once the
    /// budget runs out. Called once per `Event::Frame`.
    pub fn tick_status(&mut self) {
        if self.status_frames_left > 0 {
            self.status_frames_left -= 1;
            if self.status_frames_left == 0 {
                self.status = None;
            }
        }
    }

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
