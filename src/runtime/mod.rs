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

use crate::app::{update, Action, Command, Event, Model};
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
    match command {
        Command::RefreshList => {
            let backend = Arc::clone(backend);
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wsl::refresh(backend.as_ref()).await {
                    Ok(distros) => Action::Refreshed(distros),
                    Err(error) => Action::RefreshFailed(error.to_string()),
                };
                let _ = tx.send(action);
            });
        }
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
