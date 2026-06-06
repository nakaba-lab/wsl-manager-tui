//! Core message types for the MVU loop.
//!
//! [`Event`] is what the runtime feeds in (terminal input + timer ticks).
//! [`Action`] is what the [`crate::app::update`] reducer consumes. [`Command`]s
//! returned by `update` describe side effects the runtime then executes.

use std::path::PathBuf;

use super::input::KeyPress;
use crate::config::ConfigTarget;
use crate::i18n::{tf, Key, Lang};
use crate::manage::{Archive, ExportFormat};
use crate::metrics::MetricsSample;
use crate::wsl::{Distro, OnlineDistro};

/// Low-level input delivered by the runtime to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key press. The runtime filters out key repeat/release events (common
    /// on Windows) so a single press is never handled twice.
    Key(KeyPress),
    /// Terminal was resized to (width, height) columns/rows.
    Resize(u16, u16),
    /// Periodic timer tick. Drives list/metrics polling.
    Tick,
    /// Fast animation tick. Drives spinner animation (no polling).
    Frame,
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
    /// A resource sample was taken.
    MetricsSampled(MetricsSample),
    /// In-distro filesystem usage `(used, total)` bytes arrived for a distro.
    InnerDiskSampled {
        /// The distro the sample is for.
        name: String,
        /// The usage, or `None` if it could not be read.
        inner: Option<(u64, u64)>,
    },
    /// The list of installable distributions arrived.
    OnlineList(Vec<OnlineDistro>),
    /// The runtime computed the default export filename; open the export form.
    ExportDialogReady {
        /// The distro to export.
        distro: String,
        /// The default (editable) filename.
        filename: String,
    },
    /// The managed `exports\` listing arrived.
    ExportsListed(Vec<Archive>),
    /// A configuration file was loaded for editing.
    ConfigLoaded {
        /// Which file was loaded.
        target: ConfigTarget,
        /// Its current contents.
        content: String,
    },
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
    /// Sample resource usage (VM memory).
    SampleMetrics,
    /// Sample in-distro filesystem usage for a single (running) distro.
    SampleInnerDisk(String),
    /// Run a distribution lifecycle operation.
    Lifecycle(LifecycleOp),
    /// Suspend the TUI and run an interactive shell inline (`wsl -d <name>`),
    /// resuming when it exits.
    LaunchInlineShell(String),
    /// Open an interactive shell in a new Windows Terminal tab.
    LaunchTabShell(String),
    /// Fetch the list of installable distributions.
    ListOnline,
    /// Export a distro to an archive file.
    Export {
        /// The distro to export.
        name: String,
        /// Destination archive path (under the managed `exports\`).
        path: PathBuf,
        /// Archive format (derived from the filename extension).
        format: ExportFormat,
    },
    /// Import a distro from an archive file.
    Import {
        /// New distro name.
        name: String,
        /// Install directory (managed `installed\<name>\`).
        dir: PathBuf,
        /// Source archive path.
        tar: PathBuf,
        /// Whether the source is a `.vhd(x)` (adds `--vhd`).
        vhd: bool,
    },
    /// Build the timestamped default export filename and open the export dialog.
    OpenExportDialog {
        /// The distro to export.
        distro: String,
    },
    /// List archives in the managed `exports\` folder.
    ListExports,
    /// Delete a managed archive, then re-list.
    DeleteExport(PathBuf),
    /// Install a distro from the online catalog.
    Install {
        /// The install id.
        name: String,
    },
    /// Cancel the in-flight long-running operation.
    CancelOp,
    /// Load a configuration file for editing.
    LoadConfig(ConfigTarget),
    /// Save a configuration file.
    SaveConfig {
        /// Which file to save.
        target: ConfigTarget,
        /// The new contents.
        content: String,
    },
    /// Persist the current preferences (e.g. after a language change).
    SavePrefs,
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
    /// A localized success message shown after the operation completes.
    pub fn done_message(&self, lang: Lang) -> String {
        match self {
            LifecycleOp::Start(name) => tf(lang, Key::DoneStarted, &[name]),
            LifecycleOp::Terminate(name) => tf(lang, Key::DoneTerminated, &[name]),
            LifecycleOp::Shutdown => tf(lang, Key::DoneShutdown, &[]),
            LifecycleOp::SetDefault(name) => tf(lang, Key::DoneSetDefault, &[name]),
            LifecycleOp::Unregister(name) => tf(lang, Key::DoneUnregistered, &[name]),
        }
    }
}
