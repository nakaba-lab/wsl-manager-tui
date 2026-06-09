//! Error types for the library layer (built on `thiserror`). The application
//! layer surfaces these through `color_eyre` reports.

/// Errors from the WSL backend and supporting layers.
#[derive(Debug, thiserror::Error)]
pub enum WslError {
    /// Spawning or waiting on `wsl.exe` failed at the OS level.
    #[error("failed to run wsl.exe: {0}")]
    Spawn(#[from] std::io::Error),

    /// `wsl.exe` ran but returned a non-zero exit status.
    #[error("wsl.exe {args:?} failed: {message}")]
    Command {
        /// The arguments passed to `wsl.exe`.
        args: Vec<String>,
        /// The decoded stderr (or a fallback message).
        message: String,
    },

    /// Reading the WSL registry hive failed.
    #[error("registry read failed: {0}")]
    Registry(String),
}

/// Convenience result alias for the library layer.
pub type Result<T> = std::result::Result<T, WslError>;
