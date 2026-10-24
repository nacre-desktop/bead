// SPDX-License-Identifier: GPL-3.0-only
//! B0086: the network popout's "Scan networks" (`network.scan`, N0061). The daemon answers when
//! NetworkManager has finished the scan, so the call in flight is the progress: the button is
//! dimmed (QML: `NacreControlActions.busy`) until it returns. The view is Luau; this is the edge.
use super::{
    Status, WifiControl,
    pairing::{advertised, call},
    tr, wifi_candidate,
};
use crate::nacre_source::{CallTicket, CallWake};
use bead::core::Value;
use nacre_protocol::core_services::{EntityId, NetworkState};
use std::collections::BTreeMap;

const SCAN: &str = "network.scan";

/// The Wi-Fi device a scan asks; the daemon refuses anything else.
fn wifi_device(state: &NetworkState) -> Option<&EntityId> {
    state.devices.order.iter().find(|id| {
        state
            .devices
            .items
            .get(*id)
            .is_some_and(|device| device.kind == "wifi")
    })
}

impl Status {
    /// Ask for a rescan on the first Wi-Fi device. Never retries; a scan already running or
    /// another change in flight sends nothing.
    pub fn network_scan(&self, wake: CallWake) {
        let Some(control) = &self.wifi_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = wifi_candidate(&snapshot, &client);
        let (Some(boot), Some(state), true) = (
            candidate.topic_boot.as_ref(),
            candidate.state.as_ref(),
            candidate.fresh,
        ) else {
            return;
        };
        let mut control = control.borrow_mut();
        let Some(device) = wifi_device(state).filter(|_| {
            state.wireless_enabled
                && control.scan.is_none()
                && control.pending.is_none()
                && control.link.is_none()
                && advertised(&client, SCAN).is_some()
        }) else {
            return;
        };
        control.link_message = None;
        match call(
            &client,
            boot,
            SCAN,
            serde_json::json!({"device": device.0}),
            Some(wake),
        ) {
            Ok(ticket) => control.scan = Some(ticket),
            Err(_) => control.link_message = Some(tr("bead.panel.network-scan-failed", &[])),
        }
    }
}

/// Take a finished scan, then publish its stores.
pub(super) fn project(
    control: &mut WifiControl,
    state: Option<&NetworkState>,
) -> BTreeMap<String, Value> {
    if let Some(result) = control.scan.as_mut().and_then(CallTicket::try_take) {
        control.scan = None;
        if result.is_err() {
            control.link_message = Some(tr("bead.panel.network-scan-failed", &[]));
        }
    }
    let scanning = control.scan.is_some();
    let enabled = !scanning
        && control.pending.is_none()
        && control.link.is_none()
        && state.is_some_and(|state| state.wireless_enabled && wifi_device(state).is_some())
        && advertised(&control.client, SCAN).is_some();
    BTreeMap::from([
        ("network.scan_enabled".into(), Value::Bool(enabled)),
        ("network.scanning".into(), Value::Bool(scanning)),
        (
            "network.scan_label".into(),
            Value::Text(tr(
                if scanning {
                    "bead.panel.network-scanning"
                } else {
                    "bead.panel.network-scan"
                },
                &[],
            )),
        ),
    ])
}
