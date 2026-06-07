//! The pure update reducer: `(Model, Action) -> Vec<Command>`. Terminal- and
//! IO-free, so it is unit-tested headlessly. Side effects are described by the
//! returned [`Command`]s, which the runtime executes.

use std::path::PathBuf;

use super::input::{KeyCode, KeyMods as KeyModifiers, KeyPress as KeyEvent};
use super::{
    Action, Command, ConfigEditState, Confirm, EditMode, Event, FormKind, FormState,
    ImportPickState, InstallPickState, LifecycleOp, Modal, Model, ProgressState, TypedConfirm,
};
use crate::config::ConfigTarget;
use crate::i18n::{t, tf, Key};
use crate::prefs::ShellLaunch;
use crate::wsl::DistroState;

mod modal;
use modal::handle_modal_key;

/// Advance the model in response to an action, returning any side effects.
pub fn update(model: &mut Model, action: Action) -> Vec<Command> {
    match action {
        Action::Quit => {
            model.should_quit = true;
            vec![]
        }
        Action::Refreshed(mut distros) => {
            // Carry forward already-fetched in-distro disk usage by name so we
            // don't re-sample it on every poll.
            for distro in &mut distros {
                if distro.inner_disk.is_none() {
                    if let Some(old) = model.distros.iter().find(|d| d.name == distro.name) {
                        distro.inner_disk = old.inner_disk;
                    }
                }
            }
            model.distros = distros;
            model.loaded = true;
            model.last_error = None;
            model.clamp_selection();
            let mut cmds = sample_inner_disk_if_needed(model);
            cmds.extend(sample_vm_memory_if_needed(model));
            cmds
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
        Action::InnerDiskSampled { name, inner } => {
            if let Some(distro) = model.distros.iter_mut().find(|d| d.name == name) {
                distro.inner_disk = inner;
            }
            vec![]
        }
        Action::VmMemorySampled(total) => {
            model.vm_mem_total = total;
            // Only a successful read latches the once-per-run gate; a failed
            // sample (`None`) leaves it open so the next refresh retries rather
            // than sticking on the host-RAM fallback denominator.
            model.vm_mem_attempted = total.is_some();
            vec![]
        }
        Action::OnlineList(items) => {
            model.modal = Some(Modal::InstallPick(InstallPickState::new(items)));
            vec![]
        }
        Action::ExportDialogReady { distro, filename } => {
            model.modal = Some(Modal::Form(FormState::export(distro, filename)));
            vec![]
        }
        Action::ExportsListed(entries) => {
            model.modal = Some(Modal::ImportPick(ImportPickState::new(entries)));
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
            model.set_status(message);
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
            model.tick_status();
            vec![]
        }
        Event::Resize(_, _) => vec![],
    }
}

fn handle_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    // Any key press dismisses the last transient status message; the key's own
    // handler below may immediately set a fresh one.
    model.clear_status();
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
        // Ctrl+C asks to quit too (no immediate force-quit).
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => model.modal = Some(Modal::Quit),
        // Esc backs out one level: clear an active filter first, else confirm quit.
        (KeyCode::Esc, _) => {
            if model.filter.is_empty() {
                model.modal = Some(Modal::Quit);
            } else {
                model.filter.clear();
                model.selected = 0;
            }
        }
        (KeyCode::Char('/'), _) => model.filter_mode = true,
        (KeyCode::Char('?'), _) => model.modal = Some(Modal::Help),
        (KeyCode::Down, _) if model.keybind_style.arrows_enabled() => model.select_next(),
        (KeyCode::Up, _) if model.keybind_style.arrows_enabled() => model.select_prev(),
        (KeyCode::Char('j'), _) if model.keybind_style.vim_enabled() => model.select_next(),
        (KeyCode::Char('k'), _) if model.keybind_style.vim_enabled() => model.select_prev(),
        // Enter follows the default shell-launch preference; Shift+Enter does
        // the other mode and `w` always opens a new tab.
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            return launch_shell(model, model.default_shell_launch.other())
        }
        (KeyCode::Enter, _) => return launch_shell(model, model.default_shell_launch),
        (KeyCode::Char('w'), _) => return launch_shell(model, ShellLaunch::NewTab),
        (KeyCode::Char('r'), _) => return vec![Command::RefreshList],
        (KeyCode::Char('s'), _) => return start_selected(model),
        (KeyCode::Char('d'), _) => return set_default_selected(model),
        (KeyCode::Char('x'), _) => open_confirm_terminate(model),
        (KeyCode::Char('X'), _) => open_confirm_shutdown(model),
        (KeyCode::Char('u'), _) => open_confirm_unregister(model),
        (KeyCode::Char('e'), _) => return open_export(model),
        (KeyCode::Char('m'), _) => return vec![Command::ListExports],
        (KeyCode::Char('i'), _) => {
            model.set_status(t(model.lang, Key::StatusFetching).to_string());
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

fn open_export(model: &mut Model) -> Vec<Command> {
    let Some(distro) = selected_name(model) else {
        return vec![];
    };
    vec![Command::OpenExportDialog { distro }]
}

fn load_config(model: &mut Model, target: ConfigTarget) -> Vec<Command> {
    model.set_status(tf(model.lang, Key::StatusLoading, &[&target.label()]));
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

/// If the selected distro is running and we have not yet attempted an in-distro
/// disk sample for it, request one (at most once per distro — no per-poll `df`).
fn sample_inner_disk_if_needed(model: &mut Model) -> Vec<Command> {
    let Some((name, running)) = model
        .selected_distro()
        .map(|distro| (distro.name.clone(), distro.state == DistroState::Running))
    else {
        return vec![];
    };
    if !running || model.inner_disk_attempted.contains(&name) {
        return vec![];
    }
    model.inner_disk_attempted.insert(name.clone());
    vec![Command::SampleInnerDisk(name)]
}

/// If the WSL VM is up (any distro running) and we don't yet have its total RAM,
/// request it (machine-wide). The gate is latched here to avoid duplicate
/// in-flight requests and released again on a failed sample (see
/// [`Action::VmMemorySampled`]), so a successful read costs no per-poll
/// `/proc/meminfo` while a transient miss still recovers. When the VM is down,
/// forget the reading so it is re-measured on the next start (e.g. after a
/// `.wslconfig` memory change).
fn sample_vm_memory_if_needed(model: &mut Model) -> Vec<Command> {
    let Some(name) = model
        .distros
        .iter()
        .find(|distro| distro.state == DistroState::Running)
        .map(|distro| distro.name.clone())
    else {
        model.vm_mem_attempted = false;
        model.vm_mem_total = None;
        return vec![];
    };
    if model.vm_mem_attempted {
        return vec![];
    }
    model.vm_mem_attempted = true;
    vec![Command::SampleVmMemory(name)]
}

fn start_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.set_status(tf(model.lang, Key::StatusStarting, &[&name]));
    vec![Command::Lifecycle(LifecycleOp::Start(name))]
}

fn set_default_selected(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.set_status(tf(model.lang, Key::StatusSettingDefault, &[&name]));
    vec![Command::Lifecycle(LifecycleOp::SetDefault(name))]
}

fn launch_shell(model: &mut Model, mode: ShellLaunch) -> Vec<Command> {
    match mode {
        ShellLaunch::Inline => launch_inline(model),
        ShellLaunch::NewTab => launch_tab(model),
    }
}

fn launch_inline(model: &mut Model) -> Vec<Command> {
    let Some(name) = selected_name(model) else {
        return vec![];
    };
    model.set_status(tf(model.lang, Key::StatusLaunchingShell, &[&name]));
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
            inner_disk: None,
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
    fn ctrl_c_opens_quit_confirm_then_y_quits() {
        let mut m = Model::default();
        update(&mut m, key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!m.should_quit, "Ctrl+C should ask, not quit immediately");
        assert!(matches!(m.modal, Some(Modal::Quit)));
        update(&mut m, ch('y'));
        assert!(m.should_quit);
    }

    #[test]
    fn esc_opens_quit_confirm_when_no_filter() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!m.should_quit, "Esc should ask, not quit immediately");
        assert!(matches!(m.modal, Some(Modal::Quit)));
        // Enter confirms the quit.
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(m.should_quit);
    }

    #[test]
    fn esc_clears_active_filter_instead_of_quitting() {
        let mut m = model_with(&["Debian", "Ubuntu"]);
        // Apply a filter, then leave filter mode (Enter keeps it applied).
        update(&mut m, ch('/'));
        update(&mut m, ch('U'));
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!m.filter.is_empty(), "filter is applied in list view");
        // Esc backs out one level: clears the filter, does NOT open quit.
        update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(m.filter.is_empty(), "Esc clears the active filter");
        assert!(
            m.modal.is_none(),
            "Esc must not open quit while a filter was active"
        );
        assert!(!m.should_quit);
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
    fn status_message_auto_expires_after_ttl() {
        let mut m = Model::default();
        update(&mut m, Action::OpDone("Done".into()));
        assert_eq!(m.status.as_deref(), Some("Done"));
        // Still shown right up to the last frame before the TTL.
        for _ in 0..(Model::STATUS_TTL_FRAMES - 1) {
            update(&mut m, Action::Event(Event::Frame));
        }
        assert_eq!(m.status.as_deref(), Some("Done"), "still shown before TTL");
        // The TTL-th frame expires it (no input needed).
        update(&mut m, Action::Event(Event::Frame));
        assert!(m.status.is_none(), "auto-expired after TTL frames");
    }

    #[test]
    fn next_key_clears_status_immediately() {
        let mut m = model_with(&["Debian"]);
        update(&mut m, Action::OpDone("Done".into()));
        assert!(m.status.is_some());
        // Any key dismisses the message at once (here a navigation key).
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert!(m.status.is_none(), "next key clears the status");
    }

    #[test]
    fn poll_tick_does_not_clear_status() {
        // Only Frames count down and only keys clear; a background poll Tick must
        // not wipe the message out from under the user.
        let mut m = Model::default();
        update(&mut m, Action::OpDone("Done".into()));
        update(&mut m, Action::Event(Event::Tick));
        assert_eq!(m.status.as_deref(), Some("Done"));
    }

    #[test]
    fn new_status_resets_expiry_countdown() {
        let mut m = Model::default();
        update(&mut m, Action::OpDone("First".into()));
        for _ in 0..20 {
            update(&mut m, Action::Event(Event::Frame));
        }
        update(&mut m, Action::OpDone("Second".into()));
        assert_eq!(
            m.status_frames_left,
            Model::STATUS_TTL_FRAMES,
            "a fresh status restarts the countdown"
        );
    }

    #[test]
    fn key_that_sets_a_new_status_keeps_it() {
        // The per-key clear must not wipe a status the same key sets: pressing the
        // start key clears the old message, then sets its own "starting…".
        let mut m = model_with(&["Debian"]);
        update(&mut m, Action::OpDone("Old".into()));
        update(&mut m, ch('s'));
        assert!(
            m.status.is_some(),
            "the start key sets a fresh status that survives the per-key clear"
        );
        assert_ne!(m.status.as_deref(), Some("Old"));
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
    fn e_opens_export_dialog_command() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('e'));
        assert_eq!(
            cmds,
            vec![Command::OpenExportDialog {
                distro: "Debian".into()
            }]
        );
        assert!(m.modal.is_none(), "form opens only after ExportDialogReady");
    }

    #[test]
    fn export_dialog_ready_opens_form() {
        let mut m = model_with(&["Debian"]);
        update(
            &mut m,
            Action::ExportDialogReady {
                distro: "Debian".into(),
                filename: "Debian-20260607-153012.tar".into(),
            },
        );
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.value(0), "Debian-20260607-153012.tar"),
            other => panic!("expected export form, got {other:?}"),
        }
    }

    #[test]
    fn m_requests_export_listing() {
        let mut m = model_with(&["Debian"]);
        assert_eq!(update(&mut m, ch('m')), vec![Command::ListExports]);
    }

    #[test]
    fn exports_listed_opens_picker() {
        use crate::manage::Archive;
        let mut m = Model::default();
        update(
            &mut m,
            Action::ExportsListed(vec![Archive {
                name: "Ubuntu.tar".into(),
                path: std::path::PathBuf::from(r"C:\wsl\exports\Ubuntu.tar"),
                size: 10,
            }]),
        );
        assert!(matches!(m.modal, Some(Modal::ImportPick(_))));
    }

    fn archive(name: &str) -> crate::manage::Archive {
        crate::manage::Archive {
            name: name.into(),
            path: std::path::PathBuf::from(format!(r"C:\wsl\exports\{name}")),
            size: 1,
        }
    }

    fn open_picker(names: &[&str]) -> Model {
        let mut m = model_with(&["Debian"]);
        m.manage_dir = std::path::PathBuf::from(r"C:\wsl");
        update(
            &mut m,
            Action::ExportsListed(names.iter().map(|n| archive(n)).collect()),
        );
        m
    }

    #[test]
    fn export_submit_builds_managed_path_and_format() {
        let mut m = model_with(&["Debian"]);
        m.manage_dir = std::path::PathBuf::from(r"C:\wsl");
        update(
            &mut m,
            Action::ExportDialogReady {
                distro: "Debian".into(),
                filename: "Debian.tar.gz".into(),
            },
        );
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Export {
                name: "Debian".into(),
                path: std::path::PathBuf::from(r"C:\wsl\exports\Debian.tar.gz"),
                format: crate::manage::ExportFormat::TarGz,
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn picker_enter_opens_name_form_prefilled() {
        let mut m = open_picker(&["Ubuntu-20260607.tar.gz"]);
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.value(0), "Ubuntu-20260607"),
            other => panic!("expected import name form, got {other:?}"),
        }
    }

    #[test]
    fn picker_import_dispatches_with_managed_dir_and_vhd_flag() {
        let mut m = open_picker(&["box.vhdx"]);
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // -> name form ("box")
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // submit
        assert_eq!(
            cmds,
            vec![Command::Import {
                name: "box".into(),
                dir: std::path::PathBuf::from(r"C:\wsl\installed\box"),
                tar: std::path::PathBuf::from(r"C:\wsl\exports\box.vhdx"),
                vhd: true,
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn picker_d_asks_to_delete() {
        let mut m = open_picker(&["old.tar"]);
        let cmds = update(&mut m, ch('d'));
        assert!(cmds.is_empty(), "delete is confirmed first");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        let cmds = update(&mut m, ch('y'));
        assert_eq!(
            cmds,
            vec![Command::DeleteExport(std::path::PathBuf::from(
                r"C:\wsl\exports\old.tar"
            ))]
        );
    }

    #[test]
    fn picker_c_opens_custom_form() {
        let mut m = open_picker(&["a.tar"]);
        update(&mut m, ch('c'));
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.fields.len(), 2),
            other => panic!("expected custom import form, got {other:?}"),
        }
    }

    #[test]
    fn picker_esc_closes() {
        let mut m = open_picker(&["a.tar"]);
        let cmds = update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(cmds.is_empty());
        assert!(m.modal.is_none());
    }

    #[test]
    fn import_existing_name_asks_overwrite() {
        let mut m = open_picker(&["Debian.tar"]); // model_with(["Debian"]) already exists
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // name form prefilled "Debian"
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty(), "must confirm overwrite");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        let cmds = update(&mut m, ch('y'));
        assert!(matches!(cmds.as_slice(), [Command::Import { .. }]));
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

    /// Open a Progress modal by driving the managed export flow to submit.
    fn open_progress_via_export(m: &mut Model) {
        m.manage_dir = std::path::PathBuf::from(r"C:\wsl");
        update(
            m,
            Action::ExportDialogReady {
                distro: "Debian".into(),
                filename: "Debian.tar".into(),
            },
        );
        update(m, key(KeyCode::Enter, KeyModifiers::NONE));
    }

    #[test]
    fn progress_esc_cancels() {
        let mut m = model_with(&["Debian"]);
        open_progress_via_export(&mut m);
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
        let cmds = update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(cmds, vec![Command::CancelOp]);
        assert!(m.modal.is_none());
    }

    #[test]
    fn op_done_closes_progress_modal() {
        let mut m = model_with(&["Debian"]);
        open_progress_via_export(&mut m);
        let cmds = update(&mut m, Action::OpDone("Exported Debian".into()));
        assert!(m.modal.is_none());
        assert!(cmds.contains(&Command::RefreshList));
    }

    #[test]
    fn frame_advances_progress_spinner() {
        let mut m = model_with(&["Debian"]);
        open_progress_via_export(&mut m);
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

    #[test]
    fn arrows_only_disables_vim_keys() {
        use crate::prefs::KeybindStyle;
        let mut m = model_with(&["a", "b"]);
        m.keybind_style = KeybindStyle::ArrowsOnly;
        update(&mut m, ch('j'));
        assert_eq!(m.selected, 0, "j must not move when arrows-only");
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 1, "arrows still move");
    }

    #[test]
    fn vim_only_disables_arrows() {
        use crate::prefs::KeybindStyle;
        let mut m = model_with(&["a", "b"]);
        m.keybind_style = KeybindStyle::VimOnly;
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(m.selected, 0, "arrows must not move when vim-only");
        update(&mut m, ch('j'));
        assert_eq!(m.selected, 1);
    }

    #[test]
    fn default_shell_launch_newtab_makes_enter_open_a_tab() {
        let mut m = model_with(&["Debian"]);
        m.default_shell_launch = ShellLaunch::NewTab;
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(cmds, vec![Command::LaunchTabShell("Debian".into())]);
        // Shift+Enter does the other mode (inline).
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(cmds, vec![Command::LaunchInlineShell("Debian".into())]);
    }

    fn running_distro(name: &str) -> Distro {
        Distro {
            state: DistroState::Running,
            ..distro(name)
        }
    }

    #[test]
    fn refreshed_samples_inner_disk_for_running_selected_once_only() {
        let mut m = Model::default();
        let cmds = update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert_eq!(
            cmds,
            vec![
                Command::SampleInnerDisk("Debian".into()),
                Command::SampleVmMemory("Debian".into()),
            ]
        );
        // A second refresh must NOT re-sample (no per-poll df / meminfo).
        let cmds = update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert!(cmds.is_empty(), "must not re-sample the same distro");
    }

    #[test]
    fn vm_memory_sampled_sets_total() {
        let mut m = Model::default();
        update(
            &mut m,
            Action::VmMemorySampled(Some(8 * 1024 * 1024 * 1024)),
        );
        assert_eq!(m.vm_mem_total, Some(8 * 1024 * 1024 * 1024));
    }

    #[test]
    fn vm_memory_resets_and_resamples_when_vm_cycles() {
        let mut m = Model::default();
        // VM up: sample once and record the total.
        update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        update(
            &mut m,
            Action::VmMemorySampled(Some(4 * 1024 * 1024 * 1024)),
        );
        assert_eq!(m.vm_mem_total, Some(4 * 1024 * 1024 * 1024));

        // VM down (no running distro): forget the stale total so the next start
        // re-measures it (e.g. after a `.wslconfig` change).
        let cmds = update(&mut m, Action::Refreshed(vec![distro("Debian")]));
        assert!(cmds.is_empty());
        assert_eq!(m.vm_mem_total, None);

        // VM up again: re-sample.
        let cmds = update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert!(cmds.contains(&Command::SampleVmMemory("Debian".into())));
    }

    #[test]
    fn vm_memory_retries_after_a_failed_sample() {
        let mut m = Model::default();
        update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        // The one attempt failed (e.g. the distro was mid-boot): do NOT latch a
        // wrong host-RAM denominator forever — the next refresh must retry.
        update(&mut m, Action::VmMemorySampled(None));
        assert_eq!(m.vm_mem_total, None);
        let cmds = update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert!(
            cmds.contains(&Command::SampleVmMemory("Debian".into())),
            "a failed sample must be retried on the next refresh"
        );
    }

    #[test]
    fn vm_memory_not_resampled_after_success() {
        let mut m = Model::default();
        update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        update(
            &mut m,
            Action::VmMemorySampled(Some(4 * 1024 * 1024 * 1024)),
        );
        // A successful read latches: no per-poll re-sampling.
        let cmds = update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert!(
            !cmds.iter().any(|c| matches!(c, Command::SampleVmMemory(_))),
            "must not re-sample once a value is known"
        );
    }

    #[test]
    fn refreshed_does_not_sample_stopped_distro() {
        let mut m = Model::default();
        let cmds = update(&mut m, Action::Refreshed(vec![distro("Debian")]));
        assert!(cmds.is_empty());
    }

    #[test]
    fn inner_disk_sample_sets_value_and_carries_forward() {
        let mut m = Model::default();
        update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        update(
            &mut m,
            Action::InnerDiskSampled {
                name: "Debian".into(),
                inner: Some((10, 100)),
            },
        );
        assert_eq!(m.distros[0].inner_disk, Some((10, 100)));
        // A later refresh (fresh list) keeps the cached value.
        update(&mut m, Action::Refreshed(vec![running_distro("Debian")]));
        assert_eq!(m.distros[0].inner_disk, Some((10, 100)));
    }

    #[test]
    fn picker_navigation_moves_selection() {
        let mut m = open_picker(&["a.tar", "b.tar"]);
        update(&mut m, key(KeyCode::Down, KeyModifiers::NONE));
        match &m.modal {
            Some(Modal::ImportPick(p)) => assert_eq!(p.selected, 1),
            other => panic!("expected picker, got {other:?}"),
        }
        update(&mut m, ch('k'));
        match &m.modal {
            Some(Modal::ImportPick(p)) => assert_eq!(p.selected, 0),
            other => panic!("expected picker, got {other:?}"),
        }
    }

    #[test]
    fn custom_import_submit_uses_typed_path_and_managed_dir() {
        let mut m = open_picker(&["a.tar"]);
        update(&mut m, ch('c')); // open custom form: field 0 = archive path, field 1 = name
        for c in r"D:\dl\thing.tar.gz".chars() {
            update(&mut m, ch(c));
        }
        update(&mut m, key(KeyCode::Tab, KeyModifiers::NONE));
        for c in "Imported".chars() {
            update(&mut m, ch(c));
        }
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Import {
                name: "Imported".into(),
                dir: std::path::PathBuf::from(r"C:\wsl\installed\Imported"),
                tar: std::path::PathBuf::from(r"D:\dl\thing.tar.gz"),
                vhd: false,
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }
}
