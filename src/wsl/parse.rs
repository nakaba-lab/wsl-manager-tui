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
}
