//! A minimal INI parser/writer that preserves comments, blank lines, unknown
//! keys, and line/section order across a parse → render round trip. Used for
//! `.wslconfig` and `wsl.conf`.
//!
//! Only `key=value` entries are normalized (surrounding spaces are dropped);
//! everything else is preserved verbatim.

/// One line of an INI document.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    /// A blank or comment line, preserved verbatim.
    Other(String),
    /// A `[section]` header (stores the section name).
    Section(String),
    /// A `key=value` entry.
    Entry { key: String, value: String },
}

/// A parsed INI document that round-trips non-semantic content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IniDoc {
    lines: Vec<Line>,
}

impl IniDoc {
    /// Parse INI text. Unrecognized lines are preserved verbatim.
    pub fn parse(text: &str) -> Self {
        let mut lines = Vec::new();
        for raw in text.lines() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                lines.push(Line::Other(raw.to_string()));
            } else if let Some(section) = trimmed
                .strip_prefix('[')
                .and_then(|inner| inner.strip_suffix(']'))
            {
                lines.push(Line::Section(section.trim().to_string()));
            } else if let Some((key, value)) = trimmed.split_once('=') {
                lines.push(Line::Entry {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                });
            } else {
                lines.push(Line::Other(raw.to_string()));
            }
        }
        Self { lines }
    }

    /// Render back to INI text (always newline-terminated per line).
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Other(text) => out.push_str(text),
                Line::Section(name) => {
                    out.push('[');
                    out.push_str(name);
                    out.push(']');
                }
                Line::Entry { key, value } => {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(value);
                }
            }
            out.push('\n');
        }
        out
    }

    /// The value of `key` within `section` (case-insensitive key match).
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.find_entry(section, key)
            .and_then(|i| match &self.lines[i] {
                Line::Entry { value, .. } => Some(value.as_str()),
                _ => None,
            })
    }

    /// Set `key=value` within `section`, creating the section/entry if needed.
    /// An empty (trimmed) value removes the key, so cleared form fields do not
    /// write blank entries.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let value = value.trim();
        if let Some(index) = self.find_entry(section, key) {
            if value.is_empty() {
                self.lines.remove(index);
            } else if let Line::Entry {
                value: existing, ..
            } = &mut self.lines[index]
            {
                *existing = value.to_string();
            }
            return;
        }
        if value.is_empty() {
            return;
        }
        match self.section_insert_index(section) {
            Some(index) => self.lines.insert(
                index,
                Line::Entry {
                    key: key.to_string(),
                    value: value.to_string(),
                },
            ),
            None => {
                self.lines.push(Line::Section(section.to_string()));
                self.lines.push(Line::Entry {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }

    fn find_entry(&self, section: &str, key: &str) -> Option<usize> {
        let mut current: Option<&str> = None;
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::Section(name) => current = Some(name.as_str()),
                Line::Entry { key: k, .. }
                    if current == Some(section) && k.eq_ignore_ascii_case(key) =>
                {
                    return Some(i);
                }
                _ => {}
            }
        }
        None
    }

    /// Index just after the last entry of `section`, or `None` if absent.
    fn section_insert_index(&self, section: &str) -> Option<usize> {
        let mut current: Option<&str> = None;
        let mut found = false;
        let mut last = 0;
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::Section(name) => {
                    current = Some(name.as_str());
                    if name == section {
                        found = true;
                        last = i + 1;
                    }
                }
                Line::Entry { .. } if current == Some(section) => last = i + 1,
                _ => {}
            }
        }
        found.then_some(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        "# global config\n[wsl2]\nmemory=8GB\nunknownKey=keepme\n\n[interop]\nenabled=true\n";

    #[test]
    fn round_trips_comments_and_unknown_keys() {
        let doc = IniDoc::parse(SAMPLE);
        assert_eq!(doc.render(), SAMPLE);
    }

    #[test]
    fn gets_values_case_insensitively() {
        let doc = IniDoc::parse(SAMPLE);
        assert_eq!(doc.get("wsl2", "memory"), Some("8GB"));
        assert_eq!(doc.get("wsl2", "MEMORY"), Some("8GB"));
        assert_eq!(doc.get("interop", "enabled"), Some("true"));
        assert_eq!(doc.get("wsl2", "missing"), None);
    }

    #[test]
    fn updates_existing_key_only() {
        let mut doc = IniDoc::parse(SAMPLE);
        doc.set("wsl2", "memory", "16GB");
        assert_eq!(doc.get("wsl2", "memory"), Some("16GB"));
        // Unknown key and comment survive.
        assert_eq!(doc.get("wsl2", "unknownKey"), Some("keepme"));
        assert!(doc.render().contains("# global config"));
    }

    #[test]
    fn inserts_new_key_into_existing_section() {
        let mut doc = IniDoc::parse(SAMPLE);
        doc.set("wsl2", "swap", "0");
        let rendered = doc.render();
        // The new key lands inside [wsl2], before [interop].
        let wsl2 = rendered.find("[wsl2]").unwrap();
        let swap = rendered.find("swap=0").unwrap();
        let interop = rendered.find("[interop]").unwrap();
        assert!(wsl2 < swap && swap < interop);
    }

    #[test]
    fn creates_missing_section() {
        let mut doc = IniDoc::parse(SAMPLE);
        doc.set("user", "default", "me");
        let rendered = doc.render();
        assert!(rendered.contains("[user]"));
        assert_eq!(doc.get("user", "default"), Some("me"));
    }

    #[test]
    fn empty_value_removes_key() {
        let mut doc = IniDoc::parse(SAMPLE);
        doc.set("wsl2", "memory", "");
        assert_eq!(doc.get("wsl2", "memory"), None);
        assert!(!doc.render().contains("memory="));
        // Other keys untouched.
        assert_eq!(doc.get("wsl2", "unknownKey"), Some("keepme"));
    }
}
