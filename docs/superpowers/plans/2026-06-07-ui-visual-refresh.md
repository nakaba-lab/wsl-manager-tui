# wslm ビジュアルリフレッシュ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wslm` の TUI を「洗練ミニマル / ティール」方向で全面リフレッシュする（色・丸枠・ステート色分け・インラインゲージ・文脈連動フッター）。機能は変えない。

**Architecture:** 変更は `ui` 層に限定する。新規 `src/ui/theme.rs`（パレット＋色決定）と `src/ui/footer.rs`（文脈連動フッターの純関数）を追加し、`src/ui/util.rs` にゲージ/整列ヘルパを足す。`src/ui/mod.rs` の各 render 関数をスパン化・丸枠化し、`src/i18n/mod.rs` にフッター用キーを追加する。`app`/`wsl`/`metrics`/`runtime` は不変（MVU 純粋性と crossterm 非依存を維持）。

**Tech Stack:** Rust 1.96.0 / ratatui 0.30.1 / crossterm 0.29 / unicode-width 0.2。Truecolor 使用可・記号はプレーン Unicode 限定（Nerd Font 非依存）。

設計仕様: `docs/superpowers/specs/2026-06-07-ui-visual-refresh-design.md`

---

## File Structure

- **新規** `src/ui/theme.rs` — 色パレット定数（`ACCENT`/`DIM`/`STAR`/`SPARK`/`SELECTION_BG`）と `state_color`/`gauge_color`。`ui` 内のみ参照（`pub(super)`）。
- **新規** `src/ui/footer.rs` — `context_line(&Model)`/`global_line(&Model)`。`Model` を読み取り専用で受け、`Line` を返す純関数。
- **変更** `src/ui/util.rs` — `gauge_spans(ratio, cells)`・`pad_display(s, cols)` を追加。
- **変更** `src/ui/mod.rs` — `view` を 4 分割、`render_table`/`render_detail`/`render_status` をスパン化、`render_footer` 追加、全モーダルを丸枠＋ティールタイトルに統一、`bordered()` ヘルパ追加、テスト更新／追加。
- **変更** `src/i18n/mod.rs` — `Key` に `Hint*`（14 個）と `DetailTrend` を追加、`Key::ALL`・`entry()` を更新。
- **不変** `src/app/**`・`src/wsl/**`・`src/metrics/**`・`src/runtime/**`・`src/prefs/**`。

各タスクは独立してビルド・テスト可能な単位にする。検証は全タスク共通で:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

---

## Task 1: テーマモジュール（パレット＋色決定）

**Files:**
- Create: `src/ui/theme.rs`
- Modify: `src/ui/mod.rs`（`mod theme;` 宣言を追加）

- [ ] **Step 1: 失敗するテストを書く**

`src/ui/theme.rs` を新規作成し、以下を記述する:

```rust
//! UI カラーパレットと、状態・使用率から色を決める純粋関数。`ui` 層内のみ参照。
//! セマンティック色（緑/黄/赤）とアクセント色（ティール）を分離するのが原則。

use ratatui::style::Color;

use crate::wsl::DistroState;

/// アクセント（署名色）: タイトル・フォーカス枠・フッターのキー記号。
pub(super) const ACCENT: Color = Color::Rgb(38, 166, 154); // teal
/// 二次情報（ラベル・パス・ヘッダ・空ゲージ）。
pub(super) const DIM: Color = Color::DarkGray;
/// 既定★。
pub(super) const STAR: Color = Color::Yellow;
/// スパークライン。
pub(super) const SPARK: Color = Color::Cyan;
/// 選択行の背景（淡いティール）。
pub(super) const SELECTION_BG: Color = Color::Rgb(16, 48, 48);

/// ゲージ警告色（使用率 ≥75%）。
const WARN: Color = Color::Rgb(240, 160, 48); // amber
/// ゲージ危険色（使用率 ≥90%）。
const CRIT: Color = Color::Red;

/// ディストロ状態の色。Running=緑 / Stopped=dim / Installing=アンバー / Unknown=赤。
pub(super) fn state_color(state: DistroState) -> Color {
    match state {
        DistroState::Running => Color::Green,
        DistroState::Stopped => DIM,
        DistroState::Installing => WARN,
        DistroState::Unknown => CRIT,
    }
}

/// 使用率(0.0..=1.0)からゲージ充填色を決める。<0.75 ティール / <0.90 アンバー / それ以上 赤。
pub(super) fn gauge_color(ratio: f64) -> Color {
    if ratio >= 0.90 {
        CRIT
    } else if ratio >= 0.75 {
        WARN
    } else {
        ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_color_maps_each_variant() {
        assert_eq!(state_color(DistroState::Running), Color::Green);
        assert_eq!(state_color(DistroState::Stopped), DIM);
        assert_eq!(state_color(DistroState::Installing), WARN);
        assert_eq!(state_color(DistroState::Unknown), CRIT);
    }

    #[test]
    fn gauge_color_thresholds() {
        assert_eq!(gauge_color(0.0), ACCENT);
        assert_eq!(gauge_color(0.749), ACCENT);
        assert_eq!(gauge_color(0.75), WARN);
        assert_eq!(gauge_color(0.899), WARN);
        assert_eq!(gauge_color(0.90), CRIT);
        assert_eq!(gauge_color(1.0), CRIT);
    }
}
```

- [ ] **Step 2: テストが失敗（コンパイルエラー）することを確認**

Run: `cargo test --lib ui::theme`
Expected: FAIL — `module 'theme' not found` 等（まだ `mod theme;` 未宣言）。

- [ ] **Step 3: `mod theme;` を宣言**

`src/ui/mod.rs` の `mod util;`（24 行目付近）の直前に追加:

```rust
mod theme;
mod util;
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --lib ui::theme`
Expected: PASS（2 tests）。
注: この時点で `theme` の各定数は他から未使用なので `dead_code` 警告が出るが、後続タスクで使う。最終 clippy ゲートは Task 9 まで保留してよい。一時的に出る場合は Task 5 以降で解消される。

- [ ] **Step 5: コミット**

```sh
git add src/ui/theme.rs src/ui/mod.rs
git commit -m "feat(ui): add theme module (teal palette, state/gauge colors)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: ゲージ／整列ヘルパ（`ui/util.rs`）

**Files:**
- Modify: `src/ui/util.rs`

- [ ] **Step 1: 失敗するテストを書く**

`src/ui/util.rs` の先頭の `use` を以下に置き換える（既存は `use ratatui::layout::Rect;` と `unicode_width` のみ）:

```rust
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme;
```

ファイル末尾の `#[cfg(test)] mod tests { ... }` 内に以下のテストを追加（既存テストは残す）:

```rust
    #[test]
    fn gauge_spans_fill_counts() {
        let text: String = gauge_spans(0.5, 10)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "▕█████░░░░░▏");

        let full: String = gauge_spans(1.0, 10)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(full, "▕██████████▏");

        let empty: String = gauge_spans(0.0, 10)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(empty, "▕░░░░░░░░░░▏");
    }

    #[test]
    fn gauge_spans_clamps_and_colors() {
        // クランプ: 2.0 → 全充填、-1.0 → 空。
        let over: String = gauge_spans(2.0, 4).iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(over, "▕████▏");
        // 充填スパン（index 1）の色がしきい値色に一致する。
        assert_eq!(gauge_spans(0.5, 4)[1].style.fg, Some(theme::gauge_color(0.5)));
        assert_eq!(gauge_spans(0.8, 4)[1].style.fg, Some(theme::gauge_color(0.8)));
        assert_eq!(gauge_spans(0.95, 4)[1].style.fg, Some(theme::gauge_color(0.95)));
    }

    #[test]
    fn pad_display_pads_by_columns() {
        assert_eq!(pad_display("State", 8), "State   ");
        // CJK は 2 桁: "メモリ" = 6 桁 → 2 桁分パディング。
        assert_eq!(pad_display("メモリ", 8), "メモリ  ");
        // 既に幅以上ならそのまま。
        assert_eq!(pad_display("toolong", 3), "toolong");
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --lib ui::util`
Expected: FAIL — `gauge_spans`/`pad_display` 未定義。

- [ ] **Step 3: 実装を書く**

`src/ui/util.rs` の `human_size` 関数の直後（`#[cfg(test)]` の前）に追加:

```rust
/// 使用率 `ratio`(0.0..=1.0 にクランプ) を、両端キャップ付きの `▕███░░░▏` ゲージの
/// スパン列にする。`cells` は内部バーの桁数。充填部は [`theme::gauge_color`]、
/// 空部とキャップは [`theme::DIM`]。
pub(super) fn gauge_spans(ratio: f64, cells: u16) -> Vec<Span<'static>> {
    let ratio = ratio.clamp(0.0, 1.0);
    let cells = cells.max(1);
    let filled = (ratio * cells as f64).round() as u16;
    let filled = filled.min(cells);
    let empty = cells - filled;
    let dim = Style::default().fg(theme::DIM);
    vec![
        Span::styled("▕", dim),
        Span::styled(
            "█".repeat(filled as usize),
            Style::default().fg(theme::gauge_color(ratio)),
        ),
        Span::styled("░".repeat(empty as usize), dim),
        Span::styled("▏", dim),
    ]
}

/// `s` を表示桁数 `cols` まで右側スペースで埋める（CJK 幅対応）。`cols` 以上なら無加工。
pub(super) fn pad_display(s: &str, cols: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w >= cols {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(cols - w))
    }
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --lib ui::util`
Expected: PASS（既存 2 + 新規 3）。

- [ ] **Step 5: コミット**

```sh
git add src/ui/util.rs
git commit -m "feat(ui): add gauge_spans and pad_display helpers

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: i18n キー追加（フッター動詞 ＋ Trend）

**Files:**
- Modify: `src/i18n/mod.rs`

- [ ] **Step 1: `Key` enum に追加**

`src/i18n/mod.rs` の `Key` enum 内、`DetailVmCpuTrend,`（63 行目付近）の直後に追加:

```rust
    DetailTrend,
    // Footer context hints.
    HintShell,
    HintTab,
    HintStart,
    HintStop,
    HintDefault,
    HintExport,
    HintImport,
    HintRemove,
    HintRefresh,
    HintMove,
    HintFilter,
    HintHelp,
    HintQuit,
```

- [ ] **Step 2: `Key::ALL` に追加**

`Key::ALL` 配列内の `Key::DetailVmCpuTrend,`（165 行目付近）の直後に追加:

```rust
        Key::DetailTrend,
        Key::HintShell,
        Key::HintTab,
        Key::HintStart,
        Key::HintStop,
        Key::HintDefault,
        Key::HintExport,
        Key::HintImport,
        Key::HintRemove,
        Key::HintRefresh,
        Key::HintMove,
        Key::HintFilter,
        Key::HintHelp,
        Key::HintQuit,
```

- [ ] **Step 3: `entry()` に翻訳を追加**

`entry()` の `Key::DetailVmCpuTrend => ("CPU", "CPU推移"),`（262 行目付近）の直後に追加:

```rust
        Key::DetailTrend => ("Trend", "推移"),
        Key::HintShell => ("shell", "シェル"),
        Key::HintTab => ("tab", "タブ"),
        Key::HintStart => ("start", "起動"),
        Key::HintStop => ("stop", "停止"),
        Key::HintDefault => ("default", "既定"),
        Key::HintExport => ("export", "書出"),
        Key::HintImport => ("import", "取込"),
        Key::HintRemove => ("remove", "削除"),
        Key::HintRefresh => ("refresh", "更新"),
        Key::HintMove => ("move", "移動"),
        Key::HintFilter => ("filter", "フィルタ"),
        Key::HintHelp => ("help", "ヘルプ"),
        Key::HintQuit => ("quit", "終了"),
```

- [ ] **Step 4: 既存テストで完全性を確認**

Run: `cargo test --lib i18n`
Expected: PASS — `every_key_has_both_languages` が新キーを含めて緑（`ALL` と `entry()` が揃っているか検証）。

- [ ] **Step 5: コミット**

```sh
git add src/i18n/mod.rs
git commit -m "i18n: add footer-hint keys and Trend label

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 文脈連動フッター（`ui/footer.rs`）

**Files:**
- Create: `src/ui/footer.rs`
- Modify: `src/ui/mod.rs`（`mod footer;` 宣言、テスト用ヘルパ追加）

- [ ] **Step 1: `footer.rs` を作成**

`src/ui/footer.rs`:

```rust
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
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*verb).to_string(), Style::default().fg(theme::DIM)));
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
```

- [ ] **Step 2: `mod footer;` を宣言**

`src/ui/mod.rs` の `mod theme;`（Task 1 で追加）の並びに加える:

```rust
mod footer;
mod theme;
mod util;
```

- [ ] **Step 3: テスト用ヘルパとフッターのテストを追加（失敗確認込み）**

`src/ui/mod.rs` の `#[cfg(test)] mod tests` 内、`render` 関数の直後に Line テキスト抽出ヘルパを追加:

```rust
    fn line_text(line: &ratatui::text::Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }
```

同 tests モジュールに以下のテストを追加:

```rust
    #[test]
    fn footer_context_running_offers_stop_not_start() {
        let model = sample(); // Debian = Running, 選択中
        let text = line_text(&super::footer::context_line(&model));
        assert!(text.contains("stop"), "running footer should offer stop");
        assert!(!text.contains("start"), "running footer must not offer start");
        assert!(text.contains("tab"), "running footer should offer new-tab shell");
    }

    #[test]
    fn footer_context_stopped_offers_start_not_stop() {
        let model = Model {
            distros: vec![distro_named("Ubuntu")], // distro_named = Stopped
            loaded: true,
            ..Default::default()
        };
        let text = line_text(&super::footer::context_line(&model));
        assert!(text.contains("start"), "stopped footer should offer start");
        assert!(!text.contains("stop"), "stopped footer must not offer stop");
        assert!(text.contains("remove"), "stopped footer should offer remove");
    }

    #[test]
    fn footer_global_nav_follows_keybind_style() {
        use crate::prefs::KeybindStyle;
        let mut model = sample();
        model.keybind_style = KeybindStyle::Both;
        assert!(line_text(&super::footer::global_line(&model)).contains("↑↓/jk"));
        model.keybind_style = KeybindStyle::VimOnly;
        assert!(line_text(&super::footer::global_line(&model)).contains("jk"));
        model.keybind_style = KeybindStyle::ArrowsOnly;
        let arrows = line_text(&super::footer::global_line(&model));
        assert!(arrows.contains("↑↓") && !arrows.contains("jk"));
    }
```

Run: `cargo test --lib ui::tests::footer`
Expected: PASS（`context_line`/`global_line` は Step 1 で実装済みのため、ヘルパ追加後に緑になる）。
※ 万一 `mod footer;` 配線漏れでコンパイルエラーになる場合は Step 2 を確認。

- [ ] **Step 4: 全体ビルド確認**

Run: `cargo test --lib ui`
Expected: PASS。

- [ ] **Step 5: コミット**

```sh
git add src/ui/footer.rs src/ui/mod.rs
git commit -m "feat(ui): context-aware footer (state- and keybind-driven)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: テーブルのスタイル刷新（丸枠・色分け・選択バー）

**Files:**
- Modify: `src/ui/mod.rs`（`render_table`/`distro_row`、`bordered` ヘルパ追加、import、テスト）

- [ ] **Step 1: import とヘルパを追加**

`src/ui/mod.rs` の冒頭 import 群を以下のように補う（既存に加えて `BorderType`・`Line`・`Span` を追加）:

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline,
    Table, TableState, Wrap,
};
use ratatui::Frame;
```

`view` 関数の直後に丸枠ヘルパを追加:

```rust
/// 指定色の丸枠ブロック。タイトルは呼び出し側で付ける。
fn bordered(color: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
}

/// アクセント色＋太字のパネルタイトル。
fn panel_title(text: String) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
    ))
}
```

- [ ] **Step 2: 失敗するテストを書く（色検証ヘルパ込み）**

`#[cfg(test)] mod tests` 内、`render` ヘルパの直後にバッファ取得＆色検索ヘルパを追加:

```rust
    fn render_buf(model: &Model, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| view(f, model)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// 最初に `sym` を表示しているセルの前景色。
    fn fg_of(buf: &ratatui::buffer::Buffer, sym: &str) -> Option<ratatui::style::Color> {
        buf.content.iter().find(|c| c.symbol() == sym).map(|c| c.fg)
    }

    /// 最初に `sym` を表示しているセルの背景色。
    fn bg_of(buf: &ratatui::buffer::Buffer, sym: &str) -> Option<ratatui::style::Color> {
        buf.content.iter().find(|c| c.symbol() == sym).map(|c| c.bg)
    }
```

同 tests に新規テストを追加:

```rust
    #[test]
    fn table_running_dot_is_green_when_not_selected() {
        // 停止中を選択（index 0）、実行中を非選択（index 1）にして
        // 選択ハイライトに上書きされない実行中ドットの色を検証する。
        let model = Model {
            distros: vec![distro_named("Alpine"), {
                let mut d = distro_named("Debian");
                d.state = DistroState::Running;
                d
            }],
            loaded: true,
            ..Default::default()
        };
        let buf = render_buf(&model, 90, 24);
        assert_eq!(fg_of(&buf, "●"), Some(ratatui::style::Color::Green));
    }

    #[test]
    fn table_selected_row_has_selection_bg() {
        let buf = render_buf(&sample(), 90, 24);
        // 選択シンボル "▌" に選択背景色が付く。
        assert_eq!(bg_of(&buf, "▌"), Some(super::theme::SELECTION_BG));
    }
```

Run: `cargo test --lib ui::tests::table_`
Expected: FAIL — まだ `●` は無着色（白）、`▌` も未描画。

- [ ] **Step 3: `render_table` と `distro_row` を置き換える**

`src/ui/mod.rs` の既存 `render_table`／`distro_row` を以下で置き換える:

```rust
fn render_table(f: &mut Frame, model: &Model, area: Rect) {
    let lang = model.lang;
    let header = Row::new([
        t(lang, Key::ColName),
        t(lang, Key::ColState),
        t(lang, Key::ColVer),
        t(lang, Key::ColDefault),
        t(lang, Key::ColDisk),
    ])
    .style(Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD));
    let visible = model.visible_distros();
    let rows = visible.iter().map(|&distro| distro_row(lang, distro));
    let widths = [
        Constraint::Min(16),
        Constraint::Length(12),
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(12),
    ];
    let block = bordered(theme::ACCENT)
        .title(panel_title(" wslm — WSL Manager ".to_string()))
        .title(Line::from(format!(" {} ", lang.label())).right_aligned());
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(
            Style::default()
                .bg(theme::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");

    let mut state = TableState::default();
    if !visible.is_empty() {
        state.select(Some(model.selected.min(visible.len() - 1)));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn distro_row(lang: Lang, distro: &Distro) -> Row<'static> {
    let state = Line::from(vec![
        Span::styled(
            format!("{} ", distro.state.glyph()),
            Style::default().fg(theme::state_color(distro.state)),
        ),
        Span::raw(state_label(lang, distro.state).to_string()),
    ]);
    let default = if distro.is_default {
        Line::from(Span::styled("★", Style::default().fg(theme::STAR)))
    } else {
        Line::from("")
    };
    let disk = distro
        .disk_bytes
        .map(human_size)
        .unwrap_or_else(|| "—".to_string());
    Row::new(vec![
        Cell::from(distro.name.clone()),
        Cell::from(state),
        Cell::from(Span::styled(
            distro.version.to_string(),
            Style::default().fg(theme::DIM),
        )),
        Cell::from(default),
        Cell::from(Span::styled(disk, Style::default().fg(theme::DIM))),
    ])
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --lib ui::tests::table_`
Expected: PASS（新規 2）。

既存テスト `renders_title_and_distro`・`renders_in_japanese_when_lang_is_ja`・`table_shows_only_filtered_rows` も確認:

Run: `cargo test --lib ui`
Expected: PASS（`WSL Manager`/`Debian`/`Running`/`4.0 GB`、`名`/`実`/`JA`、フィルタ表示は維持）。

- [ ] **Step 5: コミット**

```sh
git add src/ui/mod.rs
git commit -m "feat(ui): restyle distro table (rounded teal frame, state-colored dots, selection bar)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: 詳細ペインのスタイル刷新（スパン化・インラインゲージ）

**Files:**
- Modify: `src/ui/mod.rs`（`render_detail` 全面置換、補助関数追加、`render_sparkline_row`/`vm_mem_line`/`vm_cpu_line` 整理、テスト更新）

- [ ] **Step 1: 既存テストを新レイアウトに合わせて更新（先に直す）**

`renders_detail_pane_with_vm_memory_and_cpu` の最初のアサーションを差し替える（詳細パネルのタイトルは「Detail: 名前」→「名前」だけになるため）:

```rust
        assert!(rendered.contains("Debian"), "detail title missing");
```

（`assert!(rendered.contains("Detail: Debian"), ...)` の行を上記に置換。残りの `VM Mem`/`2.0 GB`/`VM CPU`/`50.0 %`/`matches("Mem")>=2`/`matches("CPU")>=2` はそのまま維持する。）

`vm_memory_denominator_uses_wsl_vm_ram_when_known` はそのまま（`"2.0 GB / 4.0 GB"` の表記は新実装でも一致）。

- [ ] **Step 2: 失敗するテストを書く**

同 tests に追加:

```rust
    #[test]
    fn detail_memory_gauge_is_colored_by_usage() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(1 * 1024 * 1024 * 1024), // 1 GB used
            total_mem_bytes: 8 * 1024 * 1024 * 1024,   // of 8 GB → 12.5%
            ..Default::default()
        });
        let buf = render_buf(&model, 110, 24);
        // 充填ブロックが描画され、低使用率なのでアクセント(ティール)色。
        assert_eq!(fg_of(&buf, "█"), Some(super::theme::ACCENT));
        // 空部キャップが存在する。
        assert!(buf.content.iter().any(|c| c.symbol() == "▕"));
    }
```

Run: `cargo test --lib ui::tests::detail_memory_gauge`
Expected: FAIL — まだゲージ未描画（`█`/`▕` が無い）。

- [ ] **Step 3: `render_detail` と補助関数を置き換える**

`src/ui/mod.rs` の既存 `render_detail` を以下で置換。あわせて旧 `render_sparkline_row`・`vm_mem_line`・`vm_cpu_line` を削除し、下記の新補助関数群に置き換える:

```rust
fn render_detail(f: &mut Frame, model: &Model, area: Rect) {
    let lang = model.lang;
    let title = match model.selected_distro() {
        Some(distro) => format!(" {} ", distro.name),
        None => format!(" {} ", t(lang, Key::DetailTitle)),
    };
    let block = bordered(theme::DIM).title(panel_title(title));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(distro) = model.selected_distro() else {
        f.render_widget(
            Paragraph::new(Span::styled(
                t(lang, Key::NoDistros),
                Style::default().fg(theme::DIM),
            )),
            inner,
        );
        return;
    };

    // ラベル列幅（表示桁）: 表示する全ラベルの最大幅 + 1。言語ごとに整列する。
    let lw = [
        Key::DetailState,
        Key::DetailVersion,
        Key::DetailDefault,
        Key::DetailDisk,
        Key::DetailPath,
        Key::DetailVmMem,
        Key::DetailVmCpu,
        Key::DetailTrend,
    ]
    .iter()
    .map(|&k| UnicodeWidthStr::width(t(lang, k)))
    .max()
    .unwrap_or(8)
        + 1;

    // 行プラン（重要度順）。狭い高さでは末尾（Trend → Inner）から落ちる。
    #[derive(Clone, Copy)]
    enum DetailRow {
        StateDefault,
        VersionDisk,
        Path,
        Mem,
        Cpu,
        Trend,
        Inner,
    }
    let mut plan = vec![
        DetailRow::StateDefault,
        DetailRow::VersionDisk,
        DetailRow::Path,
        DetailRow::Mem,
        DetailRow::Cpu,
        DetailRow::Trend,
    ];
    if distro.inner_disk.is_some() {
        plan.push(DetailRow::Inner);
    }
    let n = (inner.height as usize).min(plan.len());
    if n == 0 {
        return;
    }
    let rows = Layout::vertical(vec![Constraint::Length(1); n]).split(inner);

    for (i, row) in plan[..n].iter().enumerate() {
        let r = rows[i];
        match row {
            DetailRow::StateDefault => {
                let cols =
                    Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                        .split(r);
                let state_val = vec![
                    Span::styled(
                        format!("{} ", distro.state.glyph()),
                        Style::default().fg(theme::state_color(distro.state)),
                    ),
                    Span::raw(state_label(lang, distro.state).to_string()),
                ];
                render_kv(f, cols[0], lang, lw, Key::DetailState, state_val);
                let def_val = if distro.is_default {
                    vec![Span::styled("★", Style::default().fg(theme::STAR))]
                } else {
                    vec![Span::styled("—", Style::default().fg(theme::DIM))]
                };
                render_kv(f, cols[1], lang, lw, Key::DetailDefault, def_val);
            }
            DetailRow::VersionDisk => {
                let cols =
                    Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                        .split(r);
                render_kv(
                    f,
                    cols[0],
                    lang,
                    lw,
                    Key::DetailVersion,
                    vec![Span::raw(distro.version.to_string())],
                );
                let disk = distro
                    .disk_bytes
                    .map(human_size)
                    .unwrap_or_else(|| "—".to_string());
                render_kv(f, cols[1], lang, lw, Key::DetailDisk, vec![Span::raw(disk)]);
            }
            DetailRow::Path => {
                let path = distro
                    .base_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "—".to_string());
                let budget = (r.width as usize).saturating_sub(lw + 1);
                let path = truncate_width(&path, budget);
                render_kv(
                    f,
                    r,
                    lang,
                    lw,
                    Key::DetailPath,
                    vec![Span::styled(path, Style::default().fg(theme::DIM))],
                );
            }
            DetailRow::Mem => {
                let used = model.metrics.latest_vmmem;
                let total = mem_total(model);
                let (pre, ratio, post) = match (used, total) {
                    (Some(u), Some(tt)) if tt > 0 => {
                        let ratio = u as f64 / tt as f64;
                        (
                            format!("{} / {} ", human_size(u), human_size(tt)),
                            Some(ratio),
                            format!(" {:.0}%", ratio.clamp(0.0, 1.0) * 100.0),
                        )
                    }
                    (Some(u), _) => (format!("{} ", human_size(u)), None, String::new()),
                    (None, _) => (t(lang, Key::VmNotRunning).to_string(), None, String::new()),
                };
                render_gauge_row(f, r, lang, lw, Key::DetailVmMem, pre, ratio, post);
            }
            DetailRow::Cpu => {
                let (pre, ratio) = match model.metrics.latest_vm_cpu_pct {
                    Some(p) => (format!("{p:.1} % "), Some((p / 100.0).clamp(0.0, 1.0))),
                    None => ("—".to_string(), None),
                };
                render_gauge_row(f, r, lang, lw, Key::DetailVmCpu, pre, ratio, String::new());
            }
            DetailRow::Trend => render_trend_row(
                f,
                r,
                lang,
                lw,
                &model.metrics.sparkline(),
                &model.metrics.cpu_sparkline(),
            ),
            DetailRow::Inner => {
                if let Some((u, tot)) = distro.inner_disk {
                    render_kv(
                        f,
                        r,
                        lang,
                        lw,
                        Key::DetailInnerDisk,
                        vec![Span::raw(format!("{} / {}", human_size(u), human_size(tot)))],
                    );
                }
            }
        }
    }
}

/// 「ラベル  値…」を 1 行で描く。ラベルは dim、`lw` 桁に整列。
fn render_kv(
    f: &mut Frame,
    area: Rect,
    lang: Lang,
    lw: usize,
    label: Key,
    mut value: Vec<Span<'static>>,
) {
    let mut spans = vec![Span::styled(
        pad_display(t(lang, label), lw),
        Style::default().fg(theme::DIM),
    )];
    spans.append(&mut value);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// 「ラベル  pre <gauge> post」。`ratio` が `Some` のときだけゲージを描く。
fn render_gauge_row(
    f: &mut Frame,
    area: Rect,
    lang: Lang,
    lw: usize,
    label: Key,
    pre: String,
    ratio: Option<f64>,
    post: String,
) {
    let mut spans = vec![
        Span::styled(pad_display(t(lang, label), lw), Style::default().fg(theme::DIM)),
        Span::raw(pre),
    ];
    if let Some(r) = ratio {
        spans.extend(gauge_spans(r, 14));
        spans.push(Span::raw(post));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// 「Trend  Mem ▁▂▃  CPU ▁▂▃」。半々に分けて 2 本のスパークラインを並べる。
fn render_trend_row(
    f: &mut Frame,
    area: Rect,
    lang: Lang,
    lw: usize,
    mem: &[u64],
    cpu: &[u64],
) {
    let cols = Layout::horizontal([
        Constraint::Length(lw as u16),
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(Span::styled(
            pad_display(t(lang, Key::DetailTrend), lw),
            Style::default().fg(theme::DIM),
        )),
        cols[0],
    );
    render_mini_spark(f, cols[1], t(lang, Key::DetailVmMemTrend), mem);
    render_mini_spark(f, cols[2], t(lang, Key::DetailVmCpuTrend), cpu);
}

/// 小ラベル＋スパークライン（残り幅いっぱい）。
fn render_mini_spark(f: &mut Frame, area: Rect, label: &str, data: &[u64]) {
    let label_cols = UnicodeWidthStr::width(label) as u16 + 1;
    let cols = Layout::horizontal([Constraint::Length(label_cols), Constraint::Min(1)]).split(area);
    f.render_widget(
        Paragraph::new(Span::styled(label.to_string(), Style::default().fg(theme::DIM))),
        cols[0],
    );
    f.render_widget(
        Sparkline::default()
            .data(data)
            .style(Style::default().fg(theme::SPARK)),
        cols[1],
    );
}

/// VM メモリ分母: 既知なら WSL VM の RAM 上限、未取得ならホスト物理 RAM。
fn mem_total(model: &Model) -> Option<u64> {
    model
        .vm_mem_total
        .filter(|&b| b > 0)
        .or_else(|| (model.metrics.total_mem_bytes > 0).then_some(model.metrics.total_mem_bytes))
}
```

注: 旧 `render_sparkline_row`・`vm_mem_line`・`vm_cpu_line` は削除する（上記に統合）。`MetricsHistory` の import が未使用になったら削除（`use crate::metrics::MetricsHistory;` の行）。

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --lib ui`
Expected: PASS（新規 `detail_memory_gauge_is_colored_by_usage`、更新済み `renders_detail_pane_with_vm_memory_and_cpu`、`detail_pane_survives_tiny_height`、`vm_memory_denominator_uses_wsl_vm_ram_when_known` を含む）。

- [ ] **Step 5: コミット**

```sh
git add src/ui/mod.rs
git commit -m "feat(ui): restyle detail pane (aligned spans, inline mem/cpu gauges, dual trend)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: トップレベルレイアウト＋フッター描画＋ステータス行

**Files:**
- Modify: `src/ui/mod.rs`（`view`・`render_status` 改修、`render_footer` 追加、テスト）

- [ ] **Step 1: 失敗するテストを書く**

`#[cfg(test)] mod tests` に追加:

```rust
    #[test]
    fn idle_status_shows_global_hints() {
        let buf = render_buf(&sample(), 110, 24);
        let dump: String = buf.content.iter().map(|c| c.symbol()).collect();
        // アイドル時はグローバルヒント（move/filter/help/quit）が出る。
        assert!(dump.contains("move"), "global hint 'move' missing");
        assert!(dump.contains("quit"), "global hint 'quit' missing");
    }

    #[test]
    fn footer_row_shows_context_hints() {
        let buf = render_buf(&sample(), 110, 24); // Running 選択
        let dump: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(dump.contains("shell"), "footer 'shell' hint missing");
        assert!(dump.contains("stop"), "footer 'stop' hint missing (running)");
    }
```

Run: `cargo test --lib ui::tests::idle_status_shows_global_hints ui::tests::footer_row_shows_context_hints`
Expected: FAIL — まだフッター行が無く、ステータス行は旧 `StatusHint` 文字列。

- [ ] **Step 2: `view` を 4 分割にする**

`src/ui/mod.rs` の `view` 内のレイアウトを置き換える:

```rust
pub fn view(f: &mut Frame, model: &Model) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(11),
        Constraint::Length(1), // 文脈連動フッター
        Constraint::Length(1), // ステータス行
    ])
    .split(area);
    render_table(f, model, chunks[0]);
    render_detail(f, model, chunks[1]);
    render_footer(f, model, chunks[2]);
    render_status(f, model, chunks[3]);
    if let Some(modal) = &model.modal {
        render_modal(f, modal, model.lang, area);
    }
}
```

- [ ] **Step 3: `render_footer` を追加し、`render_status` を改修**

`render_status` の直前に追加:

```rust
fn render_footer(f: &mut Frame, model: &Model, area: Rect) {
    // フィルタ入力中はステータス行がプロンプトを出すので、フッターは空に。
    if model.filter_mode {
        return;
    }
    f.render_widget(Paragraph::new(footer::context_line(model)), area);
}
```

既存 `render_status` を以下で置き換える（アイドル時はグローバルヒントの `Line` を描く）:

```rust
fn render_status(f: &mut Frame, model: &Model, area: Rect) {
    let lang = model.lang;
    if model.filter_mode {
        let prompt = format!("/{}▏", model.filter);
        f.render_widget(
            Paragraph::new(prompt).style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }
    if !model.filter.is_empty() {
        f.render_widget(
            Paragraph::new(tf(lang, Key::FilterApplied, &[&model.filter]))
                .style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }
    if let Some(error) = &model.last_error {
        f.render_widget(
            Paragraph::new(format!("{}: {error}", t(lang, Key::ErrorPrefix)))
                .style(Style::default().fg(Color::Red)),
            area,
        );
        return;
    }
    if let Some(status) = &model.status {
        f.render_widget(
            Paragraph::new(status.clone()).style(Style::default().fg(Color::Green)),
            area,
        );
        return;
    }
    if !model.loaded {
        f.render_widget(
            Paragraph::new(t(lang, Key::Loading)).style(Style::default().fg(theme::DIM)),
            area,
        );
        return;
    }
    f.render_widget(Paragraph::new(footer::global_line(model)), area);
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --lib ui`
Expected: PASS（新規 2 ＋ 既存。`table_shows_only_filtered_rows` はフィルタ適用表示が維持されることを確認）。

- [ ] **Step 5: コミット**

```sh
git add src/ui/mod.rs
git commit -m "feat(ui): four-pane layout with context footer + global-hint status line

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: モーダルの丸枠＋ティールタイトル統一

**Files:**
- Modify: `src/ui/mod.rs`（`render_help`/`render_quit`/`render_config_edit`/`render_form`/`render_progress`/`render_install_pick`/`render_import_pick`/`render_confirm`/`render_error`、テスト）

- [ ] **Step 1: 失敗するテストを書く**

`#[cfg(test)] mod tests` に追加:

```rust
    #[test]
    fn modals_use_rounded_borders() {
        let mut model = sample();
        model.modal = Some(Modal::Help);
        let buf = render_buf(&model, 90, 30);
        // 丸枠の左上コーナー "╭" が出る（角枠 "┌" ではない）。
        assert!(
            buf.content.iter().any(|c| c.symbol() == "╭"),
            "modal should use rounded corners"
        );
    }
```

Run: `cargo test --lib ui::tests::modals_use_rounded_borders`
Expected: FAIL — まだ角枠（`Borders::ALL` のデフォルト＝`┌`）。

- [ ] **Step 2: 各モーダルを `bordered()` ＋ `panel_title()` に置き換える**

`src/ui/mod.rs` の各モーダル関数で `Block::default().borders(Borders::ALL)...` を `bordered(色)` に、`.title(t(lang, Key::X))` を `.title(panel_title(t(lang, Key::X).to_string()))` に置き換える。色の対応:

- `render_confirm` / `render_quit`: `bordered(Color::Yellow)`
- `render_error`: `bordered(Color::Red)`
- `render_progress`: `bordered(theme::ACCENT)`
- `render_help` / `render_form` / `render_config_edit` / `render_install_pick` / `render_import_pick`: `bordered(theme::ACCENT)`

具体例（`render_help`）— 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::HelpTitle));
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::HelpTitle).to_string()));
```

`render_quit` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::QuitTitle))
        .border_style(Style::default().fg(Color::Yellow));
```

置換後:

```rust
    let block = bordered(Color::Yellow).title(panel_title(t(lang, Key::QuitTitle).to_string()));
```

`render_progress` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::ProgressTitle))
        .border_style(Style::default().fg(Color::Cyan));
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::ProgressTitle).to_string()));
```

`render_confirm` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::ConfirmTitle))
        .border_style(Style::default().fg(Color::Yellow));
```

置換後:

```rust
    let block = bordered(Color::Yellow).title(panel_title(t(lang, Key::ConfirmTitle).to_string()));
```

`render_error` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::ErrorTitle))
        .border_style(Style::default().fg(Color::Red));
```

置換後:

```rust
    let block = bordered(Color::Red).title(panel_title(t(lang, Key::ErrorTitle).to_string()));
```

`render_config_edit` 既存:

```rust
    let block = Block::default().borders(Borders::ALL).title(format!(
        " {} {} [{}] ",
        t(lang, Key::ConfigEditPrefix),
        state.target.label(),
        mode
    ));
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(format!(
        " {} {} [{}] ",
        t(lang, Key::ConfigEditPrefix),
        state.target.label(),
        mode
    )));
```

`render_form` 既存:

```rust
    let block = Block::default().borders(Borders::ALL).title(title);
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(title.to_string()));
```

`render_install_pick` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::InstallTitle));
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::InstallTitle).to_string()));
```

`render_import_pick` 既存:

```rust
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::PickImportTitle));
```

置換後:

```rust
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::PickImportTitle).to_string()));
```

さらにピッカー2種（`render_install_pick`/`render_import_pick`）の `List` の選択スタイルをテーブルと揃える。既存:

```rust
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
```

置換後（両関数とも）:

```rust
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(theme::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
```

- [ ] **Step 3: テストが通ることを確認**

Run: `cargo test --lib ui`
Expected: PASS（`modals_use_rounded_borders` ＋ 既存モーダルテスト `renders_confirm_modal`/`renders_export_form_modal`/`renders_progress_modal`/`renders_install_pick_modal`/`renders_import_pick_modal`/`renders_config_editor_form`/`renders_help_overlay`/`modals_localize_to_japanese`）。

注: モーダルのタイトル文字列（`Confirm`/`Working`/`Help` 等）は変わらないので既存アサーションは維持される。

- [ ] **Step 4: コミット**

```sh
git add src/ui/mod.rs
git commit -m "feat(ui): unify modals to rounded frames + teal titles, accent pickers

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: 最終検証（フォーマット・lint・全テスト・実機目視）

**Files:** なし（検証のみ）

- [ ] **Step 1: フォーマット**

Run: `cargo fmt --all`
その後 Run: `cargo fmt --all -- --check`
Expected: 差分なし（exit 0）。

- [ ] **Step 2: lint（警告＝エラー）**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 警告 0。`theme`/`util`/`footer` の未使用警告が残っていればここで解消（未使用 import の削除等）。
特に注意: `MetricsHistory` import が Task 6 で未使用化していれば削除済みであること、`Color`/`Modifier` の未使用が無いこと。

- [ ] **Step 3: 全テスト**

Run: `cargo test --all`
Expected: 全 PASS（`#[ignore]` の実機テストはスキップ）。

- [ ] **Step 4: 実機での目視確認（Windows / Windows Terminal）**

Run: `cargo run`
確認項目（spec の完成イメージ）:
- テーブル枠がティールの丸枠、タイトル右に `EN`/`JA`。
- 選択行に `▌` バーと淡い背景。ステート `●` が Running=緑 / Stopped=灰 / Installing=アンバー。
- 詳細ペインの Memory/CPU がインラインゲージ（低使用率=ティール、高使用率で色変化）。
- 下から 2 行目が選択ディストロに応じて `⏎ shell · …` と変化（実行中/停止中で内容が変わる）。
- 最下行がアイドル時 `↑↓/jk move · / filter · ? help · q quit`、操作後はステータス/エラー表示。
- `L` で日本語化しても整列が崩れない。`?` でヘルプ、各モーダルが丸枠＋ティールタイトル。

問題なければ完了。`finishing-a-development-branch` スキルでマージ/PR を判断する。

---

## Self-Review

**1. Spec coverage（spec §→task 対応）**
- §3 カラーシステム → Task 1（`theme.rs`）。
- §4 レイアウト（4 分割・狭小フォールバック）→ Task 7（`view`）＋ Task 6（行プラン truncate）。
- §5.1 丸枠 → Task 5（`bordered`/table）・Task 6（detail）・Task 8（modals）。
- §5.2 テーブル（色分け・選択バー・dim ヘッダ）→ Task 5。
- §5.3 詳細（スパン化・整列・インラインゲージ・Trend）→ Task 6。
- §5.4 `gauge_spans` → Task 2。
- §6 文脈連動フッター → Task 4（`footer.rs`）＋ Task 7（描画）。
- §7 モーダル統一 → Task 8。
- §8 i18n → Task 3。
- §9 テスト（色まで検証）→ Task 1/2/4/5/6/7/8 各テスト＋ Task 9。
- §10 スコープ外 → どのタスクも Nerd Font/テーマ切替/マウスを導入しない。

**2. Placeholder scan:** 「実装時調整」等の曖昧表現なし。各コード手順に完全なコードを記載済み。

**3. Type consistency:**
- `theme::ACCENT/DIM/STAR/SPARK/SELECTION_BG`（定数）、`theme::state_color(DistroState)->Color`、`theme::gauge_color(f64)->Color` — Task 1 定義、以降一貫使用。
- `util::gauge_spans(f64,u16)->Vec<Span<'static>>`、`util::pad_display(&str,usize)->String` — Task 2 定義、Task 6 使用。
- `footer::context_line(&Model)->Line<'static>`、`footer::global_line(&Model)->Line<'static>` — Task 4 定義、Task 7 使用。
- `bordered(Color)->Block<'static>`、`panel_title(String)->Line<'static>` — Task 5 定義、Task 6/8 使用。
- `render_kv/render_gauge_row/render_trend_row/render_mini_spark/mem_total` — Task 6 内で定義・使用。
- ratatui 0.30.1: `Cell.fg`/`Cell.bg`（pub フィールド）、`Buffer.content`、`Span.content`/`Span.style.fg`、`Line.spans`、`Line::right_aligned()`、`BorderType::Rounded` — 確認済み。
