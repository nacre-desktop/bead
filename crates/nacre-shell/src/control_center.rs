// SPDX-License-Identifier: GPL-3.0-only
//! B0049: the control-center host. Tiles, their order and their pages are Luau
//! (`shell/control/`); this adapter is the only code that names Nacre topics, methods and the
//! `control-center` store. It projects the *data sources* the tiles bind to (`cc.src`: value,
//! state and which verbs the daemon offers), the stored order (`cc.saved`) and the panels it can
//! open (`cc.views`), performs the published calls a tile asks for (`cc.do` = `verb:source[:arg]`)
//! and writes the edit draft (`cc.order`, a list of `{id}`; `cc.edited`) to `control-center.tiles` through the
//! published `settings.set` route. Closed, it holds nothing: the topic leases belong to the panel.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

/// Topics a tile source may read; the panel leases each for as long as it is open.
pub const TOPICS: [&str; 6] = [
    "audio",
    "network",
    "bluetooth",
    "power.profiles",
    "battery",
    "notifications",
];
/// The settings store and key behind `control-center.tiles`.
pub const STORE: &str = "control-center";
pub const STORE_TOPIC: &str = "settings.store.control-center";
const SET: &str = "settings.set";
const MAX_TILES: usize = 64;
/// `audio.adjust_volume` accepts at most this relative change per call.
const STEP: f64 = 0.2;

type Notify = Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>;

pub struct ControlCenter {
    client: Option<Client>,
    views: Vec<String>,
    topics: Vec<(&'static str, Arc<dyn Source>)>,
    store: Option<Arc<dyn Source>>,
    notify: RefCell<Option<Notify>>,
    st: RefCell<St>,
}
struct Call {
    source: String,
    ticket: CallTicket,
}
struct St {
    calls: Vec<Call>,
    store_call: Option<CallTicket>,
    /// Draft last handed to the daemon, the one waiting for the single in-flight slot, and
    /// the one the daemon refused (not retried until the draft changes).
    written: Option<Vec<String>>,
    want: Option<Vec<String>>,
    failed: Option<Vec<String>>,
    rev: Option<u64>,
    retried: bool,
    message: String,
    /// Latest slider target (percent) per source, applied once its previous call finished.
    targets: BTreeMap<String, f64>,
    max_height: f64,
}
impl Default for St {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            store_call: None,
            written: None,
            want: None,
            failed: None,
            rev: None,
            retried: false,
            message: String::new(),
            targets: BTreeMap::new(),
            max_height: 800.,
        }
    }
}

/// What a source shows. `node` and `adapter` carry the daemon's opaque ids for addressed calls.
#[derive(Default)]
struct Reading {
    on: bool,
    value: f64,
    status: String,
    icon: &'static str,
    node: Option<String>,
}

fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}
fn healthy(data: &Json) -> bool {
    data["health"]["available"] == true && data["health"]["stale"] != true
}
fn onoff(on: bool) -> String {
    tr(
        if on {
            "bead.common.on"
        } else {
            "bead.common.off"
        },
        &[],
    )
}
/// `(perceptual 0..1, muted)` of an audio node, honouring the optional `readback` members.
fn level(node: &Json) -> Option<(f64, bool)> {
    let ready = node["ready"] == true;
    let readback = node.get("readback");
    let volume = readback
        .and_then(|r| r["perceptual_volume"].as_f64())
        .or_else(|| {
            readback
                .and_then(|r| r["linear_volume"].as_f64())
                .map(f64::cbrt)
        })
        .or_else(|| {
            ready
                .then(|| node["volume"].as_f64())
                .flatten()
                .map(f64::cbrt)
        })?;
    let muted = readback
        .and_then(|r| r["muted"].as_bool())
        .or_else(|| ready.then(|| node["muted"].as_bool()).flatten());
    Some((volume, muted.unwrap_or(false)))
}
fn audio_reading(data: &Json, default: &str) -> Option<Reading> {
    if !healthy(data) {
        return None;
    }
    let id = data[default].as_str()?;
    let node = data["nodes"]["items"].get(id)?;
    let (volume, muted) = level(node)?;
    let percent = (volume * 100.).round().clamp(0., 100.);
    Some(Reading {
        on: muted,
        value: percent,
        status: if muted {
            tr("bead.panel.control-muted", &[])
        } else {
            format!("{percent}%")
        },
        icon: match (default, muted || percent == 0.) {
            ("default_input", true) => "mic_off",
            ("default_input", false) => "mic",
            (_, true) => "volume_off",
            _ if percent < 50. => "volume_down",
            _ => "volume_up",
        },
        node: Some(id.to_owned()),
    })
}

impl ControlCenter {
    pub fn new(
        client: Option<Client>,
        topics: Vec<(&'static str, Arc<dyn Source>)>,
        store: Option<Arc<dyn Source>>,
        views: Vec<String>,
    ) -> Self {
        Self {
            client,
            views,
            topics,
            store,
            notify: Default::default(),
            st: Default::default(),
        }
    }
    pub fn set_viewport(&self, _width: u32, height: u32) {
        self.st.borrow_mut().max_height = (f64::from(height) - 150.).max(240.);
    }
    pub fn subscribe(&self, notify: Notify) -> bead::core::source::Result<Vec<Subscription>> {
        *self.notify.borrow_mut() = Some(notify.clone());
        let callback: bead::core::source::Callback = {
            let notify = notify.clone();
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())))
        };
        let mut subscriptions = Vec::new();
        for (_, source) in &self.topics {
            subscriptions.push(source.subscribe(callback.clone())?);
        }
        if let Some(store) = &self.store {
            subscriptions.push(store.subscribe(callback.clone())?);
        }
        if let Some(client) = &self.client {
            subscriptions.push(client.subscribe(callback)?);
        }
        Ok(subscriptions)
    }
    fn waker(&self) -> CallWake {
        let notify = self.notify.borrow().clone();
        Arc::new(move || {
            if let Some(notify) = &notify {
                notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }
        })
    }
    fn data(&self, topic: &str) -> Option<Arc<Json>> {
        let source = &self.topics.iter().find(|(name, _)| *name == topic)?.1;
        let snapshot = source.snapshot();
        (snapshot.connected && snapshot.data.is_object()).then(|| snapshot.data.clone())
    }
    /// The daemon's advertised method at v1 on a live link, with the boot it was seen on.
    fn ready(&self, method: &str) -> Option<(String, Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 10_000)),
        ))
    }
    fn can(&self, method: &str) -> bool {
        self.ready(method).is_some()
    }

    // ---- sources: topic data -> reading, verbs -> published calls -------------------------

    fn read(&self, source: &str) -> Option<Reading> {
        match source {
            "audio.output" | "audio.input" => {
                let data = self.data("audio")?;
                let default = if source == "audio.output" {
                    "default_output"
                } else {
                    "default_input"
                };
                audio_reading(&data, default)
            }
            "network.wifi" => {
                let data = self.data("network")?;
                if !healthy(&data) {
                    return None;
                }
                let on = data["wireless_enabled"] == true;
                let active = data["access_points"]["items"]
                    .as_object()
                    .and_then(|aps| aps.values().find(|ap| ap["active"] == true))
                    .and_then(|ap| ap["name"].as_str().map(str::to_owned));
                Some(Reading {
                    on,
                    status: active.filter(|_| on).unwrap_or_else(|| onoff(on)),
                    icon: if on { "wifi" } else { "wifi_off" },
                    ..Default::default()
                })
            }
            "bluetooth.power" => {
                let data = self.data("bluetooth")?;
                let adapters = data["adapters"]["items"].as_object()?;
                if !healthy(&data) || adapters.is_empty() {
                    return None;
                }
                let sole = (adapters.len() == 1)
                    .then(|| adapters.iter().next())
                    .flatten();
                let on = adapters.values().any(|a| a["powered"] == true);
                let connected: Vec<&str> = data["devices"]["items"]
                    .as_object()
                    .map(|devices| {
                        devices
                            .values()
                            .filter(|d| d["connected"] == true)
                            .filter_map(|d| d["alias"].as_str().or_else(|| d["name"].as_str()))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Reading {
                    on,
                    status: if on && !connected.is_empty() {
                        connected.join(", ")
                    } else {
                        onoff(on)
                    },
                    icon: if on {
                        "bluetooth"
                    } else {
                        "bluetooth_disabled"
                    },
                    node: sole.map(|(id, _)| id.clone()),
                    ..Default::default()
                })
            }
            "power.profile" => {
                let data = self.data("power.profiles")?;
                let id = data["active"].as_str()?;
                if !healthy(&data) {
                    return None;
                }
                let name = data["profiles"]["items"].get(id)?["name"]
                    .as_str()?
                    .to_owned();
                let lower = name.to_lowercase();
                let mut shown = name.chars();
                Some(Reading {
                    status: shown
                        .next()
                        .map(|c| c.to_uppercase().chain(shown).collect())
                        .unwrap_or_default(),
                    icon: if lower.contains("saver") {
                        "eco"
                    } else if lower.contains("perf") {
                        "bolt"
                    } else {
                        "balance"
                    },
                    ..Default::default()
                })
            }
            "notifications.dnd" => {
                let data = self.data("notifications")?;
                if !healthy(&data) {
                    return None;
                }
                let on = data["do_not_disturb"] == true;
                Some(Reading {
                    on,
                    status: onoff(on),
                    icon: if on {
                        "do_not_disturb_on"
                    } else {
                        "notifications"
                    },
                    ..Default::default()
                })
            }
            _ => None,
        }
    }
    /// Which verbs the daemon offers for a source right now.
    fn verbs(&self, source: &str, reading: &Reading) -> (bool, bool) {
        match source {
            "audio.output" => (
                self.can("audio.toggle_mute"),
                self.can("audio.adjust_volume") || self.can("audio.set_volume"),
            ),
            "audio.input" => (self.can("audio.set_mute"), self.can("audio.set_volume")),
            "network.wifi" => (self.can("network.set_wifi"), false),
            "bluetooth.power" => (
                self.can("bluetooth.set_enabled")
                    || (reading.node.is_some() && self.can("bluetooth.set_power")),
                false,
            ),
            "notifications.dnd" => (self.can("notifications.set_dnd"), false),
            _ => (false, false),
        }
    }
    fn sources(&self) -> Vec<Json> {
        [
            "audio.output",
            "audio.input",
            "network.wifi",
            "bluetooth.power",
            "power.profile",
            "notifications.dnd",
        ]
        .iter()
        .filter_map(|source| {
            let reading = self.read(source)?;
            let (toggle, set) = self.verbs(source, &reading);
            Some(json!({
                "id": source, "available": true, "on": reading.on, "value": reading.value,
                "status": reading.status, "icon": reading.icon, "toggle": toggle, "set": set,
            }))
        })
        .collect()
    }
    fn begin(&self, source: &str, method: &str, params: Json) -> bool {
        let (Some(client), Some((boot, timeout))) = (&self.client, self.ready(method)) else {
            return false;
        };
        let mut st = self.st.borrow_mut();
        if st.calls.len() >= 8 || st.calls.iter().any(|c| c.source == source) {
            return false;
        }
        match client.begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(self.waker()),
        ) {
            Ok(ticket) => {
                st.calls.push(Call {
                    source: source.into(),
                    ticket,
                });
                st.message.clear();
                true
            }
            Err(_) => {
                st.message = tr("bead.common.request-failed", &[]);
                false
            }
        }
    }
    fn toggle(&self, source: &str) {
        let Some(reading) = self.read(source) else {
            return;
        };
        let call = match source {
            "audio.output" => Some(("audio.toggle_mute", json!({}))),
            "audio.input" => reading.node.map(|node| {
                (
                    "audio.set_mute",
                    json!({"node": node, "muted": !reading.on}),
                )
            }),
            "network.wifi" => Some(("network.set_wifi", json!({"enabled": !reading.on}))),
            "bluetooth.power" if self.can("bluetooth.set_enabled") => {
                Some(("bluetooth.set_enabled", json!({"enabled": !reading.on})))
            }
            "bluetooth.power" => reading.node.map(|adapter| {
                (
                    "bluetooth.set_power",
                    json!({"adapter": adapter, "powered": !reading.on}),
                )
            }),
            "notifications.dnd" => Some(("notifications.set_dnd", json!({"enabled": !reading.on}))),
            _ => None,
        };
        if let Some((method, params)) = call {
            self.begin(source, method, params);
        }
    }
    /// Move a level toward `percent`. The call is skipped while the previous one is in flight;
    /// the newest target waits in `targets` and is applied from the next projection.
    fn level(&self, source: &str, percent: f64) {
        let Some(reading) = self.read(source) else {
            return;
        };
        let target = (percent / 100.).clamp(0., 1.);
        let delta = target - reading.value / 100.;
        if delta.abs() < 0.005 {
            self.st.borrow_mut().targets.remove(source);
            return;
        }
        let sent = match (source, &reading.node) {
            ("audio.output", _) if self.can("audio.adjust_volume") => self.begin(
                source,
                "audio.adjust_volume",
                json!({"delta": delta.clamp(-STEP, STEP)}),
            ),
            (_, Some(node)) if self.can("audio.set_volume") => self.begin(
                source,
                "audio.set_volume",
                json!({"node": node, "volume": target}),
            ),
            _ => return,
        };
        let mut st = self.st.borrow_mut();
        if sent && delta.abs() <= STEP {
            st.targets.remove(source);
        } else {
            st.targets.insert(source.into(), percent);
        }
    }
    /// A tile asks: `toggle:<source>`, `press:<source>` or `set:<source>:<percent>`.
    pub fn act(&self, arg: &str) {
        let mut parts = arg.splitn(3, ':');
        let (verb, source, rest) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        match verb {
            "toggle" => self.toggle(source),
            "set" => {
                if let Ok(percent) = rest.parse::<f64>()
                    && percent.is_finite()
                {
                    self.level(source, percent.clamp(0., 100.));
                }
            }
            _ => {}
        }
    }

    // ---- the tile store ---------------------------------------------------------------------

    fn stored(&self) -> Option<(Arc<Json>, u64)> {
        let snapshot = self.store.as_ref()?.snapshot();
        if !snapshot.connected || !snapshot.data.is_object() {
            return None;
        }
        let rev = snapshot.data["rev"].as_u64()?;
        Some((snapshot.data.clone(), rev))
    }
    fn saved(&self) -> Option<Vec<String>> {
        let (data, _) = self.stored()?;
        let tiles = data["data"]["tiles"].as_array()?;
        Some(
            tiles
                .iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect(),
        )
    }
    fn writable(&self) -> bool {
        self.can(SET)
            && self
                .stored()
                .is_some_and(|(data, _)| data["error"].is_null())
    }
    /// The editor's draft becomes the stored order, one write at a time with the newest draft.
    fn persist(&self, stores: &Stores) {
        if stores.get("cc.edited") != Some(&Value::Bool(true)) {
            return;
        }
        let Some(Value::List(items)) = stores.get("cc.order") else {
            return;
        };
        let draft: Vec<String> = items
            .iter()
            .filter_map(|item| item["id"].as_str().map(str::to_owned))
            .collect();
        let valid = draft.len() <= MAX_TILES
            && draft.len() == items.len()
            && draft.iter().all(|id| {
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            });
        if !valid {
            return;
        }
        {
            let mut st = self.st.borrow_mut();
            if st.written.as_ref() != Some(&draft) && st.failed.as_ref() != Some(&draft) {
                st.want = Some(draft);
            }
        }
        self.flush();
    }
    fn flush(&self) {
        let Some(draft) = ({
            let st = self.st.borrow();
            st.store_call.is_none().then(|| st.want.clone()).flatten()
        }) else {
            return;
        };
        let (Some(client), Some((boot, timeout)), Some((_, topic_rev))) =
            (&self.client, self.ready(SET), self.stored())
        else {
            // Unpublished or not readable: the editor stays visible and non-persisting.
            return;
        };
        let mut st = self.st.borrow_mut();
        let rev = st.rev.map_or(topic_rev, |known| known.max(topic_rev));
        match client.begin_call(
            SET.into(),
            json!({"store": STORE, "key": ["tiles"], "value": draft, "rev": rev}),
            timeout,
            CallGuard { boot, version: 1 },
            Some(self.waker()),
        ) {
            Ok(ticket) => {
                st.store_call = Some(ticket);
                st.written = Some(draft);
                st.want = None;
            }
            Err(_) => {
                st.message = tr("bead.common.request-failed", &[]);
                st.failed = Some(draft);
                st.want = None;
            }
        }
    }
    fn poll(&self) {
        let mut st = self.st.borrow_mut();
        let mut index = 0;
        while index < st.calls.len() {
            match st.calls[index].ticket.try_take() {
                None => index += 1,
                Some(reply) => {
                    st.calls.remove(index);
                    if reply.is_err() {
                        st.message = tr("bead.common.request-failed", &[]);
                    }
                }
            }
        }
        let reply = st.store_call.as_mut().and_then(CallTicket::try_take);
        let Some(reply) = reply else {
            return;
        };
        st.store_call = None;
        match reply {
            Ok(value) => {
                st.rev = value["rev"].as_u64().or(st.rev);
                st.retried = false;
                st.message.clear();
            }
            // Re-read the revision from the topic and send the same draft once more.
            Err(e) if e.code == "stale_revision" && !st.retried => {
                st.retried = true;
                st.rev = None;
                st.want = st.written.take();
            }
            Err(e) => {
                st.message = if e.code == "stale_revision" {
                    tr("bead.common.changed-elsewhere", &[])
                } else {
                    tr("bead.common.request-failed", &[])
                };
                st.failed = st.written.clone();
                st.retried = false;
            }
        }
    }

    /// Project the open panel; a hidden panel projects nothing.
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        if stores.get("panel.open") == Some(&Value::Bool(false)) {
            return BTreeMap::new();
        }
        self.poll();
        self.persist(stores);
        let pending: Vec<(String, f64)> = {
            let st = self.st.borrow();
            st.targets
                .iter()
                .filter(|(source, _)| !st.calls.iter().any(|c| &c.source == *source))
                .map(|(source, percent)| (source.clone(), *percent))
                .collect()
        };
        for (source, percent) in pending {
            self.level(&source, percent);
        }
        let battery = self.data("battery").filter(|data| healthy(data));
        let percent = battery
            .as_ref()
            .and_then(|data| data["percentage"].as_f64())
            .map(|p| p.round().clamp(0., 100.));
        let charging = battery
            .as_ref()
            .is_some_and(|data| data["charging"] == true);
        let saved = self.saved();
        let st = self.st.borrow();
        BTreeMap::from([
            ("cc.src".into(), Value::list(self.sources())),
            (
                "cc.views".into(),
                Value::list(self.views.iter().map(|view| json!({"id": view})).collect()),
            ),
            (
                "cc.saved".into(),
                Value::list(
                    saved
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|id| json!({"id": id}))
                        .collect(),
                ),
            ),
            ("cc.saved_known".into(), Value::Bool(saved.is_some())),
            ("cc.writable".into(), Value::Bool(self.writable())),
            ("cc.message".into(), text(st.message.clone())),
            (
                "cc.battery".into(),
                text(
                    percent
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| tr("bead.panel.control-battery-unknown", &[])),
                ),
            ),
            (
                "cc.battery_icon".into(),
                text(if charging {
                    "battery_charging_full"
                } else {
                    "battery_full"
                }),
            ),
            ("cc.max_height".into(), Value::Number(st.max_height)),
        ])
    }
}

#[cfg(test)]
#[path = "control_center_tests.rs"]
mod tests;
