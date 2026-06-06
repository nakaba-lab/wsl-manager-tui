//! The `wsl.exe` backend: a trait (so the app can be driven by a mock in tests)
//! and the real implementation that shells out to `wsl.exe`.

use std::path::Path;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::error::{Result, WslError};
use crate::manage::ExportFormat;
use crate::wsl::decode::{decode_utf8, decode_wsl_output};
use crate::wsl::model::OnlineDistro;
use crate::wsl::parse::{parse_df, parse_list_online, parse_list_verbose, RawDistroRow};

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
    /// List installable distributions (`wsl --list --online`).
    async fn list_online(&self) -> Result<Vec<OnlineDistro>>;
    /// Export a distro (`wsl --export <name> <path> [--format <fmt>]`).
    async fn export(&self, name: &str, path: &Path, format: ExportFormat) -> Result<()>;
    /// Import a distro (`wsl --import <name> <dir> <tar> [--vhd]`).
    async fn import(&self, name: &str, dir: &Path, tar: &Path, vhd: bool) -> Result<()>;
    /// Install a distro (`wsl --install -d <name> --no-launch`).
    async fn install(&self, name: &str) -> Result<()>;
    /// In-distro root filesystem usage as `(used, total)` bytes (`df -kP /`).
    async fn inner_disk(&self, distro: &str) -> Result<Option<(u64, u64)>>;
    /// Read a distro's `/etc/wsl.conf` (as root); empty if it does not exist.
    async fn read_conf(&self, distro: &str) -> Result<String>;
    /// Write a distro's `/etc/wsl.conf` (as root), backing up the old file.
    async fn write_conf(&self, distro: &str, content: &str) -> Result<()>;
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

    async fn list_online(&self) -> Result<Vec<OnlineDistro>> {
        let text = run_wsl(&["--list", "--online"]).await?;
        Ok(parse_list_online(&text))
    }

    async fn export(&self, name: &str, path: &Path, format: ExportFormat) -> Result<()> {
        let path = path.to_string_lossy();
        run_wsl_long(&export_args(name, path.as_ref(), format))
            .await
            .map(drop)
    }

    async fn import(&self, name: &str, dir: &Path, tar: &Path, vhd: bool) -> Result<()> {
        let dir = dir.to_string_lossy();
        let tar = tar.to_string_lossy();
        run_wsl_long(&import_args(name, dir.as_ref(), tar.as_ref(), vhd))
            .await
            .map(drop)
    }

    async fn install(&self, name: &str) -> Result<()> {
        run_wsl_long(&["--install", "-d", name, "--no-launch"])
            .await
            .map(drop)
    }

    async fn inner_disk(&self, distro: &str) -> Result<Option<(u64, u64)>> {
        // In-distro output is UTF-8. `df -kP /` is POSIX and parseable.
        let output = tokio::process::Command::new("wsl.exe")
            .args(["-d", distro, "--", "df", "-kP", "/"])
            .output()
            .await?;
        if output.status.success() {
            Ok(parse_df(&decode_utf8(&output.stdout)))
        } else {
            Ok(None)
        }
    }

    async fn read_conf(&self, distro: &str) -> Result<String> {
        // In-distro output is UTF-8. A missing file is treated as empty.
        let output = tokio::process::Command::new("wsl.exe")
            .args(["-d", distro, "-u", "root", "--", "cat", "/etc/wsl.conf"])
            .output()
            .await?;
        if output.status.success() {
            Ok(decode_utf8(&output.stdout))
        } else {
            Ok(String::new())
        }
    }

    async fn write_conf(&self, distro: &str, content: &str) -> Result<()> {
        // Back up an existing file first (best effort).
        let _ = tokio::process::Command::new("wsl.exe")
            .args([
                "-d",
                distro,
                "-u",
                "root",
                "--",
                "sh",
                "-c",
                "test -f /etc/wsl.conf && cp /etc/wsl.conf /etc/wsl.conf.bak || true",
            ])
            .output()
            .await?;

        // Write the new content via `tee` (root) over stdin.
        let mut child = tokio::process::Command::new("wsl.exe")
            .args(["-d", distro, "-u", "root", "--", "tee", "/etc/wsl.conf"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content.as_bytes()).await?;
            stdin.shutdown().await?;
        }
        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(WslError::Command {
                args: vec![
                    "--".to_string(),
                    "tee".to_string(),
                    "/etc/wsl.conf".to_string(),
                ],
                message: "failed to write /etc/wsl.conf (need root?)".to_string(),
            })
        }
    }
}

fn export_args<'a>(name: &'a str, path: &'a str, format: ExportFormat) -> Vec<&'a str> {
    let mut args = vec!["--export", name, path];
    if let Some(fmt) = format.wsl_format_arg() {
        args.push("--format");
        args.push(fmt);
    }
    args
}

fn import_args<'a>(name: &'a str, dir: &'a str, tar: &'a str, vhd: bool) -> Vec<&'a str> {
    let mut args = vec!["--import", name, dir, tar];
    if vhd {
        args.push("--vhd");
    }
    args
}

/// Run `wsl.exe` with the given arguments and return its decoded stdout. On a
/// non-zero exit, the decoded stderr is carried in the error for display.
async fn run_wsl(args: &[&str]) -> Result<String> {
    run_wsl_inner(args, false).await
}

/// Like [`run_wsl`] but kills the child if the future is dropped (e.g. the
/// operation is cancelled). Used for long-running export/import/install.
async fn run_wsl_long(args: &[&str]) -> Result<String> {
    run_wsl_inner(args, true).await
}

async fn run_wsl_inner(args: &[&str], kill_on_drop: bool) -> Result<String> {
    let output = tokio::process::Command::new("wsl.exe")
        .args(args)
        .kill_on_drop(kill_on_drop)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_args_tar_omits_format() {
        assert_eq!(
            export_args("Debian", "p.tar", ExportFormat::Tar),
            ["--export", "Debian", "p.tar"]
        );
    }

    #[test]
    fn export_args_targz_adds_format() {
        assert_eq!(
            export_args("Debian", "p.tar.gz", ExportFormat::TarGz),
            ["--export", "Debian", "p.tar.gz", "--format", "tar.gz"]
        );
    }

    #[test]
    fn export_args_vhd_uses_format_vhd() {
        assert_eq!(
            export_args("Debian", "p.vhdx", ExportFormat::Vhd),
            ["--export", "Debian", "p.vhdx", "--format", "vhd"]
        );
    }

    #[test]
    fn import_args_tar_has_no_flag() {
        assert_eq!(
            import_args("D", "dir", "t.tar.gz", false),
            ["--import", "D", "dir", "t.tar.gz"]
        );
    }

    #[test]
    fn import_args_vhd_adds_flag() {
        assert_eq!(
            import_args("D", "dir", "t.vhdx", true),
            ["--import", "D", "dir", "t.vhdx", "--vhd"]
        );
    }
}
