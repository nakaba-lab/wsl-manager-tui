//! Runtime: the async event loop (`tokio::select!` over terminal events, a
//! tick timer, and an action channel), the `Tui` terminal RAII wrapper with
//! suspend/resume for inline shells, and the panic hook for terminal restore.
