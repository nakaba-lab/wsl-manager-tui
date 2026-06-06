//! The pure update reducer: `(Model, Action) -> Vec<Command>`. Terminal- and
//! IO-free, so it is unit-tested headlessly. Side effects are described by the
//! returned [`Command`]s, which the runtime executes.

use std::path::PathBuf;

use super::input::{KeyCode, KeyMods as KeyModifiers, KeyPress as KeyEvent};
use super::{
    Action, Command, ConfigEditState, Confirm, EditMode, Event, FormKind, FormState,
    InstallPickState, LifecycleOp, Modal, Model, ProgressState, TypedConfirm,
};
use crate::config::ConfigTarget;
use crate::i18n::{t, tf, Key};

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
        Action::MetricsSampled(sample) => {
            model.metrics.push(&sample);
            vec![]
        }
        Action::OnlineList(items) => {
            model.modal = Some(Modal::InstallPick(InstallPickState::new(items)));
            vec![]
        }
        Action::ConfigLoaded { target, content } => {
            model.modal = Some(Modal::ConfigEdit(ConfigEditState::new(target, &content)));
            vec![]
        }
        Action::OpDone(message) => {
            if matches!(model.modal, Some(Modal::Progress(_))) {
                model.modal = None;
            }
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
            vec![Command::RefreshList, Command::SampleMetrics]
        }
        Event::Frame => {
            if let Some(Modal::Progress(progress)) = &mut model.modal {
                progress.tick();
            }
            vec![]
        }
        Event::Resize(_, _) => vec![],
    }
}

fn handle_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    if model.modal.is_some() {
        handle_modal_key(model, key)
    } else if model.filter_mode {
        handle_filter_key(model, key)
    } else {
        handle_list_key(model, key)
    }
}

fn handle_filter_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Esc => {
            model.filter.clear();
            model.filter_mode = false;
            model.selected = 0;
        }
        KeyCode::Enter => model.filter_mode = false,
        KeyCode::Char(c) => {
            model.filter.push(c);
            model.selected = 0;
        }
        KeyCode::Backspace => {
            model.filter.pop();
            model.selected = 0;
        }
        _ => {}
    }
    vec![]
}

fn handle_list_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => model.modal = Some(Modal::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => model.should_quit = true,
        // Esc clears an active filter (quit is via `q`).
        (KeyCode::Esc, _) => {
            model.filter.clear();
            model.selected = 0;
        }
        (KeyCode::Char('/'), _) => model.filter_mode = true,
        (KeyCode::Char('?'), _) => model.modal = Some(Modal::Help),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => model.select_next(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => model.select_prev(),
        // Shift+Enter (where the terminal reports it) and `w` open a new tab;
        // plain Enter runs an inline shell.
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => return launch_tab(model),
        (KeyCode::Enter, _) => return launch_inline(model),
        (KeyCode::Char('w'), _) => return launch_tab(model),
        (KeyCode::Char('r'), _) => return vec![Command::RefreshList],
        (KeyCode::Char('s'), _) => return start_selected(model),
        (KeyCode::Char('d'), _) => return set_default_selected(model),
        (KeyCode::Char('x'), _) => open_confirm_terminate(model),
        (KeyCode::Char('X'), _) => open_confirm_shutdown(model),
        (KeyCode::Char('u'), _) => open_confirm_unregister(model),
        (KeyCode::Char('e'), _) => open_export_form(model),
        (KeyCode::Char('m'), _) => open_import_form(model),
        (KeyCode::Char('i'), _) => {
            model.status = Some(t(model.lang, Key::StatusFetching).to_string());
            return vec![Command::ListOnline];
        }
        (KeyCode::Char('c'), _) => return load_config(model, ConfigTarget::WslConfig),
        (KeyCode::Char('C'), _) => return load_wslconf(model),
        (KeyCode::Char('L'), _) => {
            model.lang = model.lang.toggled();
            return vec![Command::SavePrefs];
        }
        _ => {}
    }
    vec![]
}

fn open_export_form(model: &mut Model) {
    let Some(name) = selected_name(model) else {
        return;
    };
    let default_path = format!("{name}.tar");
    model.modal = Some(Modal::Form(FormState::export(name, default_path)));
}

fn open_import_form(model: &mut Model) {
    model.modal = Some(Modal::Form(FormState::import()));
}

fn load_config(model: &mut Model, target: ConfigTarget) -> Vec<Command> {
    model.status = Some(tf(model.lang, Key::StatusLoading, &[&target.label()]));
    vec![Command::LoadConfig(target)]
}

fn load_wslconf(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    load_config(model, ConfigTarget::WslConf(name))
}

fn selected_name(model: &Model) -> Option<String> {
    model.selected_distro().map(|distro| distro.name.clone())
}

fn start_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.status = Some(tf(model.lang, Key::StatusStarting, &[&name]));
    vec![Command::Lifecycle(LifecycleOp::Start(name))]
}

fn set_default_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.status = Some(tf(model.lang, Key::StatusSettingDefault, &[&name]));
    vec![Command::Lifecycle(LifecycleOp::SetDefault(name))]
}

fn launch_inline(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.status = Some(tf(model.lang, Key::StatusLaunchingShell, &[&name]));
    vec![Command::LaunchInlineShell(name)]
}

fn launch_tab(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    vec![Command::LaunchTabShell(name)]
}

fn open_confirm_terminate(model: &mut Model) {
    let Some(name) = selected_name(model) else {
        return;
    };
    let prompt = tf(model.lang, Key::PromptTerminate, &[&name]);
    model.modal = Some(Modal::Confirm(Confirm {
        prompt,
        require_typed: None,
        on_confirm: vec![Command::Lifecycle(LifecycleOp::Terminate(name))],
        progress_title: None,
        status: None,
    }));
}

fn open_confirm_shutdown(model: &mut Model) {
    model.modal = Some(Modal::Confirm(Confirm {
        prompt: tf(model.lang, Key::PromptShutdown, &[]),
        require_typed: None,
        on_confirm: vec![Command::Lifecycle(LifecycleOp::Shutdown)],
        progress_title: None,
        status: None,
    }));
}

fn open_confirm_unregister(model: &mut Model) {
    let Some(name) = selected_name(model) else {
        return;
    };
    let prompt = tf(model.lang, Key::PromptUnregister, &[&name]);
    model.modal = Some(Modal::Confirm(Confirm {
        prompt,
        require_typed: Some(TypedConfirm {
            expected: name.clone(),
            input: String::new(),
        }),
        on_confirm: vec![Command::Lifecycle(LifecycleOp::Unregister(name))],
        progress_title: None,
        status: None,
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
        Modal::Form(form) => handle_form_key(model, form, key),
        Modal::Progress(progress) => handle_progress_key(model, progress, key),
        Modal::InstallPick(pick) => handle_install_key(model, pick, key),
        Modal::ConfigEdit(state) => handle_config_key(model, state, key),
        // Any key dismisses help (the modal was already taken above).
        Modal::Help => vec![],
        Modal::Quit => handle_quit_key(model, key),
    }
}

fn handle_quit_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    if matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
        model.should_quit = true;
    }
    // Esc / n / any other key: the modal was already taken, so it closes.
    vec![]
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
                confirm_action(model, confirm)
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
                confirm_action(model, confirm)
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

fn confirm_action(model: &mut Model, confirm: Confirm) -> Vec<Command> {
    if let Some(title) = confirm.progress_title {
        model.modal = Some(Modal::Progress(ProgressState::new(title)));
    }
    if let Some(status) = confirm.status {
        model.status = Some(status);
    }
    confirm.on_confirm
}

fn handle_form_key(model: &mut Model, mut form: FormState, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Esc => vec![], // cancelled (modal already taken)
        KeyCode::Tab | KeyCode::Down => {
            form.focus_next();
            model.modal = Some(Modal::Form(form));
            vec![]
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.focus_prev();
            model.modal = Some(Modal::Form(form));
            vec![]
        }
        KeyCode::Enter => submit_form(model, form),
        KeyCode::Char(c) => {
            form.current_mut().insert(c);
            model.modal = Some(Modal::Form(form));
            vec![]
        }
        KeyCode::Backspace => {
            form.current_mut().backspace();
            model.modal = Some(Modal::Form(form));
            vec![]
        }
        _ => {
            model.modal = Some(Modal::Form(form));
            vec![]
        }
    }
}

fn submit_form(model: &mut Model, form: FormState) -> Vec<Command> {
    match form.kind.clone() {
        FormKind::Export { distro } => {
            let path = form.value(0).trim().to_string();
            if path.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            let title = tf(model.lang, Key::ProgExporting, &[&distro]);
            model.modal = Some(Modal::Progress(ProgressState::new(title)));
            vec![Command::Export {
                name: distro,
                path: PathBuf::from(path),
            }]
        }
        FormKind::Import => {
            let name = form.value(0).trim().to_string();
            let dir = form.value(1).trim().to_string();
            let tar = form.value(2).trim().to_string();
            if name.is_empty() || dir.is_empty() || tar.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            let title = tf(model.lang, Key::ProgImporting, &[&name]);
            let import = Command::Import {
                name: name.clone(),
                dir: PathBuf::from(dir),
                tar: PathBuf::from(tar),
            };
            // If a distro with this name already exists, confirm the overwrite.
            if model
                .distros
                .iter()
                .any(|distro| distro.name.eq_ignore_ascii_case(&name))
            {
                model.modal = Some(Modal::Confirm(Confirm {
                    prompt: tf(model.lang, Key::PromptImportOverwrite, &[&name]),
                    require_typed: None,
                    on_confirm: vec![import],
                    progress_title: Some(title),
                    status: None,
                }));
                return vec![];
            }
            model.modal = Some(Modal::Progress(ProgressState::new(title)));
            vec![import]
        }
    }
}

fn handle_progress_key(model: &mut Model, progress: ProgressState, key: KeyEvent) -> Vec<Command> {
    match key.code {
        // Cancel: close the dialog and ask the runtime to abort the task.
        KeyCode::Esc => {
            model.status = Some(t(model.lang, Key::StatusCancelling).to_string());
            vec![Command::CancelOp]
        }
        _ => {
            model.modal = Some(Modal::Progress(progress));
            vec![]
        }
    }
}

fn handle_install_key(
    model: &mut Model,
    mut pick: InstallPickState,
    key: KeyEvent,
) -> Vec<Command> {
    match key.code {
        KeyCode::Esc => vec![], // cancelled
        KeyCode::Down => {
            pick.select_next();
            model.modal = Some(Modal::InstallPick(pick));
            vec![]
        }
        KeyCode::Up => {
            pick.select_prev();
            model.modal = Some(Modal::InstallPick(pick));
            vec![]
        }
        KeyCode::Enter => {
            if let Some(name) = pick.selected_name() {
                model.modal = Some(Modal::Progress(ProgressState::new(format!(
                    "Installing '{name}'"
                ))));
                vec![Command::Install { name }]
            } else {
                model.modal = Some(Modal::InstallPick(pick));
                vec![]
            }
        }
        KeyCode::Char(c) => {
            pick.push_filter(c);
            model.modal = Some(Modal::InstallPick(pick));
            vec![]
        }
        KeyCode::Backspace => {
            pick.pop_filter();
            model.modal = Some(Modal::InstallPick(pick));
            vec![]
        }
        _ => {
            model.modal = Some(Modal::InstallPick(pick));
            vec![]
        }
    }
}

fn handle_config_key(model: &mut Model, mut state: ConfigEditState, key: KeyEvent) -> Vec<Command> {
    // Ctrl+S saves and closes.
    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let content = state.rendered();
        let target = state.target.clone();
        model.status = Some(tf(model.lang, Key::StatusSaving, &[&target.label()]));
        return vec![Command::SaveConfig { target, content }];
    }
    match key.code {
        KeyCode::Esc => return vec![], // cancelled (closed)
        KeyCode::Tab => match state.mode {
            EditMode::Form => state.to_raw(),
            EditMode::Raw => state.to_form(),
        },
        _ => match state.mode {
            EditMode::Form => config_form_key(&mut state, key),
            EditMode::Raw => config_raw_key(&mut state, key),
        },
    }
    model.modal = Some(Modal::ConfigEdit(state));
    vec![]
}

fn config_form_key(state: &mut ConfigEditState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::BackTab => state.focus_prev(),
        KeyCode::Down => state.focus_next(),
        KeyCode::Char(c) => {
            if let Some(field) = state.current_field_mut() {
                field.input.insert(c);
            }
        }
        KeyCode::Backspace => {
            if let Some(field) = state.current_field_mut() {
                field.input.backspace();
            }
        }
        _ => {}
    }
}

fn config_raw_key(state: &mut ConfigEditState, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => state.raw.insert(c),
        KeyCode::Backspace => state.raw.backspace(),
        KeyCode::Enter => state.raw.newline(),
        KeyCode::Left => state.raw.left(),
        KeyCode::Right => state.raw.right(),
        KeyCode::Up => state.raw.up(),
        KeyCode::Down => state.raw.down(),
        _ => {}
    }
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
    fn q_opens_quit_confirm_then_y_quits() {
        let mut m = Model::default();
        update(&mut m, ch('q'));
        assert!(!m.should_quit, "q should ask, not quit immediately");
        assert!(matches!(m.modal, Some(Modal::Quit)));
        update(&mut m, ch('y'));
        assert!(m.should_quit);
    }

    #[test]
    fn quit_confirm_can_be_cancelled() {
        let mut m = Model::default();
        update(&mut m, ch('q'));
        update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!m.should_quit);
        assert!(m.modal.is_none());
    }

    #[test]
    fn slash_filters_the_list() {
        let mut m = model_with(&["Debian", "Ubuntu", "kali-linux"]);
        update(&mut m, ch('/'));
        assert!(m.filter_mode);
        for c in "ka".chars() {
            update(&mut m, ch(c));
        }
        let visible: Vec<&str> = m
            .visible_distros()
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(visible, ["kali-linux"]);
        // Enter exits filter mode but keeps the filter applied.
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!m.filter_mode);
        assert_eq!(
            m.selected_distro().map(|d| d.name.as_str()),
            Some("kali-linux")
        );
    }

    #[test]
    fn filter_esc_clears_filter() {
        let mut m = model_with(&["Debian", "Ubuntu"]);
        update(&mut m, ch('/'));
        update(&mut m, ch('U'));
        update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!m.filter_mode);
        assert!(m.filter.is_empty());
        assert_eq!(m.visible_distros().len(), 2);
    }

    #[test]
    fn question_opens_help() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('?'));
        assert!(matches!(m.modal, Some(Modal::Help)));
        // Any key dismisses.
        update(&mut m, ch(' '));
        assert!(m.modal.is_none());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(m.should_quit);
    }

    #[test]
    fn tick_polls_refresh_and_metrics() {
        let mut m = Model::default();
        let cmds = update(&mut m, Action::Event(Event::Tick));
        assert!(cmds.contains(&Command::RefreshList));
        assert!(cmds.contains(&Command::SampleMetrics));
        assert_eq!(m.ticks, 1);
    }

    #[test]
    fn metrics_sampled_updates_history() {
        use crate::metrics::MetricsSample;
        let mut m = Model::default();
        update(
            &mut m,
            Action::MetricsSampled(MetricsSample {
                vmmem_bytes: Some(123),
                total_mem_bytes: 456,
            }),
        );
        assert_eq!(m.metrics.latest_vmmem, Some(123));
        assert_eq!(m.metrics.total_mem_bytes, 456);
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

    #[test]
    fn enter_launches_inline_shell() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(cmds, vec![Command::LaunchInlineShell("Debian".into())]);
    }

    #[test]
    fn shift_enter_launches_tab_shell() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(cmds, vec![Command::LaunchTabShell("Debian".into())]);
    }

    #[test]
    fn w_launches_tab_shell() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('w'));
        assert_eq!(cmds, vec![Command::LaunchTabShell("Debian".into())]);
    }

    #[test]
    fn enter_with_empty_list_does_nothing() {
        let mut m = Model::default();
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty());
    }

    fn online(name: &str, friendly: &str) -> crate::wsl::OnlineDistro {
        crate::wsl::OnlineDistro {
            name: name.to_string(),
            friendly: friendly.to_string(),
        }
    }

    #[test]
    fn e_opens_export_form() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('e'));
        assert!(matches!(m.modal, Some(Modal::Form(_))));
    }

    #[test]
    fn export_form_submit_dispatches_and_shows_progress() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('e'));
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Export {
                name: "Debian".into(),
                path: PathBuf::from("Debian.tar"),
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn import_form_requires_all_fields() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('m'));
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty());
        assert!(matches!(m.modal, Some(Modal::Form(_))));
    }

    #[test]
    fn import_existing_name_asks_to_overwrite() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('m')); // import form
        let fill = |m: &mut Model, text: &str| {
            for c in text.chars() {
                update(m, ch(c));
            }
        };
        fill(&mut m, "Debian"); // name (collides with existing)
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        fill(&mut m, "C:/wsl/dir");
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        fill(&mut m, "C:/backup.tar");
        // Submit: collision -> overwrite confirmation, no command yet.
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty(), "should confirm before overwriting");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        // Confirm -> Import dispatched and a progress dialog opens.
        let cmds = update(&mut m, ch('y'));
        assert!(matches!(cmds.as_slice(), [Command::Import { .. }]));
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn import_new_name_skips_overwrite_confirm() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('m'));
        for c in "Fresh".chars() {
            update(&mut m, ch(c));
        }
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        for c in "C:/d".chars() {
            update(&mut m, ch(c));
        }
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        for c in "C:/t.tar".chars() {
            update(&mut m, ch(c));
        }
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(cmds.as_slice(), [Command::Import { .. }]));
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn i_requests_online_list() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('i'));
        assert_eq!(cmds, vec![Command::ListOnline]);
    }

    #[test]
    fn online_list_opens_install_pick() {
        let mut m = Model::default();
        update(
            &mut m,
            Action::OnlineList(vec![online("Ubuntu", "Ubuntu"), online("Debian", "Debian")]),
        );
        assert!(matches!(m.modal, Some(Modal::InstallPick(_))));
    }

    #[test]
    fn install_pick_filters_and_installs() {
        let mut m = Model::default();
        update(
            &mut m,
            Action::OnlineList(vec![
                online("Ubuntu", "Ubuntu"),
                online("Debian", "Debian GNU/Linux"),
            ]),
        );
        update(&mut m, ch('D')); // filter to Debian
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Install {
                name: "Debian".into()
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn progress_esc_cancels() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('e'));
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
        let cmds = update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(cmds, vec![Command::CancelOp]);
        assert!(m.modal.is_none());
    }

    #[test]
    fn op_done_closes_progress_modal() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('e'));
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        let cmds = update(&mut m, Action::OpDone("Exported Debian".into()));
        assert!(m.modal.is_none());
        assert!(cmds.contains(&Command::RefreshList));
    }

    #[test]
    fn frame_advances_progress_spinner() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, ch('e'));
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        let before = match &m.modal {
            Some(Modal::Progress(p)) => p.frame,
            _ => panic!("expected progress modal"),
        };
        update(&mut m, Action::Event(Event::Frame));
        let after = match &m.modal {
            Some(Modal::Progress(p)) => p.frame,
            _ => panic!("expected progress modal"),
        };
        assert_eq!(after, before + 1);
    }

    fn config_loaded(content: &str) -> Action {
        Action::ConfigLoaded {
            target: ConfigTarget::WslConfig,
            content: content.to_string(),
        }
    }

    #[test]
    fn c_loads_wslconfig() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('c'));
        assert_eq!(cmds, vec![Command::LoadConfig(ConfigTarget::WslConfig)]);
    }

    #[test]
    fn shift_c_loads_wslconf_for_selected() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, key(KeyCode::Char('C'), KeyModifiers::SHIFT));
        assert_eq!(
            cmds,
            vec![Command::LoadConfig(ConfigTarget::WslConf("Debian".into()))]
        );
    }

    #[test]
    fn config_loaded_opens_editor() {
        let mut m = Model::default();
        update(&mut m, config_loaded("[wsl2]\nmemory=8GB\n"));
        assert!(matches!(m.modal, Some(Modal::ConfigEdit(_))));
    }

    #[test]
    fn config_editor_saves_with_ctrl_s() {
        let mut m = Model::default();
        update(&mut m, config_loaded("[wsl2]\nmemory=8GB\n"));
        let cmds = update(&mut m, key(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(
            cmds.as_slice(),
            [Command::SaveConfig {
                target: ConfigTarget::WslConfig,
                ..
            }]
        ));
        assert!(m.modal.is_none());
    }

    #[test]
    fn config_editor_tab_toggles_to_raw() {
        let mut m = Model::default();
        update(&mut m, config_loaded("[wsl2]\nmemory=8GB\n"));
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        match &m.modal {
            Some(Modal::ConfigEdit(state)) => assert_eq!(state.mode, EditMode::Raw),
            _ => panic!("expected config editor"),
        }
    }

    #[test]
    fn shift_l_toggles_language_and_saves() {
        use crate::i18n::Lang;
        let mut m = Model::default();
        assert_eq!(m.lang, Lang::En);
        let cmds = update(&mut m, key(KeyCode::Char('L'), KeyModifiers::SHIFT));
        assert_eq!(m.lang, Lang::Ja);
        assert_eq!(cmds, vec![Command::SavePrefs]);
    }
}
