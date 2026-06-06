//! View layer: pure rendering of the [`Model`] into ratatui widgets. Renders
//! only; never mutates state. (M1: placeholder skeleton; the table, detail
//! pane, modals and help arrive in later milestones.)

use ratatui::layout::Alignment;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::Model;

/// Render the whole UI for the current model.
pub fn view(f: &mut Frame, model: &Model) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" WSL Manager (wslm) ");
    let body = format!(
        "Skeleton runtime is live.\n\nticks: {}\n\nPress q / Esc / Ctrl-C to quit.",
        model.ticks
    );
    let paragraph = Paragraph::new(body).block(block).alignment(Alignment::Left);
    let area = f.area();
    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Render the view into an in-memory buffer (no real terminal) and assert
    /// the title and live tick counter are present.
    #[test]
    fn view_renders_title_and_ticks() {
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
        let model = Model {
            ticks: 7,
            ..Default::default()
        };
        terminal.draw(|f| view(f, &model)).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(
            rendered.contains("WSL Manager"),
            "title missing: {rendered}"
        );
        assert!(rendered.contains("ticks: 7"), "tick counter missing");
    }
}
