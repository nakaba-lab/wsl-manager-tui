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

use crate::app::{update, Action, Command, Event, LifecycleOp, Model};
use crate::ui;
use crate::wsl::{self, RealWslBackend, WslBackend};

/// Polling interval for the distro list (made configurable via prefs in M8).
const TICK: Duration = Duration::from_secs(2);

/// Set up the terminal and run the event loop to completion, restoring the
/// terminal afterwards regardless of how the loop ended.
pub async fn run() -> Result<()> {
    let backend: Arc<dyn WslBackend> = Arc::new(RealWslBackend);
    let mut model = Model::default();
    let mut tui = Tui::new()?;
    tui.enter()?;
    let result = event_loop(&mut tui, &mut model, backend).await;
    let _ = tui.exit();
    result
}

async fn event_loop(tui: &mut Tui, model: &mut Model, backend: Arc<dyn WslBackend>) -> Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    // Kick off the initial load immediately.
    dispatch(Command::RefreshList, &backend, &action_tx);

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
            Some(action) = action_rx.recv() => action,
        };

        for command in update(model, action) {
            dispatch(command, &backend, &action_tx);
        }
    }
    Ok(())
}

/// Execute a command as an async task, sending the resulting action back over
/// the channel.
fn dispatch(command: Command, backend: &Arc<dyn WslBackend>, tx: &UnboundedSender<Action>) {
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
        Command::Lifecycle(op) => {
            tokio::spawn(async move {
                let action = run_lifecycle(backend.as_ref(), op).await;
                let _ = tx.send(action);
            });
        }
    }
}

/// Run a lifecycle operation through the backend and map the outcome to an
/// [`Action`].
async fn run_lifecycle(backend: &dyn WslBackend, op: LifecycleOp) -> Action {
    let result = match &op {
        LifecycleOp::Start(name) => backend.start(name).await,
        LifecycleOp::Terminate(name) => backend.terminate(name).await,
        LifecycleOp::Shutdown => backend.shutdown().await,
        LifecycleOp::SetDefault(name) => backend.set_default(name).await,
        LifecycleOp::Unregister(name) => backend.unregister(name).await,
    };
    match result {
        Ok(()) => Action::OpDone(op.success_label()),
        Err(error) => Action::OpFailed(format!("{} failed: {error}", op.verb())),
    }
}

/// Translate a crossterm event into an [`Action`], dropping anything the app
/// does not act on. Key *release*/*repeat* events (common on Windows) are
/// filtered so a single press is not handled twice.
fn map_event(event: CrosstermEvent) -> Option<Action> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
            Some(Action::Event(Event::Key(key)))
        }
        CrosstermEvent::Resize(w, h) => Some(Action::Event(Event::Resize(w, h))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Result, WslError};
    use crate::wsl::RawDistroRow;
    use async_trait::async_trait;
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
    }

    #[tokio::test]
    async fn lifecycle_success_calls_backend_and_maps_to_opdone() {
        let mock = MockBackend::default();
        let action = run_lifecycle(&mock, LifecycleOp::Terminate("Debian".to_string())).await;
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
        let action = run_lifecycle(&mock, LifecycleOp::Shutdown).await;
        assert!(matches!(action, Action::OpFailed(_)));
        assert_eq!(
            mock.calls.lock().unwrap().as_slice(),
            &["shutdown".to_string()]
        );
    }
}
