//! Runtime: the async event loop, the command dispatcher, and the terminal
//! wrapper.
//!
//! The loop multiplexes terminal events (crossterm `EventStream`), a timer
//! tick, and an action channel with `tokio::select!`, runs each resulting
//! [`Action`] through the pure [`crate::app::update`] reducer, and dispatches
//! the returned [`Command`]s as async tasks whose results return as actions.

mod tui;

pub use tui::{install_panic_hook, Tui};

use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::app::input::{KeyCode, KeyMods, KeyPress};
use crate::app::{update, Action, Command, Event, LifecycleOp, Model};
use crate::config::{self, ConfigTarget};
use crate::i18n::{tf, Key, Lang};
use crate::metrics;
use crate::prefs::Prefs;
use crate::ui;
use crate::wsl::{self, RealWslBackend, WslBackend};

/// Animation interval for spinners (no polling).
const FRAME: Duration = Duration::from_millis(120);

/// Set up the terminal and run the event loop to completion, restoring the
/// terminal afterwards regardless of how the loop ended.
pub async fn run(prefs: Prefs) -> Result<()> {
    let backend: Arc<dyn WslBackend> = Arc::new(RealWslBackend);
    let mut model = Model {
        lang: prefs.effective_lang(),
        keybind_style: prefs.keybind_style,
        default_shell_launch: prefs.default_shell_launch,
        metrics: metrics::MetricsHistory::new(prefs.history_len),
        ..Default::default()
    };
    let mut tui = Tui::new()?;
    tui.enter()?;
    let result = event_loop(&mut tui, &mut model, backend, prefs).await;
    let _ = tui.exit();
    result
}

async fn event_loop(
    tui: &mut Tui,
    model: &mut Model,
    backend: Arc<dyn WslBackend>,
    mut prefs: Prefs,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_secs(prefs.poll_interval()));
    let mut frame = tokio::time::interval(FRAME);
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    // Abort handle for the in-flight long-running operation (export/import/install).
    let mut current_op: Option<tokio::task::AbortHandle> = None;

    // Kick off the initial load and metrics sample immediately.
    dispatch(Command::RefreshList, &backend, &action_tx, model.lang);
    dispatch(Command::SampleMetrics, &backend, &action_tx, model.lang);

    loop {
        tui.draw(|f| ui::view(f, model))?;
        if model.should_quit {
            break;
        }

        let action = tokio::select! {
            maybe_event = events.next() => match maybe_event {
                Some(Ok(event)) => match map_event(event) {
                    Some(action) => action,
                    None => continue,
                },
                _ => continue,
            },
            _ = tick.tick() => Action::Event(Event::Tick),
            _ = frame.tick() => Action::Event(Event::Frame),
            Some(action) = action_rx.recv() => action,
        };

        for command in update(model, action) {
            let lang = model.lang;
            match command {
                // Inline shell needs the terminal, so it runs here (not as a
                // spawned task): suspend the TUI, hand over the console, resume.
                Command::LaunchInlineShell(name) => {
                    run_inline_shell(tui, &name, lang).await?;
                    model.status = Some(tf(lang, Key::StatusReturnedFrom, &[&name]));
                    dispatch(Command::RefreshList, &backend, &action_tx, lang);
                }
                Command::LaunchTabShell(name) => launch_tab_shell(&name, &action_tx, lang),
                // Long-running, cancellable operations: keep the abort handle so
                // a later CancelOp (or a superseding op) can kill the child.
                command @ (Command::Export { .. }
                | Command::Import { .. }
                | Command::Install { .. }) => {
                    if let Some(previous) = current_op.take() {
                        previous.abort();
                    }
                    current_op = Some(spawn_long_op(command, &backend, &action_tx, lang));
                }
                Command::CancelOp => {
                    if let Some(handle) = current_op.take() {
                        handle.abort();
                    }
                }
                Command::SavePrefs => {
                    prefs.lang = Some(model.lang);
                    let _ = crate::prefs::save(&prefs);
                }
                spawnable => dispatch(spawnable, &backend, &action_tx, lang),
            }
        }
    }
    Ok(())
}

/// Spawn a long-running operation, returning its abort handle so it can be
/// cancelled. The child is killed if the task is aborted (`kill_on_drop`).
fn spawn_long_op(
    command: Command,
    backend: &Arc<dyn WslBackend>,
    tx: &UnboundedSender<Action>,
    lang: Lang,
) -> tokio::task::AbortHandle {
    let backend = Arc::clone(backend);
    let tx = tx.clone();
    let task = tokio::spawn(async move {
        let (label, result) = match &command {
            Command::Export { name, path } => (
                tf(lang, Key::DoneExported, &[name]),
                backend.export(name, path).await,
            ),
            Command::Import { name, dir, tar } => (
                tf(lang, Key::DoneImported, &[name]),
                backend.import(name, dir, tar).await,
            ),
            Command::Install { name } => (
                tf(lang, Key::DoneInstalled, &[name]),
                backend.install(name).await,
            ),
            _ => return,
        };
        let action = match result {
            Ok(()) => Action::OpDone(label),
            Err(error) => Action::OpFailed(tf(lang, Key::FailOp, &[&error.to_string()])),
        };
        let _ = tx.send(action);
    });
    task.abort_handle()
}

/// Suspend the TUI, run `wsl -d <name>` with the console handed over, then
/// resume. Failures to spawn the shell are reported but never abort the app;
/// only terminal suspend/resume errors propagate.
async fn run_inline_shell(tui: &mut Tui, name: &str, lang: Lang) -> Result<()> {
    tui.suspend()?;
    println!("\n{}\n", tf(lang, Key::ShellBanner, &[name]));

    let distro = name.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        std::process::Command::new("wsl.exe")
            .args(["-d", &distro])
            .status()
    })
    .await;
    match outcome {
        Ok(Ok(_status)) => {}
        Ok(Err(error)) => eprintln!("Failed to launch shell: {error}"),
        Err(error) => eprintln!("Shell task error: {error}"),
    }

    tui.resume()
}

/// Open an interactive shell in a new Windows Terminal tab. Reports success or,
/// if `wt.exe` is missing, suggests the inline shell instead.
fn launch_tab_shell(name: &str, tx: &UnboundedSender<Action>, lang: Lang) {
    let result = std::process::Command::new("wt.exe")
        .args(["-w", "0", "nt", "wsl.exe", "-d", name])
        .spawn();
    let action = match result {
        Ok(_child) => Action::OpDone(tf(lang, Key::DoneOpenedTab, &[name])),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Action::OpFailed(tf(lang, Key::WtNotFound, &[]))
        }
        Err(error) => Action::OpFailed(tf(lang, Key::FailOp, &[&error.to_string()])),
    };
    let _ = tx.send(action);
}

/// Execute a command as an async task, sending the resulting action back over
/// the channel.
fn dispatch(
    command: Command,
    backend: &Arc<dyn WslBackend>,
    tx: &UnboundedSender<Action>,
    lang: Lang,
) {
    let backend = Arc::clone(backend);
    let tx = tx.clone();
    match command {
        Command::RefreshList => {
            tokio::spawn(async move {
                let action = match wsl::refresh(backend.as_ref()).await {
                    Ok(distros) => Action::Refreshed(distros),
                    Err(error) => Action::RefreshFailed(error.to_string()),
                };
                let _ = tx.send(action);
            });
        }
        Command::SampleMetrics => {
            tokio::spawn(async move {
                let sample = tokio::task::spawn_blocking(metrics::sample)
                    .await
                    .unwrap_or_default();
                let _ = tx.send(Action::MetricsSampled(sample));
            });
        }
        Command::Lifecycle(op) => {
            tokio::spawn(async move {
                let action = run_lifecycle(backend.as_ref(), op, lang).await;
                let _ = tx.send(action);
            });
        }
        Command::ListOnline => {
            tokio::spawn(async move {
                let action = match backend.list_online().await {
                    Ok(items) => Action::OnlineList(items),
                    Err(error) => {
                        Action::OpFailed(tf(lang, Key::FailListOnline, &[&error.to_string()]))
                    }
                };
                let _ = tx.send(action);
            });
        }
        Command::LoadConfig(target) => {
            tokio::spawn(async move {
                let result = match &target {
                    ConfigTarget::WslConfig => load_wslconfig().await,
                    ConfigTarget::WslConf(distro) => {
                        backend.read_conf(distro).await.map_err(|e| e.to_string())
                    }
                };
                let action = match result {
                    Ok(content) => Action::ConfigLoaded { target, content },
                    Err(error) => Action::OpFailed(tf(lang, Key::FailLoadConfig, &[&error])),
                };
                let _ = tx.send(action);
            });
        }
        Command::SaveConfig { target, content } => {
            tokio::spawn(async move {
                let result = match &target {
                    ConfigTarget::WslConfig => save_wslconfig(content.clone()).await,
                    ConfigTarget::WslConf(distro) => backend
                        .write_conf(distro, &content)
                        .await
                        .map_err(|e| e.to_string()),
                };
                let action = match result {
                    Ok(()) => Action::OpDone(tf(lang, Key::DoneSavedConfig, &[&target.label()])),
                    Err(error) => Action::OpFailed(tf(lang, Key::FailSaveConfig, &[&error])),
                };
                let _ = tx.send(action);
            });
        }
        // Shell commands and long-running/cancellable operations are handled
        // inline in the event loop, so they never reach the spawn dispatcher.
        Command::LaunchInlineShell(_)
        | Command::LaunchTabShell(_)
        | Command::Export { .. }
        | Command::Import { .. }
        | Command::Install { .. }
        | Command::CancelOp
        | Command::SavePrefs => {}
    }
}

/// Read `.wslconfig` off the async loop.
async fn load_wslconfig() -> std::result::Result<String, String> {
    tokio::task::spawn_blocking(config::load_wslconfig)
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))
}

/// Write `.wslconfig` off the async loop.
async fn save_wslconfig(content: String) -> std::result::Result<(), String> {
    tokio::task::spawn_blocking(move || config::save_wslconfig(&content))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))
}

/// Run a lifecycle operation through the backend and map the outcome to an
/// [`Action`].
async fn run_lifecycle(backend: &dyn WslBackend, op: LifecycleOp, lang: Lang) -> Action {
    let result = match &op {
        LifecycleOp::Start(name) => backend.start(name).await,
        LifecycleOp::Terminate(name) => backend.terminate(name).await,
        LifecycleOp::Shutdown => backend.shutdown().await,
        LifecycleOp::SetDefault(name) => backend.set_default(name).await,
        LifecycleOp::Unregister(name) => backend.unregister(name).await,
    };
    match result {
        Ok(()) => Action::OpDone(op.done_message(lang)),
        Err(error) => Action::OpFailed(tf(lang, Key::FailOp, &[&error.to_string()])),
    }
}

/// Translate a crossterm event into an [`Action`], dropping anything the app
/// does not act on. Key *release*/*repeat* events (common on Windows) are
/// filtered so a single press is not handled twice.
fn map_event(event: CrosstermEvent) -> Option<Action> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
            Some(Action::Event(Event::Key(convert_key(key))))
        }
        CrosstermEvent::Resize(w, h) => Some(Action::Event(Event::Resize(w, h))),
        _ => None,
    }
}

/// Translate a crossterm key event into the app's backend-independent key type.
fn convert_key(key: crossterm::event::KeyEvent) -> KeyPress {
    use crossterm::event::{KeyCode as CKeyCode, KeyModifiers as CMods};

    let code = match key.code {
        CKeyCode::Char(c) => KeyCode::Char(c),
        CKeyCode::Enter => KeyCode::Enter,
        CKeyCode::Esc => KeyCode::Esc,
        CKeyCode::Backspace => KeyCode::Backspace,
        CKeyCode::Tab => KeyCode::Tab,
        CKeyCode::BackTab => KeyCode::BackTab,
        CKeyCode::Up => KeyCode::Up,
        CKeyCode::Down => KeyCode::Down,
        CKeyCode::Left => KeyCode::Left,
        CKeyCode::Right => KeyCode::Right,
        _ => KeyCode::Other,
    };
    let modifiers = KeyMods {
        ctrl: key.modifiers.contains(CMods::CONTROL),
        shift: key.modifiers.contains(CMods::SHIFT),
    };
    KeyPress { code, modifiers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Result, WslError};
    use crate::wsl::{OnlineDistro, RawDistroRow};
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::Mutex;

    /// A backend that records calls and can be configured to fail.
    #[derive(Default)]
    struct MockBackend {
        fail: bool,
        calls: Mutex<Vec<String>>,
    }

    impl MockBackend {
        fn record(&self, call: String) -> Result<()> {
            self.calls.lock().unwrap().push(call);
            if self.fail {
                Err(WslError::Command {
                    args: vec![],
                    message: "mock failure".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[async_trait]
    impl WslBackend for MockBackend {
        async fn list_verbose(&self) -> Result<Vec<RawDistroRow>> {
            Ok(vec![])
        }
        async fn list_running(&self) -> Result<Vec<String>> {
            Ok(vec![])
        }
        async fn start(&self, name: &str) -> Result<()> {
            self.record(format!("start {name}"))
        }
        async fn terminate(&self, name: &str) -> Result<()> {
            self.record(format!("terminate {name}"))
        }
        async fn shutdown(&self) -> Result<()> {
            self.record("shutdown".to_string())
        }
        async fn set_default(&self, name: &str) -> Result<()> {
            self.record(format!("set_default {name}"))
        }
        async fn unregister(&self, name: &str) -> Result<()> {
            self.record(format!("unregister {name}"))
        }
        async fn list_online(&self) -> Result<Vec<OnlineDistro>> {
            Ok(vec![])
        }
        async fn export(&self, name: &str, _path: &Path) -> Result<()> {
            self.record(format!("export {name}"))
        }
        async fn import(&self, name: &str, _dir: &Path, _tar: &Path) -> Result<()> {
            self.record(format!("import {name}"))
        }
        async fn install(&self, name: &str) -> Result<()> {
            self.record(format!("install {name}"))
        }
        async fn inner_disk(&self, _distro: &str) -> Result<Option<(u64, u64)>> {
            Ok(None)
        }
        async fn read_conf(&self, _distro: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn write_conf(&self, distro: &str, _content: &str) -> Result<()> {
            self.record(format!("write_conf {distro}"))
        }
    }

    #[tokio::test]
    async fn lifecycle_success_calls_backend_and_maps_to_opdone() {
        let mock = MockBackend::default();
        let action = run_lifecycle(
            &mock,
            LifecycleOp::Terminate("Debian".to_string()),
            Lang::En,
        )
        .await;
        assert!(matches!(action, Action::OpDone(_)));
        assert_eq!(
            mock.calls.lock().unwrap().as_slice(),
            &["terminate Debian".to_string()]
        );
    }

    #[tokio::test]
    async fn lifecycle_failure_maps_to_opfailed() {
        let mock = MockBackend {
            fail: true,
            ..Default::default()
        };
        let action = run_lifecycle(&mock, LifecycleOp::Shutdown, Lang::En).await;
        assert!(matches!(action, Action::OpFailed(_)));
        assert_eq!(
            mock.calls.lock().unwrap().as_slice(),
            &["shutdown".to_string()]
        );
    }
}
