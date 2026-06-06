//! Modal input handling for the reducer: dispatches keys for the active modal
//! (confirm, form, progress, install picker, config editor, quit) and the
//! resulting state transitions. Pure — emits Commands only, like the rest of
//! the reducer. Entered via `handle_modal_key`, called from `super::handle_key`.

use super::*;

pub(super) fn handle_modal_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
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
        Modal::ImportPick(pick) => handle_import_pick_key(model, pick, key),
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
        model.set_status(status);
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

fn handle_import_pick_key(
    model: &mut Model,
    mut pick: ImportPickState,
    key: KeyEvent,
) -> Vec<Command> {
    match key.code {
        KeyCode::Esc => vec![], // cancelled (modal already taken)
        KeyCode::Down if model.keybind_style.arrows_enabled() => {
            pick.select_next();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Char('j') if model.keybind_style.vim_enabled() => {
            pick.select_next();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Up if model.keybind_style.arrows_enabled() => {
            pick.select_prev();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Char('k') if model.keybind_style.vim_enabled() => {
            pick.select_prev();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Enter => {
            if let Some(entry) = pick.selected_entry() {
                let tar = entry.path.clone();
                let default_name = crate::manage::derive_distro_name(&entry.name);
                model.modal = Some(Modal::Form(FormState::import_name(tar, default_name)));
            } else {
                model.modal = Some(Modal::ImportPick(pick));
            }
            vec![]
        }
        KeyCode::Char('c') => {
            model.modal = Some(Modal::Form(FormState::import_custom()));
            vec![]
        }
        KeyCode::Char('d') => {
            if let Some(entry) = pick.selected_entry() {
                let prompt = tf(model.lang, Key::PromptDeleteArchive, &[&entry.name]);
                let path = entry.path.clone();
                model.modal = Some(Modal::Confirm(Confirm {
                    prompt,
                    require_typed: None,
                    on_confirm: vec![Command::DeleteExport(path)],
                    progress_title: None,
                    status: None,
                }));
            } else {
                model.modal = Some(Modal::ImportPick(pick));
            }
            vec![]
        }
        _ => {
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
    }
}

fn submit_form(model: &mut Model, form: FormState) -> Vec<Command> {
    match form.kind.clone() {
        FormKind::Export { distro } => {
            let filename = form.value(0).trim().to_string();
            if filename.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            let path = crate::manage::exports_dir(&model.manage_dir).join(&filename);
            let format = crate::manage::ExportFormat::from_filename(&filename);
            let title = tf(model.lang, Key::ProgExporting, &[&distro]);
            model.modal = Some(Modal::Progress(ProgressState::new(title)));
            vec![Command::Export {
                name: distro,
                path,
                format,
            }]
        }
        FormKind::ImportName { tar } => {
            let name = form.value(0).trim().to_string();
            if name.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            submit_import(model, name, tar)
        }
        FormKind::ImportCustom => {
            let tar = form.value(0).trim().to_string();
            let name = form.value(1).trim().to_string();
            if tar.is_empty() || name.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            submit_import(model, name, PathBuf::from(tar))
        }
    }
}

fn submit_import(model: &mut Model, name: String, tar: PathBuf) -> Vec<Command> {
    let dir = crate::manage::installed_dir(&model.manage_dir, &name);
    let vhd = crate::manage::is_vhd_archive(&tar.to_string_lossy());
    let title = tf(model.lang, Key::ProgImporting, &[&name]);
    let import = Command::Import {
        name: name.clone(),
        dir,
        tar,
        vhd,
    };
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

fn handle_progress_key(model: &mut Model, progress: ProgressState, key: KeyEvent) -> Vec<Command> {
    match key.code {
        // Cancel: close the dialog and ask the runtime to abort the task.
        KeyCode::Esc => {
            model.set_status(t(model.lang, Key::StatusCancelling).to_string());
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
        model.set_status(tf(model.lang, Key::StatusSaving, &[&target.label()]));
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
