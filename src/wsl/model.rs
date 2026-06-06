//! Core WSL data types (UI-independent).

use std::path::PathBuf;

/// Running state of a distribution. Determined without parsing localized status
/// strings — see [`crate::wsl::collect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroState {
    Running,
    Stopped,
    Installing,
    Unknown,
}

impl DistroState {
    /// A compact glyph for the state column.
    pub fn glyph(self) -> char {
        match self {
            DistroState::Running => '●',
            DistroState::Stopped => '○',
            DistroState::Installing => '◐',
            DistroState::Unknown => '?',
        }
    }
}

/// A distribution available to install, from `wsl --list --online`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineDistro {
    /// The install identifier (passed to `wsl --install -d`).
    pub name: String,
    /// The human-friendly name.
    pub friendly: String,
}

/// A registered WSL distribution with metadata merged from `wsl.exe`, the
/// running-distro set, and the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    /// Distribution name (e.g. `Debian`).
    pub name: String,
    /// Running/stopped state.
    pub state: DistroState,
    /// WSL major version (1 or 2).
    pub version: u8,
    /// Whether this is the default distribution.
    pub is_default: bool,
    /// Registry GUID, if found.
    pub guid: Option<String>,
    /// Install base path from the registry.
    pub base_path: Option<PathBuf>,
    /// Full path to the backing `ext4.vhdx`, if known.
    pub vhd_path: Option<PathBuf>,
    /// Size of the vhdx on disk, in bytes.
    pub disk_bytes: Option<u64>,
}
