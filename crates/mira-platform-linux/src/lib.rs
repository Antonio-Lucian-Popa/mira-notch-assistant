//! Linux data sources for Mira, implemented over D-Bus.
//!
//! * [`MprisSource`] / [`MprisControl`] — media metadata and playback control
//!   via MPRIS (`org.mpris.MediaPlayer2.*`) on the **session** bus.
//! * [`UPowerSource`] — battery and charger state via UPower on the **system**
//!   bus.
//!
//! The whole crate is gated to Linux so the workspace still builds on Windows
//! (where it compiles to nothing).
#![cfg(target_os = "linux")]

mod mpris;
mod upower;

pub use mpris::{new_mpris, MprisControl, MprisSource};
pub use upower::UPowerSource;
