//! Mira core: all the platform-independent logic the notch needs.
//!
//! This crate deliberately knows nothing about Windows, Linux, D-Bus, Direct2D
//! or Clutter. Platform crates (`mira-platform-win`, `mira-platform-linux`)
//! implement the [`source`] traits and feed [`event::AppEvent`]s into the
//! [`event::EventBus`]; the hosts (`mira-win`, `mira-daemon`) own the bus and
//! the UI/transport.
//!
//! Design principles encoded here:
//! * **Event-driven** — state changes arrive as events; nothing polls.
//! * **Local-first** — the only network access lives in [`lrclib`] (lrclib.net).
//! * **Lightweight** — no heavy runtime; a current-thread Tokio runtime is enough.

pub mod app;
pub mod error;
pub mod event;
pub mod lrclib;
pub mod lyrics;
pub mod model;
pub mod natural;
pub mod pomodoro;
pub mod reminder;
pub mod source;
pub mod store;
pub mod tokens;
pub mod track;

pub use error::{Error, Result};
