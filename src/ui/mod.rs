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

use crate::app::{Confirm, FormKind, FormState, InstallPickState, Modal, Model, ProgressState};
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
        render_modal(f, modal, area);
    }
}

fn render_detail(f: &mut Frame, model: &Model, area: Rect) {
    let title = match model.selected_distro() {
        Some(distro) => format!(" Detail: {} ", distro.name),
        None => " Detail ".to_string(),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(distro) = model.selected_distro() else {
        f.render_widget(Paragraph::new("No distributions."), inner);
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
    let default = if distro.is_default { "yes" } else { "no" };
    let info = format!(
        "State:   {}\nVersion: {}    Default: {}\nDisk:    {}\nPath:    {}\nVM Mem:  {}",
        state_label(distro.state),
        distro.version,
        default,
        disk,
        path,
        vm_mem_line(&model.metrics),
    );
    f.render_widget(Paragraph::new(info), rows[0]);

    let data = model.metrics.sparkline();
    let sparkline = Sparkline::default()
        .data(data.as_slice())
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sparkline, rows[1]);
}

fn vm_mem_line(metrics: &MetricsHistory) -> String {
    match metrics.latest_vmmem {
        Some(used) if metrics.total_mem_bytes > 0 => format!(
            "{} / {} (vmmemWSL, shared by all distros)",
            human_size(used),
            human_size(metrics.total_mem_bytes)
        ),
        Some(used) => format!("{} (vmmemWSL, shared by all distros)", human_size(used)),
        None => "— (WSL VM not running)".to_string(),
    }
}

fn render_table(f: &mut Frame, model: &Model, area: Rect) {
    let header = Row::new(["NAME", "STATE", "VER", "DEFAULT", "DISK"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = model.distros.iter().map(distro_row);
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
                .title(" WSL Manager (wslm) "),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if !model.distros.is_empty() {
        state.select(Some(model.selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn distro_row(distro: &Distro) -> Row<'static> {
    let state = format!("{} {}", distro.state.glyph(), state_label(distro.state));
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

fn state_label(state: DistroState) -> &'static str {
    match state {
        DistroState::Running => "Running",
        DistroState::Stopped => "Stopped",
        DistroState::Installing => "Installing",
        DistroState::Unknown => "Unknown",
    }
}

fn render_status(f: &mut Frame, model: &Model, area: Rect) {
    let (text, style) = if let Some(error) = &model.last_error {
        (format!("error: {error}"), Style::default().fg(Color::Red))
    } else if let Some(status) = &model.status {
        (status.clone(), Style::default().fg(Color::Green))
    } else if !model.loaded {
        ("loading…".to_string(), Style::default().fg(Color::DarkGray))
    } else {
        (
            format!(
                "{} distro(s) · j/k move · Enter shell · w tab · s start · x stop · X shutdown · d default · u unreg · r refresh · q quit",
                model.distros.len()
            ),
            Style::default(),
        )
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}

fn render_modal(f: &mut Frame, modal: &Modal, area: Rect) {
    match modal {
        Modal::Confirm(confirm) => render_confirm(f, confirm, area),
        Modal::Error { message } => render_error(f, message, area),
        Modal::Form(form) => render_form(f, form, area),
        Modal::Progress(progress) => render_progress(f, progress, area),
        Modal::InstallPick(pick) => render_install_pick(f, pick, area),
    }
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

fn render_progress(f: &mut Frame, progress: &ProgressState, area: Rect) {
    let popup = centered_rect(60, 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Working ")
        .border_style(Style::default().fg(Color::Cyan));
    let text = format!(
        "{} {}…\n\nThis may take a while. Esc to cancel.",
        progress.spinner(),
        progress.title
    );
    f.render_widget(Paragraph::new(text).block(block), popup);
}

fn render_install_pick(f: &mut Frame, pick: &InstallPickState, area: Rect) {
    let popup = centered_rect(74, 22, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Install — select a distribution ");
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

    f.render_widget(
        Paragraph::new("type to filter · ↑/↓ select · Enter install · Esc cancel"),
        rows[2],
    );
}

fn render_confirm(f: &mut Frame, confirm: &Confirm, area: Rect) {
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
        "Enter: confirm (must match) · Esc: cancel".to_string()
    } else {
        "Enter / y: confirm · Esc / n: cancel".to_string()
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

fn render_error(f: &mut Frame, message: &str, area: Rect) {
    let popup = centered_rect(64, 7, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Error ")
        .border_style(Style::default().fg(Color::Red));
    f.render_widget(
        Paragraph::new(format!("{message}\n\nPress any key to dismiss."))
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
    fn human_size_formats() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GB");
        assert_eq!(human_size(1536), "1.5 KB");
    }
}
