//! The `wsl.exe` backend: a trait (so the app can be driven by a mock in tests)
//! and the real implementation that shells out to `wsl.exe`.

use async_trait::async_trait;

use crate::error::{Result, WslError};
use crate::wsl::decode::decode_wsl_output;
use crate::wsl::parse::{parse_list_verbose, RawDistroRow};

/// Abstraction over the `wsl.exe` CLI so the app can be driven by a mock in
/// tests. Transfer/install operations are added in later milestones.
#[async_trait]
pub trait WslBackend: Send + Sync {
    /// `wsl -l -v`: every registered distro with name, version and default flag.
    async fn list_verbose(&self) -> Result<Vec<RawDistroRow>>;
    /// `wsl --list --running -q`: names of currently running distros.
    async fn list_running(&self) -> Result<Vec<String>>;
    /// Boot a distro (`wsl -d <name> -- true`).
    async fn start(&self, name: &str) -> Result<()>;
    /// Stop a single distro (`wsl --terminate <name>`).
    async fn terminate(&self, name: &str) -> Result<()>;
    /// Stop the whole WSL VM (`wsl --shutdown`).
    async fn shutdown(&self) -> Result<()>;
    /// Set the default distro (`wsl --set-default <name>`).
    async fn set_default(&self, name: &str) -> Result<()>;
    /// Unregister (permanently delete) a distro (`wsl --unregister <name>`).
    async fn unregister(&self, name: &str) -> Result<()>;
}

/// The real backend that shells out to `wsl.exe`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealWslBackend;

#[async_trait]
impl WslBackend for RealWslBackend {
    async fn list_verbose(&self) -> Result<Vec<RawDistroRow>> {
        let text = run_wsl(&["--list", "--verbose"]).await?;
        Ok(parse_list_verbose(&text))
    }

    async fn list_running(&self) -> Result<Vec<String>> {
        // When no distro is running, some wsl builds return a non-zero status
        // with a localized message. Treat any failure here as "none running".
        let Ok(text) = run_wsl(&["--list", "--running", "--quiet"]).await else {
            return Ok(Vec::new());
        };
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    async fn start(&self, name: &str) -> Result<()> {
        // Running any trivial command boots the distro.
        run_wsl(&["-d", name, "--", "true"]).await.map(drop)
    }

    async fn terminate(&self, name: &str) -> Result<()> {
        run_wsl(&["--terminate", name]).await.map(drop)
    }

    async fn shutdown(&self) -> Result<()> {
        run_wsl(&["--shutdown"]).await.map(drop)
    }

    async fn set_default(&self, name: &str) -> Result<()> {
        run_wsl(&["--set-default", name]).await.map(drop)
    }

    async fn unregister(&self, name: &str) -> Result<()> {
        run_wsl(&["--unregister", name]).await.map(drop)
    }
}

/// Run `wsl.exe` with the given arguments and return its decoded stdout. On a
/// non-zero exit, the decoded stderr is carried in the error for display.
async fn run_wsl(args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("wsl.exe")
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(decode_wsl_output(&output.stdout))
    } else {
        Err(WslError::Command {
            args: args.iter().map(|s| (*s).to_string()).collect(),
            message: decode_wsl_output(&output.stderr).trim().to_string(),
        })
    }
}
