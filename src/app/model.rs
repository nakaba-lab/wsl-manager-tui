//! The application model (state). It grows with each milestone; for now it only
//! carries the quit flag and a tick counter used to show the runtime is live.

/// The full application state. Rendered by [`crate::ui`] and mutated only by
/// [`crate::app::update`].
#[derive(Debug, Default, Clone)]
pub struct Model {
    /// Set to true when the app should exit its event loop.
    pub should_quit: bool,
    /// Number of timer ticks observed (placeholder liveness indicator).
    pub ticks: u64,
}
