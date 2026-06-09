//! State for the configuration editor modal: a form over known keys and a raw
//! text editor, kept in sync through an [`IniDoc`].

use super::modal::TextField;
use crate::config::{schema, ConfigKey, ConfigTarget, IniDoc};

/// Which editing view is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Editing known keys as fields.
    Form,
    /// Editing the raw file text.
    Raw,
}

/// One known-key form field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigField {
    /// The schema key.
    pub key: ConfigKey,
    /// The editable value.
    pub input: TextField,
}

/// A minimal multi-line text editor (column positions are in characters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEditor {
    /// The text lines.
    pub lines: Vec<String>,
    /// Cursor row.
    pub row: usize,
    /// Cursor column (in characters).
    pub col: usize,
}

impl RawEditor {
    /// Build an editor from text (always has at least one line).
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            lines,
            row: 0,
            col: 0,
        }
    }

    /// The full text (newline-terminated).
    pub fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }

    /// Insert a character at the cursor.
    pub fn insert(&mut self, c: char) {
        let line = &mut self.lines[self.row];
        let idx = byte_index(line, self.col);
        line.insert(idx, c);
        self.col += 1;
    }

    /// Delete the character before the cursor (joining lines at column 0).
    pub fn backspace(&mut self) {
        if self.col > 0 {
            let line = &mut self.lines[self.row];
            let idx = byte_index(line, self.col - 1);
            line.remove(idx);
            self.col -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.lines[self.row].push_str(&current);
        }
    }

    /// Split the current line at the cursor.
    pub fn newline(&mut self) {
        let line = &mut self.lines[self.row];
        let idx = byte_index(line, self.col);
        let rest = line.split_off(idx);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
    }

    /// Move the cursor left.
    pub fn left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
        }
    }

    /// Move the cursor right.
    pub fn right(&mut self) {
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    /// Move the cursor up.
    pub fn up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.lines[self.row].chars().count());
        }
    }

    /// Move the cursor down.
    pub fn down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.lines[self.row].chars().count());
        }
    }
}

fn byte_index(line: &str, char_col: usize) -> usize {
    line.char_indices()
        .nth(char_col)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// The configuration editor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEditState {
    /// Which file is being edited.
    pub target: ConfigTarget,
    /// The parsed document (source of truth for unknown keys/comments).
    pub doc: IniDoc,
    /// Known-key form fields.
    pub fields: Vec<ConfigField>,
    /// Focused field index (form mode).
    pub focus: usize,
    /// Active view.
    pub mode: EditMode,
    /// Raw text editor (raw mode).
    pub raw: RawEditor,
}

impl ConfigEditState {
    /// Build editor state from a target and the file's current content.
    pub fn new(target: ConfigTarget, content: &str) -> Self {
        let doc = IniDoc::parse(content);
        let fields = schema(&target)
            .iter()
            .map(|key| ConfigField {
                key: *key,
                input: TextField::new(doc.get(key.section, key.key).unwrap_or("")),
            })
            .collect();
        Self {
            target,
            doc,
            fields,
            focus: 0,
            mode: EditMode::Form,
            raw: RawEditor::from_text(content),
        }
    }

    fn apply_fields_to_doc(&mut self) {
        for field in &self.fields {
            self.doc
                .set(field.key.section, field.key.key, field.input.value.trim());
        }
    }

    /// Switch to raw mode, flushing form edits into the raw text.
    pub fn to_raw(&mut self) {
        self.apply_fields_to_doc();
        self.raw = RawEditor::from_text(&self.doc.render());
        self.mode = EditMode::Raw;
    }

    /// Switch to form mode, re-parsing the raw text into fields.
    pub fn to_form(&mut self) {
        self.doc = IniDoc::parse(&self.raw.text());
        for field in &mut self.fields {
            field.input.value = self
                .doc
                .get(field.key.section, field.key.key)
                .unwrap_or("")
                .to_string();
        }
        self.mode = EditMode::Form;
    }

    /// The content to save, based on the active view (preserves unknown keys
    /// and comments via the underlying [`IniDoc`]).
    pub fn rendered(&self) -> String {
        match self.mode {
            EditMode::Form => {
                let mut doc = self.doc.clone();
                for field in &self.fields {
                    doc.set(field.key.section, field.key.key, field.input.value.trim());
                }
                doc.render()
            }
            EditMode::Raw => self.raw.text(),
        }
    }

    /// Move focus to the next field.
    pub fn focus_next(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + 1) % self.fields.len();
        }
    }

    /// Move focus to the previous field.
    pub fn focus_prev(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
        }
    }

    /// The focused field, if any.
    pub fn current_field_mut(&mut self) -> Option<&mut ConfigField> {
        self.fields.get_mut(self.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = "[wsl2]\nmemory=8GB\nunknownKey=keep\n";

    #[test]
    fn raw_editor_inserts_and_backspaces() {
        let mut editor = RawEditor::from_text("ab\n");
        editor.right(); // after 'a'
        editor.insert('X');
        assert_eq!(editor.lines[0], "aXb");
        editor.backspace();
        assert_eq!(editor.lines[0], "ab");
    }

    #[test]
    fn raw_editor_newline_splits_line() {
        let mut editor = RawEditor::from_text("abcd\n");
        editor.right();
        editor.right();
        editor.newline();
        assert_eq!(editor.lines, vec!["ab".to_string(), "cd".to_string()]);
    }

    #[test]
    fn form_prefills_from_doc() {
        let state = ConfigEditState::new(ConfigTarget::WslConfig, CONTENT);
        let memory = state.fields.iter().find(|f| f.key.key == "memory").unwrap();
        assert_eq!(memory.input.value, "8GB");
    }

    #[test]
    fn rendered_form_preserves_unknown_keys() {
        let mut state = ConfigEditState::new(ConfigTarget::WslConfig, CONTENT);
        let memory = state
            .fields
            .iter_mut()
            .find(|f| f.key.key == "memory")
            .unwrap();
        memory.input.value = "16GB".to_string();
        let rendered = state.rendered();
        assert!(rendered.contains("memory=16GB"));
        assert!(rendered.contains("unknownKey=keep"));
    }

    #[test]
    fn form_to_raw_to_form_round_trip() {
        let mut state = ConfigEditState::new(ConfigTarget::WslConfig, CONTENT);
        state.to_raw();
        assert_eq!(state.mode, EditMode::Raw);
        assert!(state.raw.text().contains("unknownKey=keep"));
        state.to_form();
        assert_eq!(state.mode, EditMode::Form);
        let memory = state.fields.iter().find(|f| f.key.key == "memory").unwrap();
        assert_eq!(memory.input.value, "8GB");
    }
}
