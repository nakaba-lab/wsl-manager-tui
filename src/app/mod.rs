//! Application core: the `Model`, the pure `update` reducer, and the message
//! types that drive the MVU loop. This module is the main test surface and
//! must remain terminal- and IO-independent.

pub mod message;
pub mod model;
pub mod update;

pub use message::{Action, Command, Event};
pub use model::Model;
pub use update::update;
