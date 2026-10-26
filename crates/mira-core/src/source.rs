//! Platform data-source traits.
//!
//! Each OS implements these in its platform crate. A *source* is event-driven:
//! [`Source::run`] takes ownership and loops forever, publishing [`AppEvent`]s
//! onto the bus as the OS notifies it — it must not poll. Media players also
//! expose a [`MediaControl`] handle so the UI can drive playback.
//!
//! `MediaControl` is boxed as `dyn` by the hosts, so it uses `async_trait` to
//! stay object-safe. The marker source traits carry no async methods.

use async_trait::async_trait;

use crate::error::Result;
use crate::event::EventBus;

/// A long-running, event-driven data source. `run` returns only on fatal error
/// or shutdown; it publishes onto `bus` as the OS signals changes.
#[async_trait]
pub trait Source: Send {
    /// Human-readable name for logs.
    fn name(&self) -> &'static str;

    /// Run the source to completion. Implementations subscribe to OS events and
    /// publish [`crate::event::AppEvent`]s; they must avoid busy-polling.
    async fn run(self: Box<Self>, bus: EventBus) -> Result<()>;
}

/// Control surface for the active media player.
#[async_trait]
pub trait MediaControl: Send + Sync {
    async fn play_pause(&self) -> Result<()>;
    async fn next(&self) -> Result<()>;
    async fn previous(&self) -> Result<()>;
    /// Seek by a relative offset in microseconds (may be negative).
    async fn seek(&self, offset_micros: i64) -> Result<()>;
    /// Seek to an absolute position in microseconds.
    async fn set_position(&self, micros: i64) -> Result<()>;
}

// The remaining traits are *marker* super-traits of [`Source`]. They exist so a
// host can name "the power source" etc. in its wiring and so platform crates
// document which concern each type covers. They add no methods: every source
// speaks to the host the same way, through the bus.

/// Media playback + metadata (Windows: GSMTC, Linux: MPRIS).
pub trait MediaSource: Source {}
/// Battery / charger (Windows: WM_POWERBROADCAST, Linux: UPower).
pub trait PowerSource: Source {}
/// Network connectivity (Windows: NetworkInformation, Linux: NetworkManager).
pub trait NetworkSource: Source {}
/// Bluetooth audio devices (Windows: DeviceWatcher, Linux: BlueZ).
pub trait BluetoothSource: Source {}
/// Output volume (Windows: IAudioEndpointVolume, Linux: PipeWire/Pulse).
pub trait AudioSource: Source {}
/// Display brightness (Windows: WMI, Linux: UPower or /sys/class/backlight).
pub trait BrightnessSource: Source {}

/// Fullscreen detection. On Windows this lives in a source; on Linux/Wayland
/// the daemon cannot see fullscreen reliably, so the GNOME extension owns it
/// and this trait is unused there (documented in the D-Bus design).
pub trait FullscreenDetector: Source {}
