// SPDX-License-Identifier: GPL-3.0-only
//! B0062: the Bluetooth device list of the popout from the `bluetooth` topic (N0047): rows with
//! battery, the Find devices scan (`bluetooth.set_discovery`, which the daemon stops by itself
//! after 60 s) and Connect/Disconnect (`bluetooth.connect`/`.disconnect`). Pairing itself is
//! `pairing.rs` (B0055). The view is Luau; this is the Nacre edge that knows the contract.
use super::{
    BluetoothCandidate, BluetoothControl, Status,
    pairing::{advertised, call, detach},
    tr,
};
use crate::nacre_source::{CallTicket, CallWake, Client};
use bead::core::Value;
use nacre_protocol::core_services::{BluetoothDevice, BluetoothState, EntityId};
use std::collections::BTreeMap;

const MAX_DEVICE_ROWS: usize = 5;
const SET_DISCOVERY: &str = "bluetooth.set_discovery";

/// One connect or disconnect in flight; the row is busy until the daemon answers.
pub(super) struct DeviceOp {
    device: String,
    connect: bool,
    ticket: CallTicket,
}

/// Closing the panel stops a scan this panel started (the daemon would stop it after 60 s anyway).
impl Drop for BluetoothControl {
    fn drop(&mut self) {
        if let Some((boot, adapter, _)) = self.scan_owned.take() {
            stop_scan(&self.client, &boot, &adapter);
        }
    }
}
fn stop_scan(client: &Client, boot: &str, adapter: &str) {
    let stop = serde_json::json!({"adapter": adapter, "discovering": false});
    if let Ok(ticket) = call(client, boot, SET_DISCOVERY, stop, None) {
        detach(ticket);
    }
}
/// Stop the scan before a pairing or connection (BlueZ is more reliable without one).
pub(super) fn quiet_scan(control: &mut BluetoothControl, candidate: &BluetoothCandidate) {
    let (Some(boot), Some(id), Some(adapter)) = (
        candidate.topic_boot.as_ref(),
        candidate.adapter_id.as_ref(),
        candidate.adapter.as_ref(),
    ) else {
        return;
    };
    if adapter.discovering {
        stop_scan(&control.client, boot, id);
    }
    control.scan_owned = None;
}

fn link_failure(code: &str) -> &'static str {
    match code {
        "busy" => "bead.panel.bluetooth-link-busy",
        "timeout" => "bead.panel.bluetooth-link-timeout",
        "stale_revision" => "bead.panel.bluetooth-pair-gone",
        _ => "bead.panel.bluetooth-link-failed",
    }
}

fn glyph(icon: Option<&str>) -> &'static str {
    let icon = icon.unwrap_or("");
    if ["headset", "headphone", "audio"]
        .iter()
        .any(|word| icon.contains(word))
    {
        "headphones"
    } else if icon.contains("keyboard") {
        "keyboard"
    } else if icon.contains("mouse") {
        "mouse"
    } else if icon.contains("phone") {
        "smartphone"
    } else {
        "bluetooth"
    }
}
/// The battery glyph of the QML list: under 20 %, under 60 %, else full.
fn battery_glyph(percent: u8) -> &'static str {
    match percent {
        0..20 => "battery_1_bar",
        20..60 => "battery_4_bar",
        _ => "battery_full",
    }
}

/// What the rows may do right now.
struct Rules<'a> {
    /// Fresh topic, adapter on, and no other operation running.
    idle: bool,
    can_pair: bool,
    can_connect: bool,
    can_disconnect: bool,
    busy: Option<&'a DeviceOp>,
}

/// QML service order: connected first, then name and stable id.
fn rows(state: &BluetoothState, rules: &Rules) -> Vec<serde_json::Value> {
    let mut devices: Vec<(&EntityId, &BluetoothDevice)> = state
        .devices
        .order
        .iter()
        .filter_map(|id| state.devices.items.get(id).map(|device| (id, device)))
        .collect();
    let rank = |d: &BluetoothDevice| !d.connected;
    devices.sort_by(|a, b| {
        (rank(a.1), a.1.name.to_lowercase(), &a.0.0).cmp(&(
            rank(b.1),
            b.1.name.to_lowercase(),
            &b.0.0,
        ))
    });
    devices.truncate(MAX_DEVICE_ROWS);
    devices
        .into_iter()
        .map(|(id, device)| {
            let caption = match device.battery {
                Some(percent) => format!("{percent}%"),
                None if device.connected => tr("bead.panel.bluetooth-device-connected-state", &[]),
                None if device.paired => tr("bead.panel.bluetooth-device-paired", &[]),
                None => tr("bead.panel.bluetooth-device-new", &[]),
            };
            let busy = rules.busy.filter(|op| op.device == id.0);
            let (kind, action, enabled) = if device.pairing {
                ("pair", "bead.panel.bluetooth-device-pairing", false)
            } else if let Some(op) = busy {
                let id = if op.connect {
                    "bead.panel.bluetooth-device-connecting"
                } else {
                    "bead.panel.bluetooth-device-disconnecting"
                };
                ("link", id, false)
            } else if device.connected {
                (
                    "link",
                    "bead.panel.bluetooth-disconnect",
                    rules.idle && rules.can_disconnect,
                )
            } else if device.paired {
                (
                    "link",
                    "bead.panel.bluetooth-connect",
                    rules.idle && rules.can_connect,
                )
            } else {
                (
                    "pair",
                    "bead.panel.bluetooth-pair",
                    rules.idle && rules.can_pair,
                )
            };
            serde_json::json!({
                "id": format!("dev:{}", id.0),
                "label": if device.name.is_empty() {
                    tr("bead.panel.bluetooth-device-unnamed", &[])
                } else {
                    super::line(&device.name, 24)
                },
                "caption": caption,
                "battery": device.battery.map_or("", battery_glyph),
                "icon": glyph(device.icon.as_deref()),
                "kind": kind,
                "action": tr(action, &[]),
                "active": device.connected,
                "enabled": enabled,
            })
        })
        .collect()
}

/// Take finished calls, then publish the list stores.
pub(super) fn project(
    control: &mut BluetoothControl,
    candidate: &BluetoothCandidate,
) -> BTreeMap<String, Value> {
    if let Some(result) = control.scan.as_mut().and_then(CallTicket::try_take) {
        control.scan = None;
        if result.is_err() {
            control.scan_owned = None;
            control.notice = Some("bead.panel.bluetooth-scan-failed");
        }
    }
    if let Some(result) = control.link.as_mut().and_then(|op| op.ticket.try_take()) {
        control.link = None;
        if let Err(error) = result {
            control.notice = Some(link_failure(&error.code));
        }
    }
    let powered = candidate.adapter.as_ref().is_some_and(|a| a.powered);
    // A scan that ran and ended (the daemon's 60 s stop) needs no stop when the panel closes.
    let running = candidate.adapter.as_ref().is_some_and(|a| a.discovering);
    if let Some(owned) = control.scan_owned.as_mut() {
        if running {
            owned.2 = true;
        } else if owned.2 && control.scan.is_none() {
            control.scan_owned = None;
        }
    }
    let idle = candidate.fresh
        && powered
        && control.flow.is_none()
        && control.pending.is_none()
        && control.link.is_none();
    let rules = Rules {
        idle,
        can_pair: advertised(&control.client, "bluetooth.pair").is_some(),
        can_connect: advertised(&control.client, "bluetooth.connect").is_some(),
        can_disconnect: advertised(&control.client, "bluetooth.disconnect").is_some(),
        busy: control.link.as_ref(),
    };
    let state = candidate.state.as_ref().filter(|_| candidate.fresh);
    let devices = state.map(|state| rows(state, &rules)).unwrap_or_default();
    let scanning = control.scan.is_some()
        || candidate
            .adapter
            .as_ref()
            .is_some_and(|adapter| adapter.discovering);
    let can_scan = idle
        && !scanning
        && candidate.adapter_id.is_some()
        && advertised(&control.client, SET_DISCOVERY).is_some();
    let empty = match state {
        None => tr("bead.panel.bluetooth-unavailable", &[]),
        Some(state) if state.adapters.items.is_empty() => {
            tr("bead.panel.bluetooth-no-adapter", &[])
        }
        Some(_) if !powered && candidate.adapter_id.is_some() => {
            tr("bead.panel.bluetooth-is-off", &[])
        }
        Some(_) if devices.is_empty() => tr("bead.panel.bluetooth-no-devices", &[]),
        Some(_) => String::new(),
    };
    BTreeMap::from([
        ("bluetooth.devices".into(), Value::list(devices)),
        (
            "bluetooth.list_message".into(),
            Value::Text(control.notice.map_or(String::new(), |id| tr(id, &[]))),
        ),
        (
            "bluetooth.scan_label".into(),
            Value::Text(tr(
                if scanning {
                    "bead.panel.bluetooth-scanning"
                } else {
                    "bead.panel.bluetooth-scan"
                },
                &[],
            )),
        ),
        ("bluetooth.scan_enabled".into(), Value::Bool(can_scan)),
        ("bluetooth.scanning".into(), Value::Bool(scanning)),
        ("bluetooth.empty_text".into(), Value::Text(empty)),
    ])
}

impl Status {
    /// Find devices: start a discovery on the sole adapter. Never retries.
    pub fn bluetooth_scan(&self, wake: CallWake) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = super::bluetooth_candidate(&snapshot, &client);
        let mut control = control.borrow_mut();
        let (Some(boot), Some(id), Some(adapter), true) = (
            candidate.topic_boot.as_ref(),
            candidate.adapter_id.as_ref(),
            candidate.adapter.as_ref(),
            candidate.fresh,
        ) else {
            return;
        };
        if !adapter.powered
            || adapter.discovering
            || control.scan.is_some()
            || control.flow.is_some()
            || control.pending.is_some()
            || control.link.is_some()
            || advertised(&client, SET_DISCOVERY).is_none()
        {
            return;
        }
        control.notice = None;
        let start = serde_json::json!({"adapter": id, "discovering": true});
        match call(&client, boot, SET_DISCOVERY, start, Some(wake)) {
            Ok(ticket) => {
                control.scan = Some(ticket);
                control.scan_owned = Some((boot.clone(), id.clone(), false));
            }
            Err(_) => control.notice = Some("bead.panel.bluetooth-scan-failed"),
        }
    }
    /// Connect or disconnect the paired device behind row `key` (`dev:<id>`), chosen from the
    /// device's state in the current fresh topic.
    pub fn bluetooth_link(&self, key: &str, wake: CallWake) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = super::bluetooth_candidate(&snapshot, &client);
        let mut control = control.borrow_mut();
        if control.flow.is_some() || control.pending.is_some() || control.link.is_some() {
            return;
        }
        let (Some(boot), Some(state), true) = (
            candidate.topic_boot.as_ref(),
            candidate.state.as_ref(),
            candidate.fresh,
        ) else {
            return;
        };
        let Some(id) = key.strip_prefix("dev:") else {
            return;
        };
        control.notice = None;
        let device = state
            .devices
            .items
            .get(&EntityId(id.into()))
            .filter(|device| device.paired || device.connected);
        let Some(device) = device else {
            control.notice = Some("bead.panel.bluetooth-pair-gone");
            return;
        };
        let powered = state
            .adapters
            .items
            .get(&device.adapter)
            .is_some_and(|adapter| adapter.powered);
        let connect = !device.connected;
        let method = if connect {
            "bluetooth.connect"
        } else {
            "bluetooth.disconnect"
        };
        if !powered || advertised(&client, method).is_none() {
            return;
        }
        quiet_scan(&mut control, &candidate);
        match call(
            &client,
            boot,
            method,
            serde_json::json!({"device": id}),
            Some(wake),
        ) {
            Ok(ticket) => {
                control.link = Some(DeviceOp {
                    device: id.into(),
                    connect,
                    ticket,
                })
            }
            Err(_) => control.notice = Some("bead.panel.bluetooth-link-failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_follow_the_bluez_icon_names() {
        for (icon, glyph_name) in [
            ("audio-headset", "headphones"),
            ("audio-card", "headphones"),
            ("input-keyboard", "keyboard"),
            ("input-mouse", "mouse"),
            ("phone", "smartphone"),
            ("computer", "bluetooth"),
        ] {
            assert_eq!(glyph(Some(icon)), glyph_name);
        }
        assert_eq!(glyph(None), "bluetooth");
    }
    #[test]
    fn battery_glyphs_use_the_qml_thresholds() {
        for (percent, glyph_name) in [
            (0, "battery_1_bar"),
            (19, "battery_1_bar"),
            (20, "battery_4_bar"),
            (59, "battery_4_bar"),
            (60, "battery_full"),
            (100, "battery_full"),
        ] {
            assert_eq!(battery_glyph(percent), glyph_name);
        }
    }
    #[test]
    fn link_failures_map_to_fixed_messages() {
        assert_eq!(link_failure("busy"), "bead.panel.bluetooth-link-busy");
        assert_eq!(link_failure("timeout"), "bead.panel.bluetooth-link-timeout");
        assert_eq!(
            link_failure("stale_revision"),
            "bead.panel.bluetooth-pair-gone"
        );
        assert_eq!(
            link_failure("unavailable"),
            "bead.panel.bluetooth-link-failed"
        );
    }
}
