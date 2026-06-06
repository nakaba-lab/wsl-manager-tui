//! Application core: the `Model`, the pure `update` reducer, and the message
//! and modal types that drive the MVU loop. This module is the main test
//! surface and must remain terminal- and IO-independent.

pub mod message;
pub mod modal;
pub mod model;
pub mod update;

pub use message::{Action, Command, Event, LifecycleOp};
pub use modal::{
    Confirm, FormKind, FormState, InstallPickState, Modal, ProgressState, TextField, TypedConfirm,
};
pub use model::Model;
pub use update::update;
