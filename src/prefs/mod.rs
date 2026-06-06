//! Preferences: persistence of user settings to
//! `%APPDATA%\wsl-manager-tui\config.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::{detect_default_lang, Lang};

/// How list navigation keys behave.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeybindStyle {
    /// Both arrow keys and vim keys (`j`/`k`).
    #[default]
    Both,
    /// Arrow keys only.
    ArrowsOnly,
    /// Vim keys only.
    VimOnly,
}

impl KeybindStyle {
    /// Whether `j`/`k` move the selection.
    pub fn vim_enabled(self) -> bool {
        matches!(self, KeybindStyle::Both | KeybindStyle::VimOnly)
    }

    /// Whether the arrow keys move the selection.
    pub fn arrows_enabled(self) -> bool {
        matches!(self, KeybindStyle::Both | KeybindStyle::ArrowsOnly)
    }
}

/// What `Enter` does on the distro list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellLaunch {
    /// Suspend the TUI and run the shell inline.
    #[default]
    Inline,
    /// Open the shell in a new Windows Terminal tab.
    NewTab,
}

impl ShellLaunch {
    /// The other launch mode (used for the Shift+Enter override).
    pub fn other(self) -> ShellLaunch {
        match self {
            ShellLaunch::Inline => ShellLaunch::NewTab,
            ShellLaunch::NewTab => ShellLaunch::Inline,
        }
    }
}

/// Persisted user preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// UI language. `None` means auto-detect from the environment.
    pub lang: Option<Lang>,
    /// Polling interval for list/metrics refresh, in seconds.
    pub poll_interval_secs: u64,
    /// Number of samples retained for the memory sparkline.
    pub history_len: usize,
    /// How navigation keys behave.
    pub keybind_style: KeybindStyle,
    /// What `Enter` launches on the list.
    pub default_shell_launch: ShellLaunch,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            lang: None,
            poll_interval_secs: 2,
            history_len: 60,
            keybind_style: KeybindStyle::Both,
            default_shell_launch: ShellLaunch::Inline,
        }
    }
}

impl Prefs {
    /// The effective language (saved preference, else auto-detected).
    pub fn effective_lang(&self) -> Lang {
        self.lang.unwrap_or_else(detect_default_lang)
    }

    /// The polling interval, clamped to at least one second.
    pub fn poll_interval(&self) -> u64 {
        self.poll_interval_secs.max(1)
    }
}

/// Path to the persisted config (`%APPDATA%\wsl-manager-tui\config.toml`).
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join("wsl-manager-tui").join("config.toml")
}

/// Load preferences, falling back to defaults on any error or missing file.
pub fn load() -> Prefs {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Persist preferences, creating the parent directory if needed.
pub fn save(prefs: &Prefs) -> std::io::Result<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text =
        toml::to_string_pretty(prefs).map_err(|error| std::io::Error::other(error.to_string()))?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let prefs = Prefs {
            lang: Some(Lang::Ja),
            poll_interval_secs: 5,
            history_len: 120,
            keybind_style: KeybindStyle::VimOnly,
            default_shell_launch: ShellLaunch::NewTab,
        };
        let text = toml::to_string_pretty(&prefs).unwrap();
        let parsed: Prefs = toml::from_str(&text).unwrap();
        assert_eq!(parsed, prefs);
    }

    #[test]
    fn defaults_fill_missing_fields() {
        // An empty document yields all defaults.
        let parsed: Prefs = toml::from_str("").unwrap();
        assert_eq!(parsed, Prefs::default());
        assert_eq!(parsed.poll_interval(), 2);
    }

    #[test]
    fn effective_lang_prefers_saved() {
        let prefs = Prefs {
            lang: Some(Lang::Ja),
            ..Default::default()
        };
        assert_eq!(prefs.effective_lang(), Lang::Ja);
    }
}
