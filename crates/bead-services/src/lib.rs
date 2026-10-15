// SPDX-License-Identifier: MIT OR Apache-2.0
//! Event-driven desktop services, independent of views and compositor transports.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub connected: bool,
    pub default_sink: Option<String>,
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    #[serde(default)]
    pub default_source: Option<String>,
    #[serde(default)]
    pub mic_volume: Option<f64>,
    #[serde(default)]
    pub mic_muted: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub available: bool,
    pub percentage: Option<f64>,
    pub charging: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub available: bool,
    pub connected: bool,
    pub wireless_enabled: bool,
    pub name: String,
    pub strength: Option<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bluetooth {
    pub available: bool,
    pub powered: bool,
    pub connected_devices: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MenuEntry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub separator: bool,
    pub checked: Option<bool>,
    pub children: Vec<MenuEntry>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrayItem {
    pub id: String,
    pub title: String,
    pub visible: bool,
    pub item_is_menu: bool,
    pub icon_name: String,
    pub icon_theme_path: String,
    pub icon_pixmap: Vec<(i32, i32, Vec<u8>)>,
    pub attention: bool,
    pub menu: Vec<MenuEntry>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Origin order survives concurrent audio/bus callback delivery.
    #[serde(default)]
    pub revision: u64,
    pub clock: String,
    pub audio: Audio,
    pub battery: Battery,
    pub network: Network,
    pub bluetooth: Bluetooth,
    pub tray: Vec<TrayItem>,
}

mod audio;
mod bus;
pub mod files;
pub mod ipc;
pub mod process;
mod runtime;
pub use runtime::{Action, Config, Desktop, Subscription};

#[cfg(test)]
mod tests;

pub mod notification_bus;
pub mod notifications;
