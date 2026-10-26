//! UPower battery / charger source (system bus).
//!
//! We track the "display device" (UPower's aggregate battery) for percentage
//! and state, and the UPower root's `OnBattery` for AC transitions, turning the
//! latter into the charger-connected / charger-disconnected alerts.

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::Connection;

use mira_core::error::{Error, Result};
use mira_core::event::{AppEvent, EventBus};
use mira_core::model::{Power, PowerState, SystemEvent};
use mira_core::source::{PowerSource, Source};

const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    fn get_display_device(&self) -> zbus::Result<zbus::OwnedObjectPath>;

    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait UPowerDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<u32>;
}

/// Battery / charger source. Connects to the system bus on [`Source::run`].
#[derive(Default)]
pub struct UPowerSource;

impl UPowerSource {
    pub fn new() -> Self {
        UPowerSource
    }
}

impl PowerSource for UPowerSource {}

#[async_trait]
impl Source for UPowerSource {
    fn name(&self) -> &'static str {
        "upower"
    }

    async fn run(self: Box<Self>, bus: EventBus) -> Result<()> {
        let conn = Connection::system().await.map_err(err)?;
        let up = UPowerProxy::new(&conn).await.map_err(err)?;

        let dev_path = up
            .get_display_device()
            .await
            .map(|p| p.into_inner())
            .unwrap_or_else(|_| {
                zbus::ObjectPath::try_from(DISPLAY_DEVICE_PATH)
                    .expect("static display-device path is valid")
            });
        let dev = UPowerDeviceProxy::builder(&conn)
            .path(dev_path)
            .build()
            .await
            .map_err(err)?;

        let mut on_battery = up.on_battery().await.unwrap_or(false);
        bus.publish(AppEvent::Power(read_power(&up, &dev).await));

        let mut pct = dev.receive_percentage_changed().await;
        let mut state = dev.receive_state_changed().await;
        let mut onbat = up.receive_on_battery_changed().await;

        loop {
            tokio::select! {
                Some(_) = pct.next() => {
                    bus.publish(AppEvent::Power(read_power(&up, &dev).await));
                }
                Some(_) = state.next() => {
                    bus.publish(AppEvent::Power(read_power(&up, &dev).await));
                }
                Some(change) = onbat.next() => {
                    let new = change.get().await.unwrap_or(on_battery);
                    if new != on_battery {
                        let percentage = dev.percentage().await.unwrap_or(0.0);
                        let event = if on_battery && !new {
                            SystemEvent::ChargerConnected { percentage }
                        } else {
                            SystemEvent::ChargerDisconnected { percentage }
                        };
                        bus.publish(AppEvent::System(event));
                        on_battery = new;
                    }
                    bus.publish(AppEvent::Power(read_power(&up, &dev).await));
                }
                else => break,
            }
        }
        Ok(())
    }
}

async fn read_power(up: &UPowerProxy<'_>, dev: &UPowerDeviceProxy<'_>) -> Power {
    let present = dev.is_present().await.unwrap_or(false);
    let kind = dev.kind().await.unwrap_or(0);
    Power {
        // UPower device type 2 == Battery.
        has_battery: present && kind == 2,
        percentage: dev.percentage().await.unwrap_or(0.0),
        state: map_state(dev.state().await.unwrap_or(0)),
        on_ac: !up.on_battery().await.unwrap_or(false),
    }
}

fn map_state(raw: u32) -> PowerState {
    match raw {
        1 => PowerState::Charging,
        2 => PowerState::Discharging,
        3 => PowerState::Empty,
        4 => PowerState::FullyCharged,
        _ => PowerState::Unknown,
    }
}

fn err(e: zbus::Error) -> Error {
    Error::other(format!("upower: {e}"))
}
