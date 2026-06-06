//! Pure UI helpers: geometry and display-width/size formatting. No state, no
//! rendering — just functions shared by the renderers in the parent module.

use ratatui::layout::Rect;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A centered rectangle of the given size, clamped to `area`.
pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Truncate `s` to at most `max` display columns (CJK-aware), appending `…`
/// when characters are dropped.
pub(super) fn truncate_width(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1; // leave a column for the ellipsis
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + cw > budget {
            break;
        }
        out.push(c);
        width += cw;
    }
    out.push('…');
    out
}

/// Human-readable byte size using binary units.
pub(super) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_width_respects_cjk_columns() {
        assert_eq!(truncate_width("short", 10), "short");
        assert_eq!(truncate_width("abcdef", 4), "abc…");
        // Each CJK glyph is two columns: 3 glyphs = 6 columns; budget 5 -> 2
        // glyphs (4 cols) + ellipsis.
        assert_eq!(truncate_width("あいう", 5), "あい…");
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GB");
        assert_eq!(human_size(1536), "1.5 KB");
    }
}
