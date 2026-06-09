//! Registry: read-only access to
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss` to obtain each
//! distribution's GUID, base path, version, default UID, flags and vhdx file.

use std::path::PathBuf;

use crate::error::{Result, WslError};

const LXSS_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";
const DEFAULT_VHD: &str = "ext4.vhdx";

/// A distribution entry read from the `Lxss` registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LxssEntry {
    pub guid: String,
    pub name: String,
    pub base_path: PathBuf,
    pub version: u8,
    pub state: u32,
    pub default_uid: u32,
    pub flags: u32,
    pub vhd_file: Option<String>,
    pub flavor: Option<String>,
}

/// Read all distribution entries from `HKCU\...\Lxss`. Subkeys missing the
/// required `DistributionName` value are skipped.
pub fn read_lxss() -> Result<Vec<LxssEntry>> {
    let lxss = windows_registry::CURRENT_USER
        .open(LXSS_PATH)
        .map_err(|e| WslError::Registry(e.to_string()))?;
    let keys = lxss.keys().map_err(|e| WslError::Registry(e.to_string()))?;

    let mut entries = Vec::new();
    for guid in keys {
        let Ok(sub) = lxss.open(&guid) else {
            continue;
        };
        let Ok(name) = sub.get_string("DistributionName") else {
            continue;
        };
        let base_path = sub
            .get_string("BasePath")
            .map(strip_unc_prefix)
            .unwrap_or_default();
        entries.push(LxssEntry {
            guid,
            name,
            base_path: PathBuf::from(base_path),
            version: sub.get_u32("Version").unwrap_or(0) as u8,
            state: sub.get_u32("State").unwrap_or(0),
            default_uid: sub.get_u32("DefaultUid").unwrap_or(0),
            flags: sub.get_u32("Flags").unwrap_or(0),
            vhd_file: sub.get_string("VhdFileName").ok(),
            flavor: sub.get_string("Flavor").ok(),
        });
    }
    Ok(entries)
}

/// The GUID of the default distribution, if set.
pub fn read_default_guid() -> Option<String> {
    windows_registry::CURRENT_USER
        .open(LXSS_PATH)
        .ok()?
        .get_string("DefaultDistribution")
        .ok()
}

/// Full path to a distribution's virtual disk, derived from its base path and
/// vhdx file name (defaulting to `ext4.vhdx`).
pub fn vhd_path_of(entry: &LxssEntry) -> Option<PathBuf> {
    if entry.base_path.as_os_str().is_empty() {
        return None;
    }
    let file = entry.vhd_file.as_deref().unwrap_or(DEFAULT_VHD);
    Some(entry.base_path.join(file))
}

/// BasePath is sometimes stored with the `\\?\` long-path prefix; strip it for
/// display and filesystem use.
fn strip_unc_prefix(path: String) -> String {
    path.strip_prefix(r"\\?\").map(String::from).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vhd_path_joins_base_and_file() {
        let entry = LxssEntry {
            guid: "{g}".into(),
            name: "Debian".into(),
            base_path: PathBuf::from("C:\\wsl\\Debian"),
            version: 2,
            state: 1,
            default_uid: 1000,
            flags: 15,
            vhd_file: Some("ext4.vhdx".into()),
            flavor: None,
        };
        assert_eq!(
            vhd_path_of(&entry),
            Some(PathBuf::from("C:\\wsl\\Debian\\ext4.vhdx"))
        );
    }

    #[test]
    fn vhd_path_none_without_base() {
        let entry = LxssEntry {
            guid: "{g}".into(),
            name: "X".into(),
            base_path: PathBuf::new(),
            version: 2,
            state: 0,
            default_uid: 0,
            flags: 0,
            vhd_file: None,
            flavor: None,
        };
        assert_eq!(vhd_path_of(&entry), None);
    }

    #[test]
    fn strips_long_path_prefix() {
        assert_eq!(strip_unc_prefix(r"\\?\C:\wsl".into()), r"C:\wsl");
        assert_eq!(strip_unc_prefix(r"C:\wsl".into()), r"C:\wsl");
    }

    #[test]
    #[ignore = "reads the real machine registry"]
    fn reads_real_lxss() {
        let entries = read_lxss().unwrap();
        assert!(!entries.is_empty());
        for e in &entries {
            assert!(!e.name.is_empty());
        }
    }
}
