//! Config: order/comment/unknown-key-preserving INI handling for `.wslconfig`
//! and `wsl.conf`, the known-key schemas, and path resolution.
//!
//! `.wslconfig` lives on the Windows side (`%USERPROFILE%\.wslconfig`) and is
//! read/written via the filesystem here. `wsl.conf` lives inside a distro
//! (`/etc/wsl.conf`) and is read/written by the backend as root.

pub mod ini;

pub use ini::IniDoc;

use std::path::PathBuf;

/// Which configuration file is being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigTarget {
    /// The global Windows `%USERPROFILE%\.wslconfig`.
    WslConfig,
    /// A distro's `/etc/wsl.conf`.
    WslConf(String),
}

impl ConfigTarget {
    /// A short human label for titles.
    pub fn label(&self) -> String {
        match self {
            ConfigTarget::WslConfig => ".wslconfig (global)".to_string(),
            ConfigTarget::WslConf(distro) => format!("wsl.conf ({distro})"),
        }
    }
}

/// A known configuration key, used to build form fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKey {
    /// INI section name.
    pub section: &'static str,
    /// Key name.
    pub key: &'static str,
    /// Short hint about accepted values.
    pub hint: &'static str,
}

/// Known keys for `.wslconfig` (`[wsl2]`).
const WSLCONFIG_KEYS: &[ConfigKey] = &[
    ConfigKey {
        section: "wsl2",
        key: "memory",
        hint: "e.g. 8GB",
    },
    ConfigKey {
        section: "wsl2",
        key: "processors",
        hint: "integer",
    },
    ConfigKey {
        section: "wsl2",
        key: "swap",
        hint: "e.g. 0 or 8GB",
    },
    ConfigKey {
        section: "wsl2",
        key: "swapFile",
        hint: "path",
    },
    ConfigKey {
        section: "wsl2",
        key: "localhostForwarding",
        hint: "true/false",
    },
    ConfigKey {
        section: "wsl2",
        key: "networkingMode",
        hint: "NAT/mirrored",
    },
    ConfigKey {
        section: "wsl2",
        key: "nestedVirtualization",
        hint: "true/false",
    },
];

/// Known keys for `wsl.conf`.
const WSLCONF_KEYS: &[ConfigKey] = &[
    ConfigKey {
        section: "boot",
        key: "systemd",
        hint: "true/false",
    },
    ConfigKey {
        section: "boot",
        key: "command",
        hint: "shell command",
    },
    ConfigKey {
        section: "automount",
        key: "enabled",
        hint: "true/false",
    },
    ConfigKey {
        section: "automount",
        key: "root",
        hint: "e.g. /mnt/",
    },
    ConfigKey {
        section: "automount",
        key: "options",
        hint: "mount options",
    },
    ConfigKey {
        section: "network",
        key: "generateHosts",
        hint: "true/false",
    },
    ConfigKey {
        section: "network",
        key: "generateResolvConf",
        hint: "true/false",
    },
    ConfigKey {
        section: "network",
        key: "hostname",
        hint: "hostname",
    },
    ConfigKey {
        section: "interop",
        key: "enabled",
        hint: "true/false",
    },
    ConfigKey {
        section: "interop",
        key: "appendWindowsPath",
        hint: "true/false",
    },
    ConfigKey {
        section: "user",
        key: "default",
        hint: "username",
    },
];

/// The known-key schema for a target.
pub fn schema(target: &ConfigTarget) -> &'static [ConfigKey] {
    match target {
        ConfigTarget::WslConfig => WSLCONFIG_KEYS,
        ConfigTarget::WslConf(_) => WSLCONF_KEYS,
    }
}

/// Path to the global `.wslconfig`.
pub fn wslconfig_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".wslconfig")
}

/// Read `.wslconfig`, returning an empty string if it does not exist.
pub fn load_wslconfig() -> std::io::Result<String> {
    match std::fs::read_to_string(wslconfig_path()) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

/// Write `.wslconfig`, backing up any existing file to `.wslconfig.bak` first.
pub fn save_wslconfig(content: &str) -> std::io::Result<()> {
    let path = wslconfig_path();
    if path.exists() {
        let backup = path.with_file_name(".wslconfig.bak");
        let _ = std::fs::copy(&path, backup);
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_selects_by_target() {
        assert!(schema(&ConfigTarget::WslConfig)
            .iter()
            .any(|k| k.key == "memory"));
        assert!(schema(&ConfigTarget::WslConf("Debian".into()))
            .iter()
            .any(|k| k.section == "boot" && k.key == "systemd"));
    }

    #[test]
    fn target_labels() {
        assert_eq!(ConfigTarget::WslConfig.label(), ".wslconfig (global)");
        assert_eq!(
            ConfigTarget::WslConf("Debian".into()).label(),
            "wsl.conf (Debian)"
        );
    }
}
