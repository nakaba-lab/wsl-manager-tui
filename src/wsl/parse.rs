//! Parsing of `wsl --list` output into raw rows.
//!
//! Deliberately locale-independent: the (localized) STATE column is ignored
//! entirely, and the header is skipped by requiring the last token to be a
//! numeric version. Running state is determined separately from
//! `wsl --list --running` (see [`crate::wsl::collect`]).

use crate::wsl::model::OnlineDistro;

/// One row of `wsl -l -v`, before merging with registry/running data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDistroRow {
    pub name: String,
    pub version: u8,
    pub is_default: bool,
}

/// Parse decoded `wsl -l -v` text. Lines whose last token is not a number
/// (the header and blank lines) are skipped.
pub fn parse_list_verbose(text: &str) -> Vec<RawDistroRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let is_default = trimmed.starts_with('*');
        let rest = trimmed.strip_prefix('*').unwrap_or(trimmed);
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        let Some(version) = tokens.last().and_then(|t| t.parse::<u8>().ok()) else {
            continue;
        };
        rows.push(RawDistroRow {
            name: tokens[0].to_string(),
            version,
            is_default,
        });
    }
    rows
}

/// Parse decoded `wsl --list --online` text into installable distributions.
///
/// The output has prose intro lines, a header, then `NAME  FRIENDLY NAME` rows.
/// We keep only lines that split into two columns (on a run of 2+ spaces) whose
/// first column is an ASCII distro id, which skips prose and the (localized)
/// header without depending on its text.
pub fn parse_list_online(text: &str) -> Vec<OnlineDistro> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start().trim_start_matches('*').trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, friendly)) = split_two_columns(line) else {
            continue;
        };
        if !is_distro_id(name) || name.eq_ignore_ascii_case("NAME") {
            continue;
        }
        out.push(OnlineDistro {
            name: name.to_string(),
            friendly: friendly.to_string(),
        });
    }
    out
}

/// Split a line into `(first, rest)` at the first run of two or more spaces.
fn split_two_columns(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b' ' && bytes[i + 1] == b' ' {
            let name = line[..i].trim_end();
            let friendly = line[i..].trim_start();
            if name.is_empty() || friendly.is_empty() {
                return None;
            }
            return Some((name, friendly));
        }
    }
    None
}

/// Whether a string is a plausible ASCII distro install id.
fn is_distro_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Parse `df -kP /` output into `(used_bytes, total_bytes)`. Columns are
/// `Filesystem 1024-blocks Used Available Capacity Mounted-on`; blocks are 1 KiB.
pub fn parse_df(text: &str) -> Option<(u64, u64)> {
    for line in text.lines().skip(1) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() >= 4 {
            if let (Ok(total), Ok(used)) = (tokens[1].parse::<u64>(), tokens[2].parse::<u64>()) {
                return Some((used * 1024, total * 1024));
            }
        }
    }
    None
}

/// Parse `/proc/meminfo` for the WSL VM's total RAM, in bytes. The `MemTotal:`
/// value is in kibibytes (Linux labels it `kB` but means KiB). This equals the
/// memory assigned to the WSL 2 VM (`.wslconfig` `[wsl2] memory`, or the default
/// of 50% of host RAM), as the kernel sees it.
pub fn parse_meminfo_total(text: &str) -> Option<u64> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib * 1024);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = "  NAME            STATE           VERSION\r\n\
* Debian          Running         2\r\n\
  Ubuntu-24.04    Stopped         2\r\n";

    const JA: &str = "  名前            状態            バージョン\r\n\
* Debian          実行中          2\r\n\
  Ubuntu-24.04    停止            2\r\n";

    #[test]
    fn parses_english() {
        let rows = parse_list_verbose(EN);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Debian");
        assert_eq!(rows[0].version, 2);
        assert!(rows[0].is_default);
        assert_eq!(rows[1].name, "Ubuntu-24.04");
        assert!(!rows[1].is_default);
    }

    #[test]
    fn parses_japanese_identically() {
        // Localized header and state must not change the result.
        let rows = parse_list_verbose(JA);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Debian");
        assert!(rows[0].is_default);
        assert_eq!(rows[1].name, "Ubuntu-24.04");
        assert!(!rows[1].is_default);
    }

    #[test]
    fn skips_header_and_blanks() {
        let rows = parse_list_verbose("  NAME  STATE  VERSION\r\n\r\n");
        assert!(rows.is_empty());
    }

    #[test]
    fn handles_empty_input() {
        assert!(parse_list_verbose("").is_empty());
    }

    const ONLINE: &str =
        "The following is a list of valid distributions that can be installed.\r\n\
Install using 'wsl.exe --install <Distro>'.\r\n\
\r\n\
NAME                   FRIENDLY NAME\r\n\
Ubuntu                 Ubuntu\r\n\
Debian                 Debian GNU/Linux\r\n\
kali-linux             Kali Linux Rolling\r\n\
Ubuntu-24.04           Ubuntu 24.04 LTS\r\n";

    #[test]
    fn parses_online_list_skipping_prose_and_header() {
        let items = parse_list_online(ONLINE);
        let names: Vec<&str> = items.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Ubuntu", "Debian", "kali-linux", "Ubuntu-24.04"]);
        assert_eq!(items[1].friendly, "Debian GNU/Linux");
    }

    #[test]
    fn online_list_handles_empty() {
        assert!(parse_list_online("").is_empty());
    }

    // Localized (Japanese) prose intro + English column header, as emitted by
    // `wsl --list --online` on a JA locale. The prose and header must be skipped
    // and the ASCII distro rows kept.
    const ONLINE_JA_PROSE: &str =
        "インストールできる有効なディストリビューションの一覧を次に示します。\r\n\
'wsl.exe --install <Distro>' を使用してインストールします。\r\n\
\r\n\
NAME                   FRIENDLY NAME\r\n\
Ubuntu                 Ubuntu\r\n\
Debian                 Debian GNU/Linux\r\n";

    #[test]
    fn parses_online_list_with_localized_prose() {
        let items = parse_list_online(ONLINE_JA_PROSE);
        let names: Vec<&str> = items.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Ubuntu", "Debian"]);
    }

    #[test]
    fn parses_df_output() {
        let df = "Filesystem     1024-blocks    Used Available Capacity Mounted on\n\
/dev/sdc            1000000  250000    750000      25% /\n";
        // 1000000 KiB total, 250000 KiB used.
        assert_eq!(parse_df(df), Some((250000 * 1024, 1000000 * 1024)));
    }

    #[test]
    fn parse_df_handles_garbage() {
        assert_eq!(parse_df(""), None);
        assert_eq!(parse_df("not a df output\n"), None);
    }

    #[test]
    fn parses_meminfo_total() {
        let meminfo = "MemTotal:       16384000 kB\n\
MemFree:         8000000 kB\n\
MemAvailable:   12000000 kB\n";
        // 16384000 KiB → bytes.
        assert_eq!(parse_meminfo_total(meminfo), Some(16_384_000 * 1024));
    }

    #[test]
    fn parse_meminfo_total_handles_garbage() {
        assert_eq!(parse_meminfo_total(""), None);
        assert_eq!(parse_meminfo_total("MemFree: 100 kB\n"), None);
        assert_eq!(parse_meminfo_total("MemTotal: notanumber kB\n"), None);
    }
}
