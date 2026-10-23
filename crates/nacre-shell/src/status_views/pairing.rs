// SPDX-License-Identifier: GPL-3.0-only
//! B0055: Bluetooth device list and pairing flow for the Bluetooth popout.
//! The view is Luau (`shell/status/bluetooth.luau` over kit components); this module is the Nacre
//! edge: it knows the daemon contract (N0045: topic `bluetooth.pairing`, `bluetooth.pair`,
//! `.pair.respond`, `.pair.cancel`), runs the flow and publishes it as stores.
//! A typed PIN or passkey lives only in the `bluetooth.secret` input store until the respond call
//! is built; the host clears it after every action. It is never put in a message, store
//! projection or log, and the `.secret` suffix keeps it out of traces and saved state.
use super::{BluetoothCandidate, BluetoothControl, Status, devices, join, line, tr};
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Snapshot, Source, Subscription},
};
use nacre_protocol::core_services::{
    BluetoothPairRespond, BluetoothPairingState, CoreSnapshot, DeviceTarget, EntityId, PairingKind,
    pairing_text_valid,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The single store that holds a typed PIN or passkey.
pub(crate) const SECRET_STORE: &str = "bluetooth.secret";
const PAIR_CALL: &str = "bluetooth.pair";
/// `begin_call` refuses deadlines above two minutes; the daemon method may wait longer.
const MAX_CALL: Duration = Duration::from_secs(120);

/// The prompt the daemon waits on, with the local time it runs out.
struct Shown {
    kind: PairingKind,
    code: Option<String>,
    deadline: Instant,
}
enum Outcome {
    Running,
    /// Message id and whether Retry is offered.
    Failed(&'static str, bool),
}
/// One pairing started from this panel.
pub(super) struct Flow {
    client: Client,
    boot: String,
    device: String,
    name: String,
    wake: CallWake,
    /// The waiting `bluetooth.pair` call; `None` once it has finished.
    pair: Option<CallTicket>,
    respond: Option<CallTicket>,
    cancel: Option<CallTicket>,
    prompt: Option<Shown>,
    outcome: Outcome,
    /// The user cancelled or rejected: the view is back on the list and the result is ignored.
    leaving: bool,
    form_error: Option<&'static str>,
    /// Lease of `bluetooth.pairing`, held only while the pairing runs.
    topic: Option<(Arc<dyn Source>, Subscription)>,
    ticker: Option<Arc<AtomicBool>>,
    focus: bool,
}
impl Flow {
    fn stop_ticker(&mut self) {
        if let Some(stop) = self.ticker.take() {
            stop.store(true, Ordering::Release);
        }
    }
    /// Once per second while a prompt counts down, so the panel redraws the remaining time.
    fn start_ticker(&mut self) {
        self.stop_ticker();
        let stop = Arc::new(AtomicBool::new(false));
        self.ticker = Some(stop.clone());
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if stop.load(Ordering::Acquire) {
                    break;
                }
                wake();
            }
        });
    }
}
impl Drop for Flow {
    fn drop(&mut self) {
        self.stop_ticker();
        // A panel that closes mid-pairing must not leave the daemon waiting on a prompt nobody
        // sees: cancel it, and let sent calls finish.
        let pending = self.cancel.take().or_else(|| {
            self.pair.as_ref().and_then(|_| {
                call(
                    &self.client,
                    &self.boot,
                    "bluetooth.pair.cancel",
                    serde_json::json!({"device": self.device}),
                    None,
                )
                .ok()
            })
        });
        for ticket in [pending, self.respond.take()].into_iter().flatten() {
            detach(ticket);
        }
    }
}
/// Keep a sent call alive until it completes (a dropped queued ticket would cancel it).
pub(super) fn detach(mut ticket: CallTicket) {
    std::thread::spawn(move || {
        for _ in 0..150 {
            if ticket.try_take().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
}
pub(super) fn advertised(client: &Client, name: &str) -> Option<u32> {
    client
        .status()
        .methods
        .get(name)
        .filter(|method| method.v == 1)
        .map(|method| method.timeout)
}
pub(super) fn call(
    client: &Client,
    boot: &str,
    method: &str,
    params: serde_json::Value,
    wake: Option<CallWake>,
) -> bead::core::source::Result<CallTicket> {
    let timeout = advertised(client, method).unwrap_or(10_000);
    client.begin_call(
        method.into(),
        params,
        Duration::from_millis(u64::from(timeout)).min(MAX_CALL),
        CallGuard {
            boot: boot.into(),
            version: 1,
        },
        wake,
    )
}

/// Message id and Retry flag for a refused or failed `bluetooth.pair`.
fn failure(code: &str, message: &str) -> (&'static str, bool) {
    match (code, message) {
        ("permission_denied", "Pairing authentication failed") => {
            ("bead.panel.bluetooth-pair-wrong-code", true)
        }
        ("permission_denied", "Pairing was rejected") => {
            ("bead.panel.bluetooth-pair-rejected", false)
        }
        ("permission_denied", _) => ("bead.panel.bluetooth-pair-cancelled", false),
        ("timeout", _) => ("bead.panel.bluetooth-pair-timeout", true),
        ("busy", _) => ("bead.panel.bluetooth-pair-busy", true),
        ("stale_revision", _) => ("bead.panel.bluetooth-pair-gone", false),
        ("unavailable", _) => ("bead.panel.bluetooth-pair-unavailable", false),
        _ => ("bead.panel.bluetooth-pair-failed", true),
    }
}

/// The prompt addressed to `device`, when the topic is fresh and valid.
fn prompt_of(
    topic: &Snapshot,
    boot: &str,
    device: &str,
) -> Option<(PairingKind, Option<String>, u32)> {
    if !topic.connected || topic.epoch != boot {
        return None;
    }
    let state =
        serde_json::from_value::<BluetoothPairingState>(topic.data.as_ref().clone()).ok()?;
    if !state.health.available
        || state.health.stale
        || !CoreSnapshot::BluetoothPairing(state.clone()).valid()
    {
        return None;
    }
    let prompt = state.prompt?;
    (prompt.device.0 == device).then_some((prompt.kind, prompt.code, prompt.timeout_ms))
}

impl Status {
    /// Start pairing the unpaired device behind row `key` (`dev:<id>`).
    pub fn bluetooth_pair(&self, key: &str, wake: CallWake) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = super::bluetooth_candidate(&snapshot, &client);
        let mut control = control.borrow_mut();
        start(&mut control, &client, &candidate, key, wake);
    }
    /// Answer the open prompt. `accept == false` rejects (confirm and authorize) and leaves the
    /// flow. Returns whether the caller clears [`SECRET_STORE`]: always, except after a local
    /// check refused the entry, so the user can correct it.
    pub fn bluetooth_pair_respond(&self, accept: bool, stores: &Stores) -> bool {
        let Some(control) = &self.bluetooth_control else {
            return true;
        };
        let mut control = control.borrow_mut();
        let Some(flow) = control.flow.as_mut() else {
            return true;
        };
        let Some(kind) = flow
            .prompt
            .as_ref()
            .filter(|_| flow.respond.is_none())
            .map(|prompt| prompt.kind)
        else {
            return true;
        };
        flow.form_error = None;
        let device = EntityId(flow.device.clone());
        let secret = join::text(stores, SECRET_STORE);
        let answer = match (accept, kind) {
            (false, _) => BluetoothPairRespond {
                device,
                accept: false,
                pin: None,
                passkey: None,
            },
            (true, PairingKind::Confirm | PairingKind::Authorize) => BluetoothPairRespond {
                device,
                accept: true,
                pin: None,
                passkey: None,
            },
            (true, PairingKind::Pin) => {
                if !pairing_text_valid(&secret) {
                    flow.form_error = Some("bead.panel.bluetooth-pair-pin-invalid");
                    return false;
                }
                BluetoothPairRespond {
                    device,
                    accept: true,
                    pin: Some(secret),
                    passkey: None,
                }
            }
            (true, PairingKind::Passkey) => {
                let passkey = secret
                    .bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| secret.parse::<u32>().ok())
                    .flatten()
                    .filter(|value| *value <= 999_999);
                let Some(passkey) = passkey else {
                    flow.form_error = Some("bead.panel.bluetooth-pair-passkey-invalid");
                    return false;
                };
                BluetoothPairRespond {
                    device,
                    accept: true,
                    pin: None,
                    passkey: Some(passkey),
                }
            }
            // Nothing to answer: the user types the code on the device.
            (true, PairingKind::Display) => return true,
        };
        let params = serde_json::to_value(answer).unwrap_or_default();
        match call(
            &flow.client,
            &flow.boot,
            "bluetooth.pair.respond",
            params,
            Some(flow.wake.clone()),
        ) {
            Ok(ticket) => {
                flow.respond = Some(ticket);
                flow.leaving |= !accept;
            }
            Err(_) => flow.form_error = Some("bead.panel.bluetooth-pair-failed"),
        }
        true
    }
    /// Cancel a running pairing (back to the list at once), or close the error page.
    pub fn bluetooth_pair_cancel(&self) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let mut control = control.borrow_mut();
        let Some(flow) = control.flow.as_mut() else {
            return;
        };
        if flow.pair.is_none() || matches!(flow.outcome, Outcome::Failed(..)) {
            control.flow = None;
            return;
        }
        flow.leaving = true;
        flow.stop_ticker();
        if flow.cancel.is_none() {
            flow.cancel = call(
                &flow.client,
                &flow.boot,
                "bluetooth.pair.cancel",
                serde_json::json!({"device": flow.device}),
                Some(flow.wake.clone()),
            )
            .ok();
        }
    }
    /// Try again after a failure that offers it.
    pub fn bluetooth_pair_retry(&self, wake: CallWake) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = super::bluetooth_candidate(&snapshot, &client);
        let mut control = control.borrow_mut();
        let Some(Outcome::Failed(_, true)) = control.flow.as_ref().map(|flow| &flow.outcome) else {
            return;
        };
        let key = control
            .flow
            .take()
            .map(|flow| format!("dev:{}", flow.device));
        if let Some(key) = key {
            start(&mut control, &client, &candidate, &key, wake);
        }
    }
    /// The panel closes or Esc was pressed: whatever pairing runs is cancelled.
    pub fn bluetooth_pair_leave(&self) {
        if self
            .bluetooth_control
            .as_ref()
            .is_some_and(|control| control.borrow().flow.is_some())
        {
            self.bluetooth_pair_cancel();
        }
    }
    /// Whether the pair page is showing (Esc leaves it before it closes the panel).
    pub fn bluetooth_pairing_open(&self) -> bool {
        self.bluetooth_control.as_ref().is_some_and(|control| {
            control
                .borrow()
                .flow
                .as_ref()
                .is_some_and(|flow| !flow.leaving)
        })
    }
}

fn start(
    control: &mut BluetoothControl,
    client: &Client,
    candidate: &BluetoothCandidate,
    key: &str,
    wake: CallWake,
) {
    if control.flow.is_some() || control.pending.is_some() {
        return;
    }
    let (Some(boot), Some(state), true) = (
        candidate.topic_boot.as_ref(),
        candidate.state.as_ref(),
        candidate.fresh,
    ) else {
        return;
    };
    let Some(id) = key.strip_prefix("dev:").map(|id| EntityId(id.into())) else {
        return;
    };
    control.notice = None;
    let device = state.devices.items.get(&id).filter(|device| {
        !device.paired
            && !device.connected
            && !device.pairing
            && state
                .adapters
                .items
                .get(&device.adapter)
                .is_some_and(|adapter| adapter.powered)
    });
    let Some(device) = device else {
        control.notice = Some("bead.panel.bluetooth-pair-gone");
        return;
    };
    if advertised(client, PAIR_CALL).is_none()
        || advertised(client, "bluetooth.pair.respond").is_none()
        || advertised(client, "bluetooth.pair.cancel").is_none()
    {
        control.notice = Some("bead.panel.bluetooth-pair-unavailable");
        return;
    }
    devices::quiet_scan(control, candidate);
    // Watch the prompt topic first so no prompt is missed, then send the call that waits.
    let Ok(lease) = client.lease_topic("bluetooth.pairing") else {
        control.notice = Some("bead.panel.bluetooth-pair-unavailable");
        return;
    };
    let lease: Arc<dyn Source> = Arc::new(lease);
    let notify = wake.clone();
    let Ok(subscription) = lease.subscribe(Arc::new(move |_| notify())) else {
        control.notice = Some("bead.panel.bluetooth-pair-unavailable");
        return;
    };
    let ticket = call(
        client,
        boot,
        PAIR_CALL,
        serde_json::to_value(DeviceTarget { device: id.clone() }).unwrap_or_default(),
        Some(wake.clone()),
    );
    let Ok(ticket) = ticket else {
        control.notice = Some("bead.panel.bluetooth-pair-failed");
        return;
    };
    control.flow = Some(Flow {
        client: client.clone(),
        boot: boot.clone(),
        device: id.0,
        name: device.name.clone(),
        wake,
        pair: Some(ticket),
        respond: None,
        cancel: None,
        prompt: None,
        outcome: Outcome::Running,
        leaving: false,
        form_error: None,
        topic: Some((lease, subscription)),
        ticker: None,
        focus: false,
    });
}

/// Advance the flow (finished calls, the prompt in the topic) and publish the pairing stores.
pub(super) fn project(
    control: &mut BluetoothControl,
    candidate: &BluetoothCandidate,
) -> BTreeMap<String, Value> {
    settle(control);
    let focus = control
        .flow
        .as_mut()
        .is_some_and(|flow| std::mem::take(&mut flow.focus));
    let mut stores = devices::project(control, candidate);
    let flow = control.flow.as_ref().filter(|flow| !flow.leaving);
    let (kind, text, retry) = match flow.map(|flow| (flow, &flow.outcome)) {
        None => ("waiting", String::new(), false),
        Some((_, Outcome::Failed(id, retry))) => ("error", tr(id, &[]), *retry),
        Some((flow, Outcome::Running)) => match &flow.prompt {
            None => (
                "waiting",
                tr("bead.panel.bluetooth-pair-waiting", &[]),
                false,
            ),
            Some(shown) => {
                let code = shown.code.clone().unwrap_or_default();
                let args = [("code", code.as_str())];
                match shown.kind {
                    PairingKind::Pin => {
                        ("pin", tr("bead.panel.bluetooth-pair-enter-pin", &[]), false)
                    }
                    PairingKind::Passkey => (
                        "passkey",
                        tr("bead.panel.bluetooth-pair-enter-passkey", &[]),
                        false,
                    ),
                    PairingKind::Confirm => (
                        "confirm",
                        tr("bead.panel.bluetooth-pair-confirm-code", &args),
                        false,
                    ),
                    PairingKind::Authorize => (
                        "authorize",
                        tr("bead.panel.bluetooth-pair-authorize", &[]),
                        false,
                    ),
                    PairingKind::Display => (
                        "display",
                        tr("bead.panel.bluetooth-pair-display", &args),
                        false,
                    ),
                }
            }
        },
    };
    let countdown = flow
        .filter(|flow| matches!(flow.outcome, Outcome::Running))
        .and_then(|flow| flow.prompt.as_ref())
        .map_or(String::new(), |shown| {
            let left = shown
                .deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f64()
                .ceil() as u64;
            tr(
                "bead.panel.bluetooth-pair-countdown",
                &[("seconds", &left.to_string())],
            )
        });
    stores.extend([
        (
            "bluetooth.view".into(),
            Value::Text(if flow.is_some() { "pair" } else { "list" }.into()),
        ),
        (
            "bluetooth.pair_title".into(),
            Value::Text(flow.map_or(String::new(), |flow| {
                tr(
                    "bead.panel.bluetooth-pair-title",
                    &[("name", &line(&flow.name, 28))],
                )
            })),
        ),
        ("bluetooth.pair_kind".into(), Value::Text(kind.into())),
        ("bluetooth.pair_text".into(), Value::Text(text)),
        ("bluetooth.pair_countdown".into(), Value::Text(countdown)),
        (
            "bluetooth.pair_error".into(),
            Value::Text(
                flow.and_then(|flow| flow.form_error)
                    .map_or(String::new(), |id| tr(id, &[])),
            ),
        ),
        (
            "bluetooth.pair_busy".into(),
            Value::Bool(flow.is_some_and(|flow| flow.respond.is_some())),
        ),
        ("bluetooth.pair_retry".into(), Value::Bool(retry)),
    ]);
    if focus {
        stores.insert(
            "bead.focused".into(),
            Value::Text("status-popout.secret.field".into()),
        );
    }
    stores
}

/// Take finished calls and follow the prompt topic.
fn settle(control: &mut BluetoothControl) {
    let Some(flow) = control.flow.as_mut() else {
        return;
    };
    if let Some(result) = flow.cancel.as_mut().and_then(CallTicket::try_take) {
        let _ = result;
        flow.cancel = None;
    }
    if let Some(result) = flow.respond.as_mut().and_then(CallTicket::try_take) {
        flow.respond = None;
        flow.form_error = match result {
            Ok(_) => None,
            // The prompt moved on (or closed) before the answer arrived: the next state shows it.
            Err(error) if error.code == "stale_revision" => None,
            Err(error) if error.code == "validation" => {
                Some("bead.panel.bluetooth-pair-answer-invalid")
            }
            Err(_) => Some("bead.panel.bluetooth-pair-failed"),
        };
    }
    if let Some(result) = flow.pair.as_mut().and_then(CallTicket::try_take) {
        flow.pair = None;
        flow.stop_ticker();
        flow.prompt = None;
        flow.topic = None;
        match result {
            Ok(_) => {
                control.notice = (!flow.leaving).then_some("bead.panel.bluetooth-pair-done");
                control.flow = None;
            }
            Err(_) if flow.leaving => control.flow = None,
            Err(error) => {
                let (id, retry) = failure(&error.code, &error.message);
                flow.outcome = Outcome::Failed(id, retry);
            }
        }
        return;
    }
    if !matches!(flow.outcome, Outcome::Running) {
        return;
    }
    let topic = flow.topic.as_ref().map(|(source, _)| source.snapshot());
    let current = topic
        .as_deref()
        .and_then(|topic| prompt_of(topic, &flow.boot, &flow.device));
    match (current, flow.prompt.as_ref()) {
        (Some((kind, code, timeout_ms)), shown)
            if shown.is_none_or(|shown| shown.kind != kind || shown.code != code) =>
        {
            flow.prompt = Some(Shown {
                kind,
                code,
                deadline: Instant::now() + Duration::from_millis(u64::from(timeout_ms)),
            });
            flow.form_error = None;
            flow.focus = matches!(kind, PairingKind::Pin | PairingKind::Passkey);
            flow.start_ticker();
        }
        (None, Some(_)) => {
            flow.prompt = None;
            flow.stop_ticker();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_keep_their_retry_rule() {
        assert_eq!(
            failure("permission_denied", "Pairing authentication failed"),
            ("bead.panel.bluetooth-pair-wrong-code", true)
        );
        assert_eq!(
            failure("permission_denied", "Pairing was rejected"),
            ("bead.panel.bluetooth-pair-rejected", false)
        );
        assert_eq!(
            failure("permission_denied", "Authentication was cancelled"),
            ("bead.panel.bluetooth-pair-cancelled", false)
        );
        assert_eq!(
            failure("timeout", "x"),
            ("bead.panel.bluetooth-pair-timeout", true)
        );
        assert_eq!(
            failure("busy", "x"),
            ("bead.panel.bluetooth-pair-busy", true)
        );
        assert_eq!(
            failure("internal", "x"),
            ("bead.panel.bluetooth-pair-failed", true)
        );
    }
}
