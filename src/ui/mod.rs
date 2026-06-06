//! View layer: pure rendering of the [`Model`] into ratatui widgets. Renders
//! only; never mutates state. (M2: the distro table and a status line; the
//! detail pane, modals and help arrive in later milestones.)

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::Model;
use crate::wsl::{Distro, DistroState};

/// Render the whole UI for the current model.
pub fn view(f: &mut Frame, model: &Model) {
    let area = f.area();
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    render_table(f, model, chunks[0]);
    render_status(f, model, chunks[1]);
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
    } else if !model.loaded {
        ("loading…".to_string(), Style::default().fg(Color::DarkGray))
    } else {
        (
            format!(
                "{} distro(s) · ↑/↓ or j/k: move · r: refresh · q: quit",
                model.distros.len()
            ),
            Style::default(),
        )
    };
    f.render_widget(Paragraph::new(text).style(style), area);
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
    use crate::wsl::{Distro, DistroState};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

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
        let mut terminal = Terminal::new(TestBackend::new(72, 10)).unwrap();
        let model = sample();
        terminal.draw(|f| view(f, &model)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(rendered.contains("WSL Manager"), "title missing");
        assert!(rendered.contains("Debian"), "distro name missing");
        assert!(rendered.contains("Running"), "state missing");
        assert!(rendered.contains("4.0 GB"), "disk size missing");
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GB");
        assert_eq!(human_size(1536), "1.5 KB");
    }
}
