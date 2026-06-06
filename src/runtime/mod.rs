//! Runtime: the async event loop and the terminal wrapper.
//!
//! The loop multiplexes terminal events (via crossterm's `EventStream`) and a
//! timer tick with `tokio::select!`, translates them into [`Action`]s, and runs
//! them through the pure [`crate::app::update`] reducer. An action channel for
//! results of async side effects is added in M2.

mod tui;

pub use tui::{install_panic_hook, Tui};

use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;

use crate::app::{update, Action, Event, Model};
use crate::ui;

/// Default tick interval for the skeleton runtime (made configurable via prefs
/// in M8).
const TICK: Duration = Duration::from_secs(1);

/// Set up the terminal and run the event loop to completion, restoring the
/// terminal afterwards regardless of how the loop ended.
pub async fn run() -> Result<()> {
    let mut model = Model::default();
    let mut tui = Tui::new()?;
    tui.enter()?;
    let result = event_loop(&mut tui, &mut model).await;
    let _ = tui.exit();
    result
}

async fn event_loop(tui: &mut Tui, model: &mut Model) -> Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(TICK);

    loop {
        tui.draw(|f| ui::view(f, model))?;
        if model.should_quit {
            break;
        }

        tokio::select! {
            maybe_event = events.next() => {
                if let Some(Ok(event)) = maybe_event {
                    if let Some(action) = map_event(event) {
                        update(model, action);
                    }
                }
            }
            _ = tick.tick() => {
                update(model, Action::Event(Event::Tick));
            }
        }
    }
    Ok(())
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
