//! `wsl_manager_tui` — core library for the WSL Manager TUI.
//!
//! The crate is structured around a Model–View–Update (MVU) architecture:
//! pure, terminal-independent logic lives in [`app`], side effects are
//! described as commands and executed by [`runtime`], and rendering is
//! confined to [`ui`]. The thin `wslm` binary (`src/main.rs`) only wires
//! these together.
//!
//! Layers that must stay UI-independent (no terminal knowledge): [`wsl`],
//! [`registry`], [`metrics`], [`config`], [`app`], [`i18n`], [`prefs`].

pub mod app;
pub mod config;
pub mod error;
pub mod i18n;
pub mod metrics;
pub mod prefs;
pub mod registry;
pub mod runtime;
pub mod ui;
pub mod wsl;
