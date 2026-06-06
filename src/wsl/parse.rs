//! Parsing of `wsl --list --verbose` output into raw rows.
//!
//! Deliberately locale-independent: the (localized) STATE column is ignored
//! entirely, and the header is skipped by requiring the last token to be a
//! numeric version. Running state is determined separately from
//! `wsl --list --running` (see [`crate::wsl::collect`]).

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
}
