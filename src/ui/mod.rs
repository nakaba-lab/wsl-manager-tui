//! View layer: pure rendering of the [`Model`] into ratatui widgets. Renders
//! only; never mutates state. Covers the distro table, the detail pane with a
//! VM-memory sparkline, the status line, and all modals (confirm, error, form,
//! progress, install picker).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline, Table,
    TableState, Wrap,
};
use ratatui::Frame;

use crate::app::{
    ConfigEditState, Confirm, EditMode, FormKind, FormState, InstallPickState, Modal, Model,
    ProgressState,
};
use crate::i18n::{t, Key, Lang};
use crate::metrics::MetricsHistory;
use crate::wsl::{Distro, DistroState};

/// Render the whole UI for the current model.
pub fn view(f: &mut Frame, model: &Model) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(8),
        Constraint::Length(1),
    ])
    .split(area);
    render_table(f, model, chunks[0]);
    render_detail(f, model, chunks[1]);
    render_status(f, model, chunks[2]);
    if let Some(modal) = &model.modal {
        render_modal(f, modal, model.lang, area);
    }
}

fn render_detail(f: &mut Frame, model: &Model, area: Rect) {
    let lang = model.lang;
    let title = match model.selected_distro() {
        Some(distro) => format!(" Detail: {} ", distro.name),
        None => " Detail ".to_string(),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(distro) = model.selected_distro() else {
        f.render_widget(Paragraph::new(t(lang, Key::NoDistros)), inner);
        return;
    };

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    let path = distro
        .base_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "—".to_string());
    let disk = distro
        .disk_bytes
        .map(human_size)
        .unwrap_or_else(|| "—".to_string());
    let default = if distro.is_default { "★" } else { "—" };
    let info = format!(
        "{}: {}\n{}: {}    {}: {}\n{}: {}\n{}: {}\n{}: {}",
        t(lang, Key::DetailState),
        state_label(lang, distro.state),
        t(lang, Key::DetailVersion),
        distro.version,
        t(lang, Key::DetailDefault),
        default,
        t(lang, Key::DetailDisk),
        disk,
        t(lang, Key::DetailPath),
        path,
        t(lang, Key::DetailVmMem),
        vm_mem_line(lang, &model.metrics),
    );
    f.render_widget(Paragraph::new(info), rows[0]);

    let data = model.metrics.sparkline();
    let sparkline = Sparkline::default()
        .data(data.as_slice())
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sparkline, rows[1]);
}

fn vm_mem_line(lang: Lang, metrics: &MetricsHistory) -> String {
    let note = t(lang, Key::VmSharedNote);
    match metrics.latest_vmmem {
        Some(used) if metrics.total_mem_bytes > 0 => format!(
            "{} / {} {note}",
            human_size(used),
            human_size(metrics.total_mem_bytes)
        ),
        Some(used) => format!("{} {note}", human_size(used)),
        None => t(lang, Key::VmNotRunning).to_string(),
    }
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
    .style(Style::default().add_modifier(Modifier::BOLD));
    let visible = model.visible_distros();
    let rows = visible.iter().map(|&distro| distro_row(lang, distro));
    let widths = [
        Constraint::Min(16),
        Constraint::Length(12),
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(12),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" WSL Manager (wslm) · {} ", lang.label())),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if !visible.is_empty() {
        state.select(Some(model.selected.min(visible.len() - 1)));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn distro_row(lang: Lang, distro: &Distro) -> Row<'static> {
    let state = format!(
        "{} {}",
        distro.state.glyph(),
        state_label(lang, distro.state)
    );
    let default = if distro.is_default { "★" } else { "" };
    let disk = distro
        .disk_bytes
        .map(human_size)
        .unwrap_or_else(|| "—".to_string());
    Row::new(vec![
        Cell::from(distro.name.clone()),
        Cell::from(state),
        Cell::from(distro.version.to_string()),
        Cell::from(default),
        Cell::from(disk),
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

fn render_status(f: &mut Frame, model: &Model, area: Rect) {
    let lang = model.lang;
    if model.filter_mode {
        let prompt = format!("/{}▏", model.filter);
        let widget = Paragraph::new(prompt).style(Style::default().fg(Color::Yellow));
        f.render_widget(widget, area);
        return;
    }
    let (text, style) = if !model.filter.is_empty() {
        (
            format!("filter: {} · Esc clears", model.filter),
            Style::default().fg(Color::Yellow),
        )
    } else if let Some(error) = &model.last_error {
        (
            format!("{}: {error}", t(lang, Key::ErrorPrefix)),
            Style::default().fg(Color::Red),
        )
    } else if let Some(status) = &model.status {
        (status.clone(), Style::default().fg(Color::Green))
    } else if !model.loaded {
        (
            t(lang, Key::Loading).to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (t(lang, Key::StatusHint).to_string(), Style::default())
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}

fn render_modal(f: &mut Frame, modal: &Modal, lang: Lang, area: Rect) {
    match modal {
        Modal::Confirm(confirm) => render_confirm(f, confirm, lang, area),
        Modal::Error { message } => render_error(f, message, lang, area),
        Modal::Form(form) => render_form(f, form, area),
        Modal::Progress(progress) => render_progress(f, progress, lang, area),
        Modal::InstallPick(pick) => render_install_pick(f, pick, lang, area),
        Modal::ConfigEdit(state) => render_config_edit(f, state, lang, area),
        Modal::Help => render_help(f, area),
        Modal::Quit => render_quit(f, area),
    }
}

fn render_help(f: &mut Frame, area: Rect) {
    let popup = centered_rect(66, 24, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help — keybindings ");
    let text = "\
 j/k · ↑/↓     move selection
 /             filter list (Esc clears)
 Enter         inline shell (exit returns to wslm)
 w             shell in a new Windows Terminal tab
 s             start (boot) the distro
 x             stop (terminate) the distro
 X             shut down the whole WSL VM
 d             set as default
 u             unregister — delete (type name to confirm)
 e             export to a .tar backup
 m             import from a .tar
 i             install from the online catalog
 c / C         edit .wslconfig / wsl.conf
 L             toggle English / Japanese
 r             refresh now
 ?             this help
 q             quit  (Ctrl+C forces quit)

 Press any key to close.";
    f.render_widget(Paragraph::new(text).block(block), popup);
}

fn render_quit(f: &mut Frame, area: Rect) {
    let popup = centered_rect(44, 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Quit ")
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(
        Paragraph::new("Quit wslm?\n\nEnter / y: quit · Esc / n: stay").block(block),
        popup,
    );
}

fn render_config_edit(f: &mut Frame, state: &ConfigEditState, lang: Lang, area: Rect) {
    let popup = centered_rect(82, 26, area);
    f.render_widget(Clear, popup);
    let mode = match state.mode {
        EditMode::Form => "Form",
        EditMode::Raw => "Raw",
    };
    let block = Block::default().borders(Borders::ALL).title(format!(
        " Edit {} [{}] ",
        state.target.label(),
        mode
    ));
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
        let marker = if i == state.focus { "▶ " } else { "  " };
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

fn render_form(f: &mut Frame, form: &FormState, area: Rect) {
    let title = match &form.kind {
        FormKind::Export { .. } => " Export distribution ",
        FormKind::Import => " Import distribution ",
    };
    let popup = centered_rect(72, form.fields.len() as u16 * 2 + 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut text = String::new();
    for (i, label) in form.labels.iter().enumerate() {
        let marker = if i == form.focus { "▶ " } else { "  " };
        let cursor = if i == form.focus { "▏" } else { "" };
        text.push_str(&format!(
            "{marker}{label}:\n    {}{cursor}\n",
            form.fields[i].value
        ));
    }
    text.push_str("\nTab / ↑↓: move · Enter: submit · Esc: cancel");
    f.render_widget(Paragraph::new(text), inner);
}

fn render_progress(f: &mut Frame, progress: &ProgressState, lang: Lang, area: Rect) {
    let popup = centered_rect(60, 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Working ")
        .border_style(Style::default().fg(Color::Cyan));
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::InstallTitle));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    f.render_widget(Paragraph::new(format!("filter: {}", pick.filter)), rows[0]);

    let filtered = pick.filtered();
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|distro| ListItem::new(format!("{:<24} {}", distro.name, distro.friendly)))
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if !filtered.is_empty() {
        state.select(Some(pick.selected.min(filtered.len() - 1)));
    }
    f.render_stateful_widget(list, rows[1], &mut state);

    f.render_widget(Paragraph::new(t(lang, Key::InstallHint)), rows[2]);
}

fn render_confirm(f: &mut Frame, confirm: &Confirm, lang: Lang, area: Rect) {
    let mut lines: Vec<String> = confirm.prompt.lines().map(String::from).collect();
    if let Some(typed) = &confirm.require_typed {
        lines.push(String::new());
        lines.push(format!(
            "type \"{}\" to confirm: {}",
            typed.expected, typed.input
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm ")
        .border_style(Style::default().fg(Color::Yellow));
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Error ")
        .border_style(Style::default().fg(Color::Red));
    f.render_widget(
        Paragraph::new(format!("{message}\n\n{}", t(lang, Key::ErrorDismiss)))
            .block(block)
            .wrap(Wrap { trim: true }),
        popup,
    );
}

/// A centered rectangle of the given size, clamped to `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Human-readable byte size using binary units.
fn human_size(bytes: u64) -> String {
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
    use crate::app::{Confirm, LifecycleOp, Modal, TypedConfirm};
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
            op: LifecycleOp::Unregister("Debian".to_string()),
            prompt: "PERMANENTLY delete 'Debian'.".to_string(),
            require_typed: Some(TypedConfirm {
                expected: "Debian".to_string(),
                input: "Deb".to_string(),
            }),
        }));
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Confirm"), "confirm title missing");
        assert!(rendered.contains("PERMANENTLY"), "prompt missing");
        assert!(rendered.contains("type"), "typed hint missing");
    }

    #[test]
    fn renders_detail_pane_with_vm_memory() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
        });
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Detail: Debian"), "detail title missing");
        assert!(rendered.contains("VM Mem"), "vm memory line missing");
        assert!(rendered.contains("2.0 GB"), "vm memory value missing");
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
    fn renders_help_overlay() {
        let mut model = sample();
        model.modal = Some(Modal::Help);
        let rendered = render(&model, 90, 30);
        assert!(rendered.contains("Help"), "help title missing");
        assert!(rendered.contains("quit"), "keybinding text missing");
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
    fn human_size_formats() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GB");
        assert_eq!(human_size(1536), "1.5 KB");
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
        }
    }
}
