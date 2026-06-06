//! Merge parsed `wsl -l -v` rows, the running-distro set, and registry metadata
//! into [`Distro`] records, then fill disk sizes.
//!
//! [`collect_distros`] is pure (no IO) and is the main test surface for this
//! layer; [`refresh`] adds the IO (backend calls, registry, filesystem).

use std::collections::HashSet;

use crate::error::Result;
use crate::registry::{self, LxssEntry};
use crate::wsl::backend::WslBackend;
use crate::wsl::model::{Distro, DistroState};
use crate::wsl::parse::RawDistroRow;

/// Merge parsed rows with the running set and registry entries. Pure: disk
/// sizes are left as `None` and filled by [`refresh`].
pub fn collect_distros(
    rows: Vec<RawDistroRow>,
    running: &HashSet<String>,
    lxss: &[LxssEntry],
) -> Vec<Distro> {
    rows.into_iter()
        .map(|row| {
            let entry = lxss.iter().find(|e| e.name == row.name);
            let state = if running.contains(&row.name) {
                DistroState::Running
            } else {
                DistroState::Stopped
            };
            Distro {
                name: row.name,
                state,
                version: row.version,
                is_default: row.is_default,
                guid: entry.map(|e| e.guid.clone()),
                base_path: entry.map(|e| e.base_path.clone()),
                vhd_path: entry.and_then(registry::vhd_path_of),
                disk_bytes: None,
            }
        })
        .collect()
}

/// Fetch the full distro list from the backend, merge with registry data, and
/// fill in vhdx disk sizes from the filesystem. Registry failures are tolerated
/// (the distros simply lack paths/sizes).
pub async fn refresh(backend: &dyn WslBackend) -> Result<Vec<Distro>> {
    let rows = backend.list_verbose().await?;
    let running: HashSet<String> = backend.list_running().await?.into_iter().collect();
    let lxss = registry::read_lxss().unwrap_or_default();

    let mut distros = collect_distros(rows, &running, &lxss);
    for distro in &mut distros {
        if let Some(path) = &distro.vhd_path {
            distro.disk_bytes = std::fs::metadata(path).ok().map(|meta| meta.len());
        }
    }
    Ok(distros)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(name: &str) -> LxssEntry {
        LxssEntry {
            guid: format!("{{guid-{name}}}"),
            name: name.to_string(),
            base_path: PathBuf::from(format!("C:\\wsl\\{name}")),
            version: 2,
            state: 1,
            default_uid: 1000,
            flags: 15,
            vhd_file: Some("ext4.vhdx".to_string()),
            flavor: None,
        }
    }

    fn row(name: &str, is_default: bool) -> RawDistroRow {
        RawDistroRow {
            name: name.to_string(),
            version: 2,
            is_default,
        }
    }

    #[test]
    fn merges_running_state_locale_independent() {
        let rows = vec![row("Debian", true), row("Ubuntu", false)];
        let running: HashSet<String> = ["Debian".to_string()].into_iter().collect();
        let lxss = vec![entry("Debian"), entry("Ubuntu")];

        let distros = collect_distros(rows, &running, &lxss);
        assert_eq!(distros[0].state, DistroState::Running);
        assert_eq!(distros[1].state, DistroState::Stopped);
        assert_eq!(distros[0].guid.as_deref(), Some("{guid-Debian}"));
        assert_eq!(
            distros[0].vhd_path,
            Some(PathBuf::from("C:\\wsl\\Debian\\ext4.vhdx"))
        );
        assert!(distros[0].is_default);
    }

    #[test]
    fn distro_without_registry_entry_has_no_paths() {
        let distros = collect_distros(vec![row("Orphan", false)], &HashSet::new(), &[]);
        assert_eq!(distros[0].state, DistroState::Stopped);
        assert!(distros[0].vhd_path.is_none());
        assert!(distros[0].base_path.is_none());
    }
}
