//! The pure update reducer. Terminal- and IO-free, so it is unit-tested
//! headlessly. From M2 it will also return a list of side-effecting commands
//! for the runtime to execute; for now it just mutates the model in place.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Event, Model};

/// Advance the model in response to an action.
pub fn update(model: &mut Model, action: Action) {
    match action {
        Action::Quit => model.should_quit = true,
        Action::Event(event) => update_event(model, event),
    }
}

fn update_event(model: &mut Model, event: Event) {
    match event {
        Event::Key(key) => handle_key(model, key),
        Event::Tick => model.ticks = model.ticks.wrapping_add(1),
        Event::Resize(_, _) => {}
    }
}

fn handle_key(model: &mut Model, key: KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => model.should_quit = true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => model.should_quit = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> Action {
        Action::Event(Event::Key(KeyEvent::new(code, mods)))
    }

    #[test]
    fn q_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(m.should_quit);
    }

    #[test]
    fn esc_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(m.should_quit);
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
    fn tick_increments() {
        let mut m = Model::default();
        update(&mut m, Action::Event(Event::Tick));
        update(&mut m, Action::Event(Event::Tick));
        assert_eq!(m.ticks, 2);
    }

    #[test]
    fn quit_action_sets_flag() {
        let mut m = Model::default();
        update(&mut m, Action::Quit);
        assert!(m.should_quit);
    }
}
