//! Managed export/import folder: path, archive-format and name helpers (pure),
//! plus a thin filesystem layer (listing/deleting archives). Terminal-/clock-
//! independent so it can be unit-tested headlessly; the runtime performs the IO.

use std::path::{Path, PathBuf};

/// The export archive formats `wsl --export` can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Uncompressed tar (the `wsl` default; pass no `--format`).
    Tar,
    /// gzip-compressed tar (`--format tar.gz`).
    TarGz,
    /// xz-compressed tar (`--format tar.xz`).
    TarXz,
    /// Virtual hard disk (`--format vhd`).
    Vhd,
}

impl ExportFormat {
    /// Pick the format from a filename's extension (defaults to `Tar`).
    pub fn from_filename(name: &str) -> ExportFormat {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            ExportFormat::TarGz
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            ExportFormat::TarXz
        } else if lower.ends_with(".vhdx") || lower.ends_with(".vhd") {
            ExportFormat::Vhd
        } else {
            ExportFormat::Tar
        }
    }

    /// The `--format` value, or `None` for plain tar (omit the flag for
    /// back-compat with older `wsl`).
    pub fn wsl_format_arg(self) -> Option<&'static str> {
        match self {
            ExportFormat::Tar => None,
            ExportFormat::TarGz => Some("tar.gz"),
            ExportFormat::TarXz => Some("tar.xz"),
            ExportFormat::Vhd => Some("vhd"),
        }
    }
}

/// Whether a filename is a VHD archive (import needs `--vhd` for these).
pub fn is_vhd_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".vhdx") || lower.ends_with(".vhd")
}

/// Recognised archive extensions, longest-first so `.tar.gz` wins over `.tar`.
const ARCHIVE_EXTS: [&str; 7] = [
    ".tar.gz", ".tar.xz", ".tgz", ".txz", ".vhdx", ".tar", ".vhd",
];

/// Whether a filename has a recognised archive extension.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ARCHIVE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// The `exports\` subfolder of the managed root.
pub fn exports_dir(root: &Path) -> PathBuf {
    root.join("exports")
}

/// The `installed\<name>\` folder for an imported distro.
pub fn installed_dir(root: &Path, name: &str) -> PathBuf {
    root.join("installed").join(sanitize_name(name))
}

/// Replace characters illegal in Windows file names with `_`.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The default export filename: `<distro>-<local_ts>.tar`. `local_ts` is the
/// runtime-supplied local timestamp (`YYYYMMDD-HHMMSS`).
pub fn export_filename(distro: &str, local_ts: &str) -> String {
    format!("{}-{}.tar", sanitize_name(distro), local_ts)
}

/// A distro-name default derived from an archive filename (extension stripped).
pub fn derive_distro_name(archive_filename: &str) -> String {
    let lower = archive_filename.to_ascii_lowercase();
    for ext in ARCHIVE_EXTS {
        if lower.ends_with(ext) {
            return archive_filename[..archive_filename.len() - ext.len()].to_string();
        }
    }
    archive_filename.to_string()
}

use std::cmp::Reverse;
use std::io;

/// An export archive on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    /// File name (e.g. `Ubuntu-20260607-153012.tar.gz`).
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

/// List recognised archives in `<root>\exports`, newest-first. A missing folder
/// yields an empty list.
pub fn list_exports(root: &Path) -> io::Result<Vec<Archive>> {
    let dir = exports_dir(root);
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut rows: Vec<(Option<std::time::SystemTime>, Archive)> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_archive(name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        rows.push((
            meta.modified().ok(),
            Archive {
                name: name.to_string(),
                path: path.clone(),
                size: meta.len(),
            },
        ));
    }
    rows.sort_by_key(|r| Reverse(r.0)); // newest first
    Ok(rows.into_iter().map(|(_, a)| a).collect())
}

/// Delete an archive file.
pub fn delete_export(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Create `<root>\exports` and `<root>\installed` if missing.
pub fn ensure_dirs(root: &Path) -> io::Result<()> {
    std::fs::create_dir_all(exports_dir(root))?;
    std::fs::create_dir_all(root.join("installed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_format_from_extension() {
        assert_eq!(ExportFormat::from_filename("a.tar"), ExportFormat::Tar);
        assert_eq!(ExportFormat::from_filename("a.TAR.GZ"), ExportFormat::TarGz);
        assert_eq!(ExportFormat::from_filename("a.tgz"), ExportFormat::TarGz);
        assert_eq!(ExportFormat::from_filename("a.tar.xz"), ExportFormat::TarXz);
        assert_eq!(ExportFormat::from_filename("a.vhdx"), ExportFormat::Vhd);
        assert_eq!(ExportFormat::from_filename("a.bak"), ExportFormat::Tar);
    }

    #[test]
    fn export_format_wsl_arg() {
        assert_eq!(ExportFormat::Tar.wsl_format_arg(), None);
        assert_eq!(ExportFormat::TarGz.wsl_format_arg(), Some("tar.gz"));
        assert_eq!(ExportFormat::Vhd.wsl_format_arg(), Some("vhd"));
    }

    #[test]
    fn vhd_detection() {
        assert!(is_vhd_archive("x.vhdx"));
        assert!(is_vhd_archive("X.VHD"));
        assert!(!is_vhd_archive("x.tar.gz"));
    }

    #[test]
    fn paths_compose() {
        let root = Path::new(r"C:\wsl");
        assert_eq!(exports_dir(root), Path::new(r"C:\wsl\exports"));
        assert_eq!(
            installed_dir(root, "My/Distro"),
            Path::new(r"C:\wsl\installed\My_Distro")
        );
    }

    #[test]
    fn export_filename_uses_timestamp() {
        assert_eq!(
            export_filename("Ubuntu", "20260607-153012"),
            "Ubuntu-20260607-153012.tar"
        );
    }

    #[test]
    fn derive_name_strips_archive_extension() {
        assert_eq!(
            derive_distro_name("Ubuntu-20260607.tar.gz"),
            "Ubuntu-20260607"
        );
        assert_eq!(derive_distro_name("Debian.tar"), "Debian");
        assert_eq!(derive_distro_name("box.vhdx"), "box");
        assert_eq!(derive_distro_name("noext"), "noext");
    }

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_name("a:b/c"), "a_b_c");
    }

    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// A unique temp directory for an fs test (cleaned up by the test).
    fn temp_root() -> PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("wslm-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_exports_filters_and_sorts() {
        let root = temp_root();
        let exp = exports_dir(&root);
        std::fs::create_dir_all(&exp).unwrap();
        std::fs::write(exp.join("a.tar"), b"1").unwrap();
        std::fs::write(exp.join("b.tar.gz"), b"22").unwrap();
        std::fs::write(exp.join("notes.txt"), b"x").unwrap(); // ignored
        let names: Vec<String> = list_exports(&root)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a.tar".to_string()));
        assert!(names.contains(&"b.tar.gz".to_string()));
        assert!(!names.iter().any(|n| n == "notes.txt"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_exports_missing_dir_is_empty() {
        let root = temp_root();
        assert!(list_exports(&root).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ensure_dirs_then_delete() {
        let root = temp_root();
        ensure_dirs(&root).unwrap();
        assert!(exports_dir(&root).is_dir());
        assert!(root.join("installed").is_dir());
        let f = exports_dir(&root).join("x.tar");
        std::fs::write(&f, b"1").unwrap();
        delete_export(&f).unwrap();
        assert!(!f.exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
