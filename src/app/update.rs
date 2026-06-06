//! The pure update reducer: `(Model, Action) -> Vec<Command>`. Terminal- and
//! IO-free, so it is unit-tested headlessly. Side effects are described by the
//! returned [`Command`]s, which the runtime executes.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Command, Event, Model};

/// Advance the model in response to an action, returning any side effects to
/// run.
pub fn update(model: &mut Model, action: Action) -> Vec<Command> {
    match action {
        Action::Quit => {
            model.should_quit = true;
            vec![]
        }
        Action::Refreshed(distros) => {
            model.distros = distros;
            model.loaded = true;
            model.last_error = None;
            model.clamp_selection();
            vec![]
        }
        Action::RefreshFailed(message) => {
            model.loaded = true;
            model.last_error = Some(message);
            vec![]
        }
        Action::Event(event) => update_event(model, event),
    }
}

fn update_event(model: &mut Model, event: Event) -> Vec<Command> {
    match event {
        Event::Key(key) => handle_key(model, key),
        Event::Tick => {
            model.ticks = model.ticks.wrapping_add(1);
            vec![Command::RefreshList]
        }
        Event::Resize(_, _) => vec![],
    }
}

fn handle_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => model.should_quit = true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => model.should_quit = true,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => model.select_next(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => model.select_prev(),
        (KeyCode::Char('r'), _) => return vec![Command::RefreshList],
        _ => {}
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsl::{Distro, DistroState};

    fn key(code: KeyCode, mods: KeyModifiers) -> Action {
        Action::Event(Event::Key(KeyEvent::new(code, mods)))
    }

    fn distro(name: &str) -> Distro {
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

    #[test]
    fn q_quits() {
        let mut m = Model::default();
        let cmds = update(&mut m, key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(m.should_quit);
        assert!(cmds.is_empty());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(m.should_quit);
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(!m.should_quit);
    }

    #[test]
    fn tick_polls_refresh() {
        let mut m = Model::default();
        let cmds = update(&mut m, Action::Event(Event::Tick));
        assert_eq!(cmds, vec![Command::RefreshList]);
        assert_eq!(m.ticks, 1);
    }

    #[test]
    fn r_requests_refresh() {
        let mut m = Model::default();
        let cmds = update(&mut m, key(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(cmds, vec![Command::RefreshList]);
    }

    #[test]
    fn refreshed_sets_distros_and_clears_error() {
        let mut m = Model {
            last_error: Some("boom".to_string()),
            ..Default::default()
        };
        update(
            &mut m,
            Action::Refreshed(vec![distro("Debian"), distro("Ubuntu")]),
        );
        assert_eq!(m.distros.len(), 2);
        assert!(m.last_error.is_none());
        assert!(m.loaded);
    }

    #[test]
    fn refresh_failed_records_error() {
        let mut m = Model::default();
        update(&mut m, Action::RefreshFailed("nope".to_string()));
        assert_eq!(m.last_error.as_deref(), Some("nope"));
        assert!(m.loaded);
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut m = Model::default();
        update(&mut m, Action::Refreshed(vec![distro("a"), distro("b")]));
        assert_eq!(m.selected, 0);
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 1);
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE)); // clamp at last
        assert_eq!(m.selected, 1);
        update(&mut m, key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(m.selected, 0);
        update(&mut m, key(KeyCode::Up, KeyModifiers::NONE)); // clamp at first
        assert_eq!(m.selected, 0);
    }

    #[test]
    fn refreshed_clamps_selection_when_list_shrinks() {
        let mut m = Model::default();
        update(
            &mut m,
            Action::Refreshed(vec![distro("a"), distro("b"), distro("c")]),
        );
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 2);
        update(&mut m, Action::Refreshed(vec![distro("a")]));
        assert_eq!(m.selected, 0);
    }
}
