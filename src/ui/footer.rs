//! 最下部の文脈連動フッターの組み立て（純関数）。`Model` を読み取り専用で受け、
//! `Line` を返す。キー記号はアクセント色、説明語は dim。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::Model;
use crate::i18n::{t, Key};
use crate::wsl::DistroState;

use super::theme;

/// `(キー記号, 説明語)` の並びを `key verb · key verb` の 1 行にする。
fn hints_line(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, verb)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme::DIM)));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            (*verb).to_string(),
            Style::default().fg(theme::DIM),
        ));
    }
    Line::from(spans)
}

/// 選択中ディストロの状態に応じて「今できる操作」を出すフッター行。
pub(super) fn context_line(model: &Model) -> Line<'static> {
    let lang = model.lang;
    let mut pairs: Vec<(&str, &str)> = vec![("⏎", t(lang, Key::HintShell))];
    match model.selected_distro().map(|d| d.state) {
        Some(DistroState::Running) => {
            pairs.push(("w", t(lang, Key::HintTab)));
            pairs.push(("x", t(lang, Key::HintStop)));
            pairs.push(("d", t(lang, Key::HintDefault)));
            pairs.push(("e", t(lang, Key::HintExport)));
            pairs.push(("m", t(lang, Key::HintImport)));
        }
        Some(DistroState::Stopped) => {
            pairs.push(("s", t(lang, Key::HintStart)));
            pairs.push(("d", t(lang, Key::HintDefault)));
            pairs.push(("e", t(lang, Key::HintExport)));
            pairs.push(("u", t(lang, Key::HintRemove)));
        }
        _ => {
            pairs.push(("r", t(lang, Key::HintRefresh)));
        }
    }
    hints_line(&pairs)
}

/// アイドル時にステータス行へ出すグローバルヒント。ナビキーは `keybind_style` に追従。
pub(super) fn global_line(model: &Model) -> Line<'static> {
    let lang = model.lang;
    let nav = match (
        model.keybind_style.arrows_enabled(),
        model.keybind_style.vim_enabled(),
    ) {
        (true, true) => "↑↓/jk",
        (true, false) => "↑↓",
        (false, true) => "jk",
        (false, false) => "↑↓",
    };
    let pairs: Vec<(&str, &str)> = vec![
        (nav, t(lang, Key::HintMove)),
        ("/", t(lang, Key::HintFilter)),
        ("?", t(lang, Key::HintHelp)),
        ("q", t(lang, Key::HintQuit)),
    ];
    hints_line(&pairs)
}
