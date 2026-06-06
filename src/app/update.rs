//! The pure update reducer: `(Model, Action) -> Vec<Command>`. Terminal- and
//! IO-free, so it is unit-tested headlessly. Side effects are described by the
//! returned [`Command`]s, which the runtime executes.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Command, Confirm, Event, LifecycleOp, Modal, Model, TypedConfirm};

/// Advance the model in response to an action, returning any side effects.
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
        Action::OpDone(message) => {
            model.status = Some(message);
            vec![Command::RefreshList]
        }
        Action::OpFailed(message) => {
            model.modal = Some(Modal::Error { message });
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
    if model.modal.is_some() {
        handle_modal_key(model, key)
    } else {
        handle_list_key(model, key)
    }
}

fn handle_list_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => model.should_quit = true,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => model.should_quit = true,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => model.select_next(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => model.select_prev(),
        (KeyCode::Char('r'), _) => return vec![Command::RefreshList],
        (KeyCode::Char('s'), _) => return start_selected(model),
        (KeyCode::Char('d'), _) => return set_default_selected(model),
        (KeyCode::Char('x'), _) => open_confirm_terminate(model),
        (KeyCode::Char('X'), _) => open_confirm_shutdown(model),
        (KeyCode::Char('u'), _) => open_confirm_unregister(model),
        _ => {}
    }
    vec![]
}

fn selected_name(model: &Model) -> Option<String> {
    model.selected_distro().map(|distro| distro.name.clone())
}

fn start_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.status = Some(format!("Starting {name}…"));
    vec![Command::Lifecycle(LifecycleOp::Start(name))]
}

fn set_default_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.status = Some(format!("Setting {name} as default…"));
    vec![Command::Lifecycle(LifecycleOp::SetDefault(name))]
}

fn open_confirm_terminate(model: &mut Model) {
    let Some(name) = selected_name(model) else {
        return;
    };
    model.modal = Some(Modal::Confirm(Confirm {
        op: LifecycleOp::Terminate(name.clone()),
        prompt: format!("Terminate (stop) '{name}'?"),
        require_typed: None,
    }));
}

fn open_confirm_shutdown(model: &mut Model) {
    model.modal = Some(Modal::Confirm(Confirm {
        op: LifecycleOp::Shutdown,
        prompt: "Shut down ALL running WSL distributions?".to_string(),
        require_typed: None,
    }));
}

fn open_confirm_unregister(model: &mut Model) {
    let Some(name) = selected_name(model) else {
        return;
    };
    model.modal = Some(Modal::Confirm(Confirm {
        op: LifecycleOp::Unregister(name.clone()),
        prompt: format!("PERMANENTLY delete '{name}' and ALL its data."),
        require_typed: Some(TypedConfirm {
            expected: name,
            input: String::new(),
        }),
    }));
}

fn handle_modal_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    let Some(modal) = model.modal.take() else {
        return vec![];
    };
    match modal {
        // Any key dismisses an error (the modal was already taken above).
        Modal::Error { .. } => vec![],
        Modal::Confirm(confirm) => handle_confirm_key(model, confirm, key),
    }
}

fn handle_confirm_key(model: &mut Model, mut confirm: Confirm, key: KeyEvent) -> Vec<Command> {
    match key.code {
        // Cancelled: the modal was already taken, so leave it closed.
        KeyCode::Esc => vec![],
        KeyCode::Enter => {
            if confirm
                .require_typed
                .as_ref()
                .is_none_or(TypedConfirm::matches)
            {
                confirm_op(model, confirm.op)
            } else {
                // Typed name does not match: keep the dialog open.
                model.modal = Some(Modal::Confirm(confirm));
                vec![]
            }
        }
        KeyCode::Char(c) => {
            if let Some(typed) = confirm.require_typed.as_mut() {
                typed.input.push(c);
                model.modal = Some(Modal::Confirm(confirm));
                vec![]
            } else if matches!(c, 'y' | 'Y') {
                confirm_op(model, confirm.op)
            } else if matches!(c, 'n' | 'N') {
                vec![] // cancelled
            } else {
                model.modal = Some(Modal::Confirm(confirm));
                vec![]
            }
        }
        KeyCode::Backspace => {
            if let Some(typed) = confirm.require_typed.as_mut() {
                typed.input.pop();
            }
            model.modal = Some(Modal::Confirm(confirm));
            vec![]
        }
        _ => {
            model.modal = Some(Modal::Confirm(confirm));
            vec![]
        }
    }
}

fn confirm_op(model: &mut Model, op: LifecycleOp) -> Vec<Command> {
    model.status = Some(format!("{}…", op.verb()));
    vec![Command::Lifecycle(op)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsl::{Distro, DistroState};

    fn key(code: KeyCode, mods: KeyModifiers) -> Action {
        Action::Event(Event::Key(KeyEvent::new(code, mods)))
    }

    fn ch(c: char) -> Action {
        key(KeyCode::Char(c), KeyModifiers::NONE)
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

    fn model_with(names: &[&str]) -> Model {
        let mut m = Model::default();
        update(
            &mut m,
            Action::Refreshed(names.iter().map(|n| distro(n)).collect()),
        );
        m
    }

    #[test]
    fn q_quits() {
        let mut m = Model::default();
        assert!(update(&mut m, ch('q')).is_empty());
        assert!(m.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(m.should_quit);
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
        assert_eq!(update(&mut m, ch('r')), vec![Command::RefreshList]);
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut m = model_with(&["a", "b"]);
        assert_eq!(m.selected, 0);
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 1);
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 1);
        update(&mut m, key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(m.selected, 0);
    }

    #[test]
    fn start_dispatches_immediately() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('s'));
        assert_eq!(
            cmds,
            vec![Command::Lifecycle(LifecycleOp::Start("Debian".into()))]
        );
        assert!(m.modal.is_none());
    }

    #[test]
    fn set_default_dispatches_immediately() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('d'));
        assert_eq!(
            cmds,
            vec![Command::Lifecycle(LifecycleOp::SetDefault("Debian".into()))]
        );
    }

    #[test]
    fn terminate_requires_confirmation() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('x'));
        assert!(cmds.is_empty(), "no command before confirmation");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        // Enter confirms and dispatches.
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Lifecycle(LifecycleOp::Terminate("Debian".into()))]
        );
        assert!(m.modal.is_none());
    }

    #[test]
    fn confirm_can_be_cancelled_with_esc() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('x'));
        let cmds = update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(cmds.is_empty());
        assert!(m.modal.is_none());
    }

    #[test]
    fn shutdown_confirmation_accepts_y() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, key(KeyCode::Char('X'), KeyModifiers::SHIFT));
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        let cmds = update(&mut m, ch('y'));
        assert_eq!(cmds, vec![Command::Lifecycle(LifecycleOp::Shutdown)]);
    }

    #[test]
    fn unregister_requires_typed_name() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('u'));
        // Enter with empty input does nothing and keeps the dialog open.
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty());
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        // Type the name, then Enter confirms.
        for c in "Debian".chars() {
            update(&mut m, ch(c));
        }
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Lifecycle(LifecycleOp::Unregister("Debian".into()))]
        );
    }

    #[test]
    fn keys_are_consumed_by_open_modal() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('x')); // opens confirm
                                 // 'q' should not quit while a modal is open.
        update(&mut m, ch('q'));
        assert!(!m.should_quit);
    }

    #[test]
    fn op_done_sets_status_and_refreshes() {
        let mut m = Model::default();
        let cmds = update(&mut m, Action::OpDone("Terminated Debian".into()));
        assert_eq!(cmds, vec![Command::RefreshList]);
        assert_eq!(m.status.as_deref(), Some("Terminated Debian"));
    }

    #[test]
    fn op_failed_opens_error_modal() {
        let mut m = Model::default();
        update(&mut m, Action::OpFailed("boom".into()));
        assert!(matches!(m.modal, Some(Modal::Error { .. })));
        // Any key dismisses it.
        update(&mut m, ch(' '));
        assert!(m.modal.is_none());
    }

    #[test]
    fn refreshed_clears_error_and_clamps() {
        let mut m = Model {
            last_error: Some("boom".to_string()),
            ..Default::default()
        };
        update(&mut m, Action::Refreshed(vec![distro("a")]));
        assert!(m.last_error.is_none());
        assert!(m.loaded);
        assert_eq!(m.selected, 0);
    }
}
