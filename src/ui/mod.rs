//! View layer: pure rendering of the [`Model`] into ratatui widgets. Renders
//! only; never mutates state. Covers the distro table, the detail pane with a
//! VM-memory sparkline, the status line, and all modals (confirm, error, form,
//! progress, install picker).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline,
    Table, TableState, Wrap,
};
use ratatui::Frame;

use crate::app::{
    ConfigEditState, Confirm, EditMode, FormKind, FormState, ImportPickState, InstallPickState,
    Modal, Model, ProgressState,
};
use crate::i18n::{t, tf, Key, Lang};
use crate::wsl::{Distro, DistroState};
use unicode_width::UnicodeWidthStr;

mod footer;
mod theme;
mod util;
use util::{centered_rect, gauge_spans, human_size, pad_display, truncate_width};

/// Render the whole UI for the current model.
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
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
}

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

    let lw = [
        Key::DetailState,
        Key::DetailVersion,
        Key::DetailDefault,
        Key::DetailDisk,
        Key::DetailPath,
        Key::DetailVmMem,
        Key::DetailVmCpu,
        Key::DetailTrend,
        Key::DetailInnerDisk,
    ]
    .iter()
    .map(|&k| UnicodeWidthStr::width(t(lang, k)))
    .max()
    .unwrap_or(8)
        + 1;

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
                let budget = (r.width as usize).saturating_sub(lw);
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
                render_gauge_row(f, r, lw, t(lang, Key::DetailVmMem), pre, ratio, post);
            }
            DetailRow::Cpu => {
                let (pre, ratio) = match model.metrics.latest_vm_cpu_pct {
                    Some(p) => (
                        format!("{p:.1} % "),
                        Some((p as f64 / 100.0).clamp(0.0, 1.0)),
                    ),
                    None => ("—".to_string(), None),
                };
                render_gauge_row(
                    f,
                    r,
                    lw,
                    t(lang, Key::DetailVmCpu),
                    pre,
                    ratio,
                    String::new(),
                );
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
                        vec![Span::raw(format!(
                            "{} / {}",
                            human_size(u),
                            human_size(tot)
                        ))],
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
    lw: usize,
    label: &str,
    pre: String,
    ratio: Option<f64>,
    post: String,
) {
    let mut spans = vec![
        Span::styled(pad_display(label, lw), Style::default().fg(theme::DIM)),
        Span::raw(pre),
    ];
    if let Some(r) = ratio {
        spans.extend(gauge_spans(r, 14));
        spans.push(Span::raw(post));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// 「Trend  Mem ▁▂▃  CPU ▁▂▃」。半々に分けて 2 本のスパークラインを並べる。
fn render_trend_row(f: &mut Frame, area: Rect, lang: Lang, lw: usize, mem: &[u64], cpu: &[u64]) {
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
        Paragraph::new(Span::styled(
            label.to_string(),
            Style::default().fg(theme::DIM),
        )),
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

fn state_label(lang: Lang, state: DistroState) -> &'static str {
    t(
        lang,
        match state {
            DistroState::Running => Key::StateRunning,
            DistroState::Stopped => Key::StateStopped,
            DistroState::Installing => Key::StateInstalling,
            DistroState::Unknown => Key::StateUnknown,
        },
    )
}

fn render_footer(f: &mut Frame, model: &Model, area: Rect) {
    // フィルタ入力中はステータス行がプロンプトを出すので、フッターは空に。
    if model.filter_mode {
        return;
    }
    f.render_widget(Paragraph::new(footer::context_line(model)), area);
}

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

fn render_modal(f: &mut Frame, modal: &Modal, lang: Lang, area: Rect) {
    match modal {
        Modal::Confirm(confirm) => render_confirm(f, confirm, lang, area),
        Modal::Error { message } => render_error(f, message, lang, area),
        Modal::Form(form) => render_form(f, form, lang, area),
        Modal::Progress(progress) => render_progress(f, progress, lang, area),
        Modal::InstallPick(pick) => render_install_pick(f, pick, lang, area),
        Modal::ImportPick(pick) => render_import_pick(f, pick, lang, area),
        Modal::ConfigEdit(state) => render_config_edit(f, state, lang, area),
        Modal::Help => render_help(f, lang, area),
        Modal::Quit => render_quit(f, lang, area),
    }
}

fn render_help(f: &mut Frame, lang: Lang, area: Rect) {
    let popup = centered_rect(66, 24, area);
    f.render_widget(Clear, popup);
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::HelpTitle).to_string()));
    f.render_widget(Paragraph::new(t(lang, Key::HelpBody)).block(block), popup);
}

fn render_quit(f: &mut Frame, lang: Lang, area: Rect) {
    let popup = centered_rect(44, 5, area);
    f.render_widget(Clear, popup);
    let block = bordered(Color::Yellow).title(panel_title(t(lang, Key::QuitTitle).to_string()));
    f.render_widget(Paragraph::new(t(lang, Key::QuitPrompt)).block(block), popup);
}

fn render_config_edit(f: &mut Frame, state: &ConfigEditState, lang: Lang, area: Rect) {
    let popup = centered_rect(82, 26, area);
    f.render_widget(Clear, popup);
    let mode = t(
        lang,
        match state.mode {
            EditMode::Form => Key::ModeForm,
            EditMode::Raw => Key::ModeRaw,
        },
    );
    let block = bordered(theme::ACCENT).title(panel_title(format!(
        " {} {} [{}] ",
        t(lang, Key::ConfigEditPrefix),
        state.target.label(),
        mode
    )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    match state.mode {
        EditMode::Form => render_config_form(f, state, rows[0]),
        EditMode::Raw => render_config_raw(f, state, rows[0]),
    }
    f.render_widget(Paragraph::new(t(lang, Key::ConfigSaveHint)), rows[1]);
}

fn render_config_form(f: &mut Frame, state: &ConfigEditState, area: Rect) {
    let mut text = String::new();
    for (i, field) in state.fields.iter().enumerate() {
        let marker = if i == state.focus { "▌ " } else { "  " };
        let cursor = if i == state.focus { "▏" } else { "" };
        text.push_str(&format!(
            "{marker}[{}] {} = {}{}   ({})\n",
            field.key.section, field.key.key, field.input.value, cursor, field.key.hint
        ));
    }
    f.render_widget(Paragraph::new(text), area);
}

fn render_config_raw(f: &mut Frame, state: &ConfigEditState, area: Rect) {
    let mut text = String::new();
    for (row, line) in state.raw.lines.iter().enumerate() {
        if row == state.raw.row {
            let head: String = line.chars().take(state.raw.col).collect();
            let tail: String = line.chars().skip(state.raw.col).collect();
            text.push_str(&format!("{head}▏{tail}\n"));
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
}

fn render_form(f: &mut Frame, form: &FormState, lang: Lang, area: Rect) {
    let title = t(
        lang,
        match &form.kind {
            FormKind::Export { .. } => Key::FormExportTitle,
            FormKind::ImportName { .. } | FormKind::ImportCustom => Key::FormImportTitle,
        },
    );
    let popup = centered_rect(72, form.fields.len() as u16 * 2 + 5, area);
    f.render_widget(Clear, popup);
    let block = bordered(theme::ACCENT).title(panel_title(title.to_string()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut text = String::new();
    for (i, &label) in form.labels.iter().enumerate() {
        let marker = if i == form.focus { "▌ " } else { "  " };
        let cursor = if i == form.focus { "▏" } else { "" };
        text.push_str(&format!(
            "{marker}{}:\n    {}{cursor}\n",
            t(lang, label),
            form.fields[i].value
        ));
    }
    text.push('\n');
    text.push_str(t(lang, Key::FormFooter));
    if matches!(form.kind, FormKind::Export { .. }) {
        text.push('\n');
        text.push_str(t(lang, Key::ExportFormatHint));
    }
    f.render_widget(Paragraph::new(text), inner);
}

fn render_progress(f: &mut Frame, progress: &ProgressState, lang: Lang, area: Rect) {
    let popup = centered_rect(60, 5, area);
    f.render_widget(Clear, popup);
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::ProgressTitle).to_string()));
    let text = format!(
        "{} {}…\n\n{}",
        progress.spinner(),
        progress.title,
        t(lang, Key::ProgressHint)
    );
    f.render_widget(Paragraph::new(text).block(block), popup);
}

fn render_install_pick(f: &mut Frame, pick: &InstallPickState, lang: Lang, area: Rect) {
    let popup = centered_rect(74, 22, area);
    f.render_widget(Clear, popup);
    let block = bordered(theme::ACCENT).title(panel_title(t(lang, Key::InstallTitle).to_string()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    f.render_widget(Paragraph::new(format!("/{}", pick.filter)), rows[0]);

    let filtered = pick.filtered();
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|distro| ListItem::new(format!("{:<24} {}", distro.name, distro.friendly)))
        .collect();
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(theme::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
    let mut state = ListState::default();
    if !filtered.is_empty() {
        state.select(Some(pick.selected.min(filtered.len() - 1)));
    }
    f.render_stateful_widget(list, rows[1], &mut state);

    f.render_widget(Paragraph::new(t(lang, Key::InstallHint)), rows[2]);
}

fn render_import_pick(f: &mut Frame, pick: &ImportPickState, lang: Lang, area: Rect) {
    let popup = centered_rect(74, 22, area);
    f.render_widget(Clear, popup);
    let block =
        bordered(theme::ACCENT).title(panel_title(t(lang, Key::PickImportTitle).to_string()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    if pick.entries.is_empty() {
        f.render_widget(Paragraph::new(t(lang, Key::PickImportEmpty)), rows[0]);
    } else {
        let items: Vec<ListItem> = pick
            .entries
            .iter()
            .map(|a| ListItem::new(format!("{:<44} {}", a.name, human_size(a.size))))
            .collect();
        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(theme::SELECTION_BG)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▌ ");
        let mut state = ListState::default();
        state.select(Some(pick.selected.min(pick.entries.len() - 1)));
        f.render_stateful_widget(list, rows[0], &mut state);
    }

    f.render_widget(Paragraph::new(t(lang, Key::PickImportHints)), rows[1]);
}

fn render_confirm(f: &mut Frame, confirm: &Confirm, lang: Lang, area: Rect) {
    let mut lines: Vec<String> = confirm.prompt.lines().map(String::from).collect();
    if let Some(typed) = &confirm.require_typed {
        lines.push(String::new());
        lines.push(tf(
            lang,
            Key::ConfirmTypedLine,
            &[&typed.expected, &typed.input],
        ));
    }
    lines.push(String::new());
    lines.push(if confirm.require_typed.is_some() {
        t(lang, Key::ConfirmHintTyped).to_string()
    } else {
        t(lang, Key::ConfirmHintYesNo).to_string()
    });

    let height = lines.len() as u16 + 2;
    let popup = centered_rect(64, height, area);
    f.render_widget(Clear, popup);
    let block = bordered(Color::Yellow).title(panel_title(t(lang, Key::ConfirmTitle).to_string()));
    f.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(block)
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn render_error(f: &mut Frame, message: &str, lang: Lang, area: Rect) {
    let popup = centered_rect(64, 7, area);
    f.render_widget(Clear, popup);
    let block = bordered(Color::Red).title(panel_title(t(lang, Key::ErrorTitle).to_string()));
    f.render_widget(
        Paragraph::new(format!("{message}\n\n{}", t(lang, Key::ErrorDismiss)))
            .block(block)
            .wrap(Wrap { trim: true }),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Confirm, Modal, TypedConfirm};
    use crate::wsl::{Distro, DistroState};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(model: &Model, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| view(f, model)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

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

    fn line_text(line: &ratatui::text::Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn sample() -> Model {
        Model {
            distros: vec![Distro {
                name: "Debian".to_string(),
                state: DistroState::Running,
                version: 2,
                is_default: true,
                guid: None,
                base_path: None,
                vhd_path: None,
                disk_bytes: Some(4 * 1024 * 1024 * 1024),
                inner_disk: None,
            }],
            loaded: true,
            ..Default::default()
        }
    }

    #[test]
    fn renders_title_and_distro() {
        let rendered = render(&sample(), 110, 24);
        assert!(rendered.contains("WSL Manager"), "title missing");
        assert!(rendered.contains("Debian"), "distro name missing");
        assert!(rendered.contains("Running"), "state missing");
        assert!(rendered.contains("4.0 GB"), "disk size missing");
    }

    #[test]
    fn renders_confirm_modal() {
        let mut model = sample();
        model.modal = Some(Modal::Confirm(Confirm {
            prompt: "PERMANENTLY delete 'Debian'.".to_string(),
            require_typed: Some(TypedConfirm {
                expected: "Debian".to_string(),
                input: "Deb".to_string(),
            }),
            on_confirm: vec![],
            progress_title: None,
            status: None,
        }));
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Confirm"), "confirm title missing");
        assert!(rendered.contains("PERMANENTLY"), "prompt missing");
        assert!(rendered.contains("type"), "typed hint missing");
    }

    #[test]
    fn renders_detail_pane_with_vm_memory_and_cpu() {
        use crate::metrics::MetricsSample;
        use std::time::{Duration, Instant};
        let mut model = sample();
        let t0 = Instant::now();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 4,
        });
        // Second sample 1 s later: 2.0 CPU-seconds / (1 s × 4) = 50%.
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
            vm_cpu_100ns: Some(20_000_000),
            taken_at: Some(t0 + Duration::from_secs(1)),
            logical_cpus: 4,
        });
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Debian"), "detail title missing");
        assert!(rendered.contains("VM Mem"), "vm memory line missing");
        assert!(rendered.contains("2.0 GB"), "vm memory value missing");
        assert!(rendered.contains("VM CPU"), "vm cpu line missing");
        assert!(rendered.contains("50.0 %"), "vm cpu value missing");
        // The trend-row labels ("Mem"/"CPU") must render IN ADDITION to the
        // "VM Mem"/"VM CPU" info lines, so each short label appears at least
        // twice. A loose `contains` would pass on the info line alone and miss a
        // dropped trend row.
        assert!(
            rendered.matches("Mem").count() >= 2,
            "memory trend label missing (only the VM Mem info line present)"
        );
        assert!(
            rendered.matches("CPU").count() >= 2,
            "cpu trend label missing (only the VM CPU info line present)"
        );
    }

    #[test]
    fn detail_pane_survives_tiny_height() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(1024),
            total_mem_bytes: 2048,
            ..Default::default()
        });
        // A 6-row terminal squeezes the detail interior below two trend rows;
        // rendering must not panic.
        let _ = render(&model, 80, 6);
    }

    #[test]
    fn vm_memory_denominator_uses_wsl_vm_ram_when_known() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024, // host RAM
            ..Default::default()
        });
        model.vm_mem_total = Some(4 * 1024 * 1024 * 1024); // WSL VM ceiling
        let rendered = render(&model, 110, 24);
        // Denominator is the VM's RAM (4 GB), not the host's (8 GB).
        assert!(
            rendered.contains("2.0 GB / 4.0 GB"),
            "VM RAM should be the denominator"
        );
    }

    #[test]
    fn renders_export_form_modal() {
        use crate::app::FormState;
        let mut model = sample();
        model.modal = Some(Modal::Form(FormState::export(
            "Debian".to_string(),
            "Debian.tar".to_string(),
        )));
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Export"), "form title missing");
        assert!(rendered.contains("Debian.tar"), "default path missing");
    }

    #[test]
    fn renders_progress_modal() {
        use crate::app::ProgressState;
        let mut model = sample();
        model.modal = Some(Modal::Progress(ProgressState::new(
            "Exporting 'Debian'".to_string(),
        )));
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Working"), "progress title missing");
        assert!(rendered.contains("Exporting 'Debian'"), "op label missing");
    }

    #[test]
    fn renders_install_pick_modal() {
        use crate::app::InstallPickState;
        use crate::wsl::OnlineDistro;
        let mut model = sample();
        model.modal = Some(Modal::InstallPick(InstallPickState::new(vec![
            OnlineDistro {
                name: "Ubuntu".to_string(),
                friendly: "Ubuntu".to_string(),
            },
            OnlineDistro {
                name: "Debian".to_string(),
                friendly: "Debian GNU/Linux".to_string(),
            },
        ])));
        let rendered = render(&model, 110, 28);
        assert!(rendered.contains("Install"), "picker title missing");
        assert!(rendered.contains("Ubuntu"), "Ubuntu missing");
        assert!(rendered.contains("Debian"), "Debian missing");
    }

    #[test]
    fn renders_import_pick_modal() {
        use crate::app::ImportPickState;
        let mut model = sample();
        model.modal = Some(Modal::ImportPick(ImportPickState::new(vec![
            crate::manage::Archive {
                name: "Ubuntu-20260607.tar.gz".into(),
                path: std::path::PathBuf::from(r"C:\wsl\exports\Ubuntu-20260607.tar.gz"),
                size: 1024,
            },
        ])));
        let rendered = render(&model, 100, 30);
        assert!(
            rendered.contains("Ubuntu-20260607.tar.gz"),
            "picker should list the archive"
        );
    }

    #[test]
    fn renders_config_editor_form() {
        use crate::app::ConfigEditState;
        use crate::config::ConfigTarget;
        let mut model = sample();
        model.modal = Some(Modal::ConfigEdit(ConfigEditState::new(
            ConfigTarget::WslConfig,
            "[wsl2]\nmemory=8GB\n",
        )));
        let rendered = render(&model, 110, 30);
        assert!(rendered.contains("Edit"), "editor title missing");
        assert!(rendered.contains("memory"), "known key missing");
        assert!(rendered.contains("8GB"), "value missing");
        assert!(rendered.contains("Form"), "mode indicator missing");
    }

    #[test]
    fn renders_in_japanese_when_lang_is_ja() {
        let mut model = sample();
        model.lang = Lang::Ja;
        let rendered = render(&model, 110, 24);
        // Wide (CJK) glyphs occupy two terminal cells; the continuation cell
        // breaks up multi-char substrings in the test buffer, so we match the
        // leading glyph of each translated string instead.
        assert!(rendered.contains('名'), "JA column header missing"); // 名前 (NAME)
        assert!(rendered.contains('実'), "JA running state missing"); // 実行中 (Running)
        assert!(rendered.contains("JA"), "language indicator missing");
    }

    #[test]
    fn modals_localize_to_japanese() {
        let mut model = sample();
        model.lang = Lang::Ja;
        model.modal = Some(Modal::Help);
        let rendered = render(&model, 90, 30);
        assert!(rendered.contains('ヘ'), "JA help title missing"); // ヘルプ
        assert!(rendered.contains('移'), "JA help body missing"); // 移動 (move)
    }

    #[test]
    fn renders_help_overlay() {
        let mut model = sample();
        model.modal = Some(Modal::Help);
        let rendered = render(&model, 90, 30);
        assert!(rendered.contains("Help"), "help title missing");
        assert!(rendered.contains("quit"), "keybinding text missing");
    }

    #[test]
    fn modals_use_rounded_borders() {
        let mut model = sample();
        model.modal = Some(Modal::Help);
        let buf = render_buf(&model, 90, 30);
        // すべてのパネル（テーブル/詳細/モーダル）が丸枠。角枠 "┌" は残らない。
        assert!(
            !buf.content.iter().any(|c| c.symbol() == "┌"),
            "modal should use rounded corners (no square ┌ anywhere)"
        );
        assert!(
            buf.content.iter().any(|c| c.symbol() == "╭"),
            "rounded corners expected"
        );
    }

    #[test]
    fn table_shows_only_filtered_rows() {
        let model = Model {
            distros: vec![distro_named("Debian"), distro_named("Ubuntu")],
            filter: "ubu".to_string(),
            loaded: true,
            ..Default::default()
        };
        let rendered = render(&model, 90, 24);
        assert!(rendered.contains("Ubuntu"), "matching row missing");
        assert!(!rendered.contains("Debian"), "filtered-out row present");
    }

    #[test]
    fn detail_memory_gauge_is_colored_by_usage() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(1024 * 1024 * 1024),   // 1 GB used
            total_mem_bytes: 8 * 1024 * 1024 * 1024, // of 8 GB → 12.5%
            ..Default::default()
        });
        let buf = render_buf(&model, 110, 24);
        assert_eq!(fg_of(&buf, "█"), Some(super::theme::ACCENT));
        assert!(buf.content.iter().any(|c| c.symbol() == "▕"));
    }

    fn distro_named(name: &str) -> Distro {
        Distro {
            name: name.to_string(),
            state: DistroState::Stopped,
            version: 2,
            is_default: false,
            guid: None,
            base_path: None,
            vhd_path: None,
            disk_bytes: None,
            inner_disk: None,
        }
    }

    #[test]
    fn table_running_dot_is_green_when_not_selected() {
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
        assert_eq!(bg_of(&buf, "▌"), Some(super::theme::SELECTION_BG));
    }

    #[test]
    fn idle_status_shows_global_hints() {
        let buf = render_buf(&sample(), 110, 24);
        let dump: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(dump.contains("move"), "global hint 'move' missing");
        assert!(dump.contains("quit"), "global hint 'quit' missing");
    }

    #[test]
    fn footer_row_shows_context_hints() {
        let buf = render_buf(&sample(), 110, 24); // Running 選択
        let dump: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(dump.contains("shell"), "footer 'shell' hint missing");
        assert!(
            dump.contains("stop"),
            "footer 'stop' hint missing (running)"
        );
    }

    #[test]
    fn footer_context_running_offers_stop_not_start() {
        let model = sample(); // Debian = Running, 選択中
        let text = line_text(&super::footer::context_line(&model));
        assert!(text.contains("stop"), "running footer should offer stop");
        assert!(
            !text.contains("start"),
            "running footer must not offer start"
        );
        assert!(
            text.contains("tab"),
            "running footer should offer new-tab shell"
        );
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
        assert!(
            text.contains("remove"),
            "stopped footer should offer remove"
        );
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
}
