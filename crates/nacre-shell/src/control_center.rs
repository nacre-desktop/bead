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
use nacre_protocol::{control_center::stream_id, core_services::BatteryState};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

/// Topics a tile source may read; the panel leases each for as long as it is open.
pub const TOPICS: [&str; 9] = [
    "audio",
    "controls",
    "network",
    "bluetooth",
    "vpn",
    "power.profiles",
    "battery",
    "notifications",
    "displays",
];
/// The VPN tile's source; its profile list and `switch` verb feed the dock page (N0062).
const VPN: &str = "vpn.state";
/// The settings store and key behind `control-center.tiles`.
pub const STORE: &str = "control-center";
pub const STORE_TOPIC: &str = "settings.store.control-center";
const SET: &str = "settings.set";
/// One-shot tiles: source id and the daemon method a press calls.
const ACTIONS: [(&str, &str); 3] = [
    ("screenshot.capture", "screenshot.capture"),
    ("color-picker.pick", "color_picker.pick"),
    ("wallpaper.next", "wallpaper.next"),
];
const MAX_TILES: usize = 64;
/// Tile sets the QML shell migrates to its current default while the stored defaults revision is below 2.
const PREVIOUS_DEFAULTS: [&[&str]; 2] = [
    &[
        "volume",
        "brightness",
        "network",
        "bluetooth",
        "battery",
        "night",
        "dnd",
        "awake",
        "appearance",
    ],
    &[
        "volume",
        "mic-volume",
        "brightness",
        "keyboard",
        "network",
        "bluetooth",
        "battery",
        "night",
        "dnd",
        "awake",
        "appearance",
    ],
];
/// Source ids of application streams start with this; the rest is the `audio` topic node id.
const STREAM: &str = "stream:";
/// The most application rows the sound views list.
const MAX_STREAMS: usize = 12;
/// The QML shell waited this long after closing its panel before an interactive tool started.
const CLOSE_DELAY: Duration = Duration::from_millis(320);

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
    /// An interactive tool was started: the host closes the panel (N0046: "close your panels first").
    close: bool,
    /// The last VPN change was refused: the page says so until the next attempt (QML `NacreVpn.error`).
    vpn_failed: bool,
    /// The viewport height; `cc.max_height` is what remains inside the live frame.
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
            close: false,
            vpn_failed: false,
            max_height: 800.,
        }
    }
}

/// What a source shows. `node` carries the daemon's opaque id (adapter, output, mode) for addressed calls.
#[derive(Default)]
struct Reading {
    on: bool,
    /// Another controller owns the state: shown, not switchable.
    locked: bool,
    value: f64,
    status: String,
    icon: &'static str,
    node: Option<String>,
    /// What a level control names under its icon (QML: the device description); empty uses the tile's own label.
    label: String,
    /// The profiles a picker offers: `(id, shown name, active)`.
    choices: Vec<(String, String, bool)>,
    /// A note the backend attaches to the state (the profile daemon's degradation reason).
    note: String,
}
/// What the daemon lets a tile do right now.
#[derive(Default)]
struct Verbs {
    toggle: bool,
    set: bool,
    press: bool,
}
const PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];
const MODES: [&str; 3] = ["light", "dark", "auto"];

fn capital(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
/// Slider percent (perceptual) to the daemon's linear level: the slider is cube-rooted.
fn linear(percent: f64) -> f64 {
    (percent / 100.).clamp(0., 1.).powi(3)
}

/// The shown name of a known power profile id.
fn profile_name(id: &str) -> String {
    tr(
        match id {
            "power-saver" => "bead.osd.power-saver",
            "performance" => "bead.osd.performance",
            _ => "bead.osd.balanced",
        },
        &[],
    )
}

/// N0062 `vpn` profiles in the daemon's order: `(id, name, state, active)`.
fn vpn_profiles(data: &Json) -> Vec<(String, String, String, bool)> {
    let items = &data["connections"]["items"];
    data["connections"]["order"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| {
            let id = id.as_str()?;
            let profile = &items[id];
            Some((
                id.to_owned(),
                profile["name"].as_str()?.to_owned(),
                profile["state"].as_str()?.to_owned(),
                profile["active"] == true,
            ))
        })
        .collect()
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
/// `(linear 0..1, muted)` of an application stream: QML wrote the stream slider's linear value.
fn stream_level(node: &Json) -> Option<(f64, bool)> {
    let ready = node["ready"] == true;
    let readback = node.get("readback");
    let volume = readback
        .and_then(|r| r["linear_volume"].as_f64())
        .or_else(|| ready.then(|| node["volume"].as_f64()).flatten())?;
    let muted = readback
        .and_then(|r| r["muted"].as_bool())
        .or_else(|| ready.then(|| node["muted"].as_bool()).flatten());
    Some((volume, muted.unwrap_or(false)))
}
/// Whether the icon theme has this application icon (QML shows its `apps` glyph otherwise).
fn themed_icon(name: &str) -> bool {
    thread_local! {
        static KNOWN: RefCell<BTreeMap<String, bool>> = const { RefCell::new(BTreeMap::new()) };
    }
    KNOWN.with(|known| {
        let mut known = known.borrow_mut();
        if known.len() > 256 {
            known.clear();
        }
        *known.entry(name.to_owned()).or_insert_with(|| {
            !name.contains('/')
                && freedesktop_icons::lookup(name)
                    .with_size(32)
                    .find()
                    .is_some()
        })
    })
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
        icon: match (default, muted) {
            ("default_input", true) => "mic_off",
            ("default_input", false) => "mic",
            (_, true) => "volume_off",
            _ if percent < 35. => "volume_down",
            _ => "volume_up",
        },
        node: Some(id.to_owned()),
        label: node["name"].as_str().unwrap_or_default().to_owned(),
        ..Default::default()
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
        self.st.borrow_mut().max_height = f64::from(height);
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

    /// Launcher Quick reads only the three control-centre-backed toggles.
    pub(crate) fn quick_state(
        &self,
        source: &str,
    ) -> Option<(bool, bool, bool, String, &'static str)> {
        let reading = self.read(source)?;
        let enabled = self.verbs(source, &reading).toggle && !reading.locked;
        Some((
            reading.on,
            enabled,
            reading.locked,
            reading.status,
            reading.icon,
        ))
    }
    pub(crate) fn quick_pending(&self, source: &str) -> bool {
        self.st
            .borrow()
            .calls
            .iter()
            .any(|call| call.source == source)
    }
    pub(crate) fn poll_quick(&self) {
        self.poll();
    }
    pub(crate) fn quick_message(&self) -> String {
        self.st.borrow().message.clone()
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
                // A live link whose backend is absent (no power-profiles-daemon): the QML tile stays and says so.
                if data["health"]["available"] == false && data["health"]["stale"] != true {
                    return Some(Reading {
                        status: tr("bead.panel.battery-profiles-unavailable", &[]),
                        icon: "balance",
                        ..Default::default()
                    });
                }
                let id = data["active"].as_str()?;
                if !healthy(&data) {
                    return None;
                }
                // N0059: entity ids are the profile daemon's own (`power-saver`, `balanced`, `performance`)
                // and what `power.set_profile` takes; the entry's name may be a display name or the id.
                let items = &data["profiles"]["items"];
                let name = items.get(id)?["name"].as_str()?;
                let choices = data["profiles"]["order"]
                    .as_array()
                    .map(|order| {
                        order
                            .iter()
                            .filter_map(|entry| entry.as_str())
                            .filter(|entry| PROFILES.contains(entry))
                            .filter(|entry| items[*entry]["available"] == true)
                            .map(|entry| (entry.to_owned(), profile_name(entry), entry == id))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Reading {
                    status: if PROFILES.contains(&id) {
                        profile_name(id)
                    } else {
                        capital(name)
                    },
                    note: data["degraded"]
                        .as_str()
                        .map(|reason| reason.chars().take(80).collect())
                        .unwrap_or_default(),
                    choices,
                    icon: match id {
                        "power-saver" => "eco",
                        "performance" => "bolt",
                        _ => "balance",
                    },
                    ..Default::default()
                })
            }
            // N0062: as QML, the tile is on while any profile is active and names the first one.
            VPN => {
                let data = self.data("vpn")?;
                if !healthy(&data) {
                    return None;
                }
                let profiles = vpn_profiles(&data);
                let first = profiles.iter().find(|profile| profile.3);
                Some(Reading {
                    on: first.is_some(),
                    status: first.map_or_else(|| onoff(false), |profile| profile.1.clone()),
                    icon: "vpn_key",
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
            // N0046 `controls`: a member is present only while its backend is.
            "brightness.level" => {
                let data = self.controls()?;
                let outputs = data["brightness"].as_object()?;
                let (name, percent) = outputs
                    .iter()
                    .find(|(name, _)| {
                        let name = name.to_ascii_lowercase();
                        name.starts_with("edp") || name.starts_with("lvds")
                    })
                    .or_else(|| outputs.iter().next())?;
                let value = percent.as_f64()?.clamp(0., 100.).round();
                Some(Reading {
                    value,
                    status: format!("{value}%"),
                    icon: "brightness_6",
                    label: self.output_description(name),
                    node: Some(name.clone()),
                    ..Default::default()
                })
            }
            "keyboard.backlight" => {
                let data = self.controls()?;
                let (level, max) = (
                    data["keyboard"]["level"].as_f64()?,
                    data["keyboard"]["max"].as_f64()?,
                );
                let value = (level / max.max(1.) * 100.).clamp(0., 100.).round();
                Some(Reading {
                    on: level > 0.,
                    value,
                    status: format!("{value}%"),
                    icon: "keyboard",
                    ..Default::default()
                })
            }
            "airplane.mode" | "night-light.state" | "stay-awake.state" => {
                let (member, icon) = match source {
                    "airplane.mode" => ("airplane", "flight"),
                    "night-light.state" => ("night_light", "nightlight"),
                    _ => ("stay_awake", "coffee"),
                };
                let data = self.controls()?;
                let on = data[member]["enabled"].as_bool()?;
                Some(Reading {
                    on,
                    locked: data[member]["external"] == true,
                    status: onoff(on),
                    icon,
                    ..Default::default()
                })
            }
            "appearance.mode" => {
                let data = self.controls()?;
                let mode = data["appearance"]["mode"].as_str()?;
                MODES.contains(&mode).then(|| Reading {
                    on: mode == "dark",
                    status: tr(
                        match mode {
                            "light" => "bead.panel.control-mode-light",
                            "dark" => "bead.panel.control-mode-dark",
                            _ => "bead.panel.control-mode-auto",
                        },
                        &[],
                    ),
                    icon: "palette",
                    node: Some(mode.to_owned()),
                    ..Default::default()
                })
            }
            "recorder.state" => {
                let recording = self.controls()?["recorder"]["recording"].as_bool()?;
                Some(Reading {
                    on: recording,
                    status: if recording {
                        tr("bead.panel.control-recording", &[])
                    } else {
                        onoff(false)
                    },
                    icon: "videocam",
                    ..Default::default()
                })
            }
            // One-shot actions have no state: they exist while the daemon advertises them.
            "screenshot.capture" | "color-picker.pick" | "wallpaper.next" => self
                .can(ACTIONS.iter().find(|(id, _)| *id == source)?.1)
                .then(Reading::default),
            // B0086: an application stream, `stream:<audio topic id>`; its level is linear.
            _ => {
                let id = source.strip_prefix(STREAM)?;
                let data = self.data("audio").filter(|data| healthy(data))?;
                let (linear, muted) = stream_level(data["nodes"]["items"].get(id)?)?;
                let percent = (linear * 100.).round().clamp(0., 100.);
                Some(Reading {
                    on: muted,
                    value: percent,
                    status: format!("{percent}%"),
                    node: Some(id.to_owned()),
                    ..Default::default()
                })
            }
        }
    }
    /// The output's description as the QML level tile shows it ("Make Model"), else its name.
    fn output_description(&self, name: &str) -> String {
        self.data("displays")
            .and_then(|data| {
                let output = data["outputs"]
                    .as_array()?
                    .iter()
                    .find(|o| o["name"] == name)?;
                let words: Vec<&str> = ["make", "model"]
                    .iter()
                    .filter_map(|key| output[*key].as_str())
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .collect();
                (!words.is_empty()).then(|| words.join(" "))
            })
            .unwrap_or_else(|| name.to_owned())
    }
    fn controls(&self) -> Option<Arc<Json>> {
        self.data("controls").filter(|data| healthy(data))
    }
    /// Which verbs the daemon offers for a source right now.
    fn verbs(&self, source: &str, reading: &Reading) -> Verbs {
        let (toggle, set, press) = match source {
            "audio.output" | "audio.input" => (
                self.can("audio.set_mute"),
                self.can("audio.set_volume"),
                false,
            ),
            "network.wifi" => (self.can("network.set_wifi"), false, false),
            "bluetooth.power" => (
                reading.node.is_some() && self.can("bluetooth.set_power"),
                false,
                false,
            ),
            "notifications.dnd" => (self.can("notifications.set_dnd"), false, false),
            "brightness.level" => (false, self.can("brightness.set"), false),
            "keyboard.backlight" => (false, self.can("keyboard_light.set"), false),
            source if source.starts_with(STREAM) => (
                self.can("audio.set_mute"),
                self.can("audio.set_volume"),
                false,
            ),
            "airplane.mode" => (self.can("airplane.set"), false, false),
            "night-light.state" => (self.can("night_light.set") && !reading.locked, false, false),
            "stay-awake.state" => (self.can("stay_awake.set"), false, false),
            "appearance.mode" => (self.can("appearance.set_mode"), false, false),
            "recorder.state" => (
                self.can(if reading.on {
                    "recorder.stop"
                } else {
                    "recorder.start"
                }),
                false,
                false,
            ),
            "power.profile" => (
                false,
                self.can("power.set_profile") && !reading.choices.is_empty(),
                false,
            ),
            _ => (false, false, ACTIONS.iter().any(|(id, _)| *id == source)),
        };
        Verbs { toggle, set, press }
    }
    /// B0086: the application streams of the sound popout and the Audio page, in daemon order.
    pub(crate) fn audio_streams(&self) -> Vec<Json> {
        let Some(data) = self.data("audio").filter(|data| healthy(data)) else {
            return Vec::new();
        };
        let order = data["nodes"]["order"].as_array();
        order
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let id = id.as_str()?;
                let node = &data["nodes"]["items"][id];
                if !matches!(node["kind"].as_str(), Some("playback" | "capture")) {
                    return None;
                }
                let (linear, muted) = stream_level(node)?;
                let percent = (linear * 100.).round().clamp(0., 100.);
                let name = node["name"].as_str().filter(|name| !name.is_empty());
                let icon = node["icon"].as_str().filter(|icon| themed_icon(icon));
                Some(json!({
                    "id": id,
                    "label": name.map_or_else(|| tr("bead.panel.audio-application", &[]), |name| crate::status_views::line(name, 40)),
                    "icon": icon.unwrap_or(""),
                    "value": percent, "status": format!("{percent}%"), "muted": muted,
                    "enabled": node["ready"] == true && self.can("audio.set_volume"),
                }))
            })
            .take(MAX_STREAMS)
            .collect()
    }
    pub(crate) fn audio_devices(&self) -> Vec<Json> {
        let Some(data) = self.data("audio").filter(|data| healthy(data)) else {
            return Vec::new();
        };
        let Some(order) = data["nodes"]["order"].as_array() else {
            return Vec::new();
        };
        order.iter().filter_map(|id| {
            let id = id.as_str()?;
            let node = &data["nodes"]["items"][id];
            let kind = node["kind"].as_str()?;
            if !matches!(kind, "output" | "input") { return None; }
            Some(json!({"id": id, "kind": kind, "label": node["name"].as_str().unwrap_or(id),
                "selected": data[if kind == "output" { "default_output" } else { "default_input" }] == id,
                "enabled": node["ready"] == true && self.can("audio.set_default")}))
        }).collect()
    }
    fn sources(&self) -> Vec<Json> {
        [
            "audio.output",
            "audio.input",
            "network.wifi",
            "bluetooth.power",
            VPN,
            "power.profile",
            "notifications.dnd",
            "brightness.level",
            "keyboard.backlight",
            "airplane.mode",
            "night-light.state",
            "stay-awake.state",
            "appearance.mode",
            "recorder.state",
            "screenshot.capture",
            "color-picker.pick",
            "wallpaper.next",
        ]
        .iter()
        .filter_map(|source| {
            let reading = self.read(source)?;
            let verbs = self.verbs(source, &reading);
            // The profile picker is offered only while the daemon can switch profiles.
            let choices: Vec<Json> = reading
                .choices
                .iter()
                .filter(|_| verbs.set)
                .map(|(id, name, active)| json!({"id": id, "name": name, "active": active}))
                .collect();
            Some(json!({
                "id": source, "available": true, "on": reading.on, "value": reading.value,
                "status": reading.status, "label": reading.label, "icon": reading.icon, "toggle": verbs.toggle,
                "set": verbs.set && choices.is_empty(), "press": verbs.press, "choices": choices,
                "note": reading.note,
                "mode": reading.node.as_deref().filter(|_| *source == "appearance.mode").unwrap_or(""),
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
                if source == VPN {
                    st.vpn_failed = false;
                }
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
        let on = reading.on;
        let call = match source {
            "audio.output" => Some(("audio.set_mute", json!({"target": "sink", "muted": !on}))),
            "audio.input" => Some(("audio.set_mute", json!({"target": "source", "muted": !on}))),
            "network.wifi" => Some(("network.set_wifi", json!({"enabled": !on}))),
            "bluetooth.power" => reading.node.map(|adapter| {
                (
                    "bluetooth.set_power",
                    json!({"adapter": adapter, "powered": !on}),
                )
            }),
            "notifications.dnd" => Some(("notifications.set_dnd", json!({"enabled": !on}))),
            "airplane.mode" => Some(("airplane.set", json!({"enabled": !on}))),
            "night-light.state" if !reading.locked => {
                Some(("night_light.set", json!({"enabled": !on})))
            }
            "stay-awake.state" => Some(("stay_awake.set", json!({"enabled": !on}))),
            "appearance.mode" => reading.node.and_then(|mode| {
                let next = MODES[(MODES.iter().position(|m| *m == mode)? + 1) % MODES.len()];
                Some(("appearance.set_mode", json!({"mode": next})))
            }),
            source if source.starts_with(STREAM) => reading.node.map(|node| {
                (
                    "audio.set_mute",
                    json!({"target": "stream", "node": node, "muted": !on}),
                )
            }),
            "recorder.state" if on => Some(("recorder.stop", json!({}))),
            "recorder.state" => {
                self.after_close("recorder.start", json!({"mode": "region"}));
                None
            }
            _ => None,
        };
        if let Some((method, params)) = call {
            self.begin(source, method, params);
        }
    }
    /// A one-shot tile: interactive tools close the panel first, the rest call at once.
    fn press(&self, source: &str) {
        let Some((_, method)) = ACTIONS.iter().find(|(id, _)| *id == source) else {
            return;
        };
        match *method {
            "screenshot.capture" => self.after_close(method, json!({"mode": "region"})),
            "color_picker.pick" => self.after_close(method, json!({})),
            _ if self.read(source).is_some() => {
                self.begin(source, method, json!({}));
            }
            _ => {}
        }
    }
    /// N0046: interactive tools (recorder, screenshot, picker) reply as soon as they start and the caller
    /// closes its own panels first. The panel is asked to close now; the call goes out after the QML delay
    /// from a short-lived thread, because a closed panel keeps nothing resident to send it.
    fn after_close(&self, method: &'static str, params: Json) {
        let (Some(client), Some((boot, timeout))) = (self.client.clone(), self.ready(method))
        else {
            return;
        };
        self.st.borrow_mut().close = true;
        std::thread::spawn(move || {
            std::thread::sleep(CLOSE_DELAY);
            if let Ok(_ticket) = client.begin_call(
                method.into(),
                params,
                timeout,
                CallGuard { boot, version: 1 },
                None,
            ) {
                // A queued call is cancelled when its ticket drops: hold it until it is on the wire.
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }
    /// True once after a tile started an interactive tool: the host closes the panel.
    pub fn take_close(&self) -> bool {
        std::mem::take(&mut self.st.borrow_mut().close)
    }
    /// B0067: the dock's gear. The window route can wait on its socket, so it runs off the UI thread.
    fn open_settings(&self, page: &str) {
        self.st.borrow_mut().close = true;
        let args: Vec<Json> = (!page.is_empty())
            .then(|| Json::String(page.to_owned()))
            .into_iter()
            .collect();
        std::thread::spawn(move || {
            let _ = crate::app::route(0, "settings", "open", &args);
        });
    }
    /// The QML clipboard tile: the launcher in its clipboard mode, with the dock closed as `openMode` does.
    fn open_clipboard(&self) {
        self.st.borrow_mut().close = true;
        std::thread::spawn(|| {
            crate::app::command("clipboard");
        });
    }
    /// B0067: the notification summary the dock shows under the tiles: the newest two history entries,
    /// the stored count and whether the history has loaded (the QML `NacreControlNotices`).
    fn notices(&self) -> Vec<(String, Value)> {
        let data = self.data("notifications").filter(|d| healthy(d));
        let history = data.as_ref().map(|d| &d["history"]);
        let rows: Vec<Json> = history
            .and_then(|h| h["order"].as_array())
            .into_iter()
            .flatten()
            .rev()
            .filter_map(|id| {
                let item = &history?["items"][id.as_str()?];
                Some(json!({
                    "id": id.as_str()?, "app": item["app_name"].as_str().unwrap_or(""),
                    "summary": item["summary"].as_str().unwrap_or(""), "body": item["body"].as_str().unwrap_or(""),
                }))
            })
            .take(2)
            .collect();
        let count = data
            .as_ref()
            .and_then(|d| d["history_count"].as_u64())
            .unwrap_or(rows.len() as u64);
        let ready = data
            .as_ref()
            .is_some_and(|d| d["history_storage"]["ready"] == true);
        vec![
            ("cc.notices".into(), Value::list(rows)),
            ("cc.notice_count".into(), Value::Number(count as f64)),
            ("cc.notice_ready".into(), Value::Bool(ready)),
            (
                "cc.notice_clear".into(),
                Value::Bool(count > 0 && self.can("notifications.clear")),
            ),
        ]
    }
    fn choose(&self, source: &str, choice: &str) {
        if matches!(source, "audio.output" | "audio.input") {
            let kind = if source == "audio.output" {
                "output"
            } else {
                "input"
            };
            if let Some(data) = self.data("audio").filter(|data| healthy(data))
                && let Some(node) = data["nodes"]["items"].get(choice)
                && node["ready"] == true
                && node["kind"] == kind
            {
                self.begin(source, "audio.set_default", json!({"node": choice}));
            }
            return;
        }

        if source == "power.profile"
            && self
                .read(source)
                .is_some_and(|reading| reading.choices.iter().any(|(id, _, _)| id == choice))
        {
            self.begin(source, "power.set_profile", json!({"profile": choice}));
        } else if source == "appearance.mode" && MODES.contains(&choice) {
            // B0067: the segmented Light | Dark | Auto control names its mode; tapping the current one is a no-op.
            if self.read(source).and_then(|r| r.node).as_deref() != Some(choice) {
                self.begin(source, "appearance.set_mode", json!({"mode": choice}));
            }
        }
    }
    /// The call a slider target means: `(method, params)`, or `None` when it already holds.
    fn set_call(
        &self,
        source: &str,
        reading: &Reading,
        percent: f64,
    ) -> Option<(&'static str, Json)> {
        match source {
            "audio.output" | "audio.input" => {
                if (percent - reading.value).abs() < 0.5 {
                    return None;
                }
                let target = if source == "audio.output" {
                    "sink"
                } else {
                    "source"
                };
                Some((
                    "audio.set_volume",
                    json!({"target": target, "value": linear(percent)}),
                ))
            }
            source if source.starts_with(STREAM) => {
                if (percent - reading.value).abs() < 0.5 {
                    return None;
                }
                Some((
                    "audio.set_volume",
                    json!({"target": "stream", "node": reading.node, "value": (percent / 100.).clamp(0., 1.)}),
                ))
            }
            "brightness.level" => {
                let percent = percent.round();
                (percent != reading.value).then(|| {
                    (
                        "brightness.set",
                        json!({"output": reading.node, "percent": percent as u8}),
                    )
                })
            }
            "keyboard.backlight" => {
                let max = self.controls()?["keyboard"]["max"].as_f64()?.max(1.);
                let level = (percent / 100. * max).round();
                (level != (reading.value / 100. * max).round())
                    .then(|| ("keyboard_light.set", json!({"level": level as u8})))
            }
            _ => None,
        }
    }
    /// Move a level to `percent`. The call is skipped while the previous one is in flight;
    /// the newest target waits in `targets` and is applied from the next projection.
    fn level(&self, source: &str, percent: f64) {
        let Some(reading) = self.read(source) else {
            return;
        };
        let Some((method, params)) = self.set_call(source, &reading, percent) else {
            self.st.borrow_mut().targets.remove(source);
            return;
        };
        let sent = self.begin(source, method, params);
        let mut st = self.st.borrow_mut();
        if sent {
            st.targets.remove(source);
        } else {
            st.targets.insert(source.into(), percent);
        }
    }
    /// N0062: the page row for a profile connects it, or disconnects it when it is active (QML
    /// `activate(uuid, !active)`). Only IDs the daemon published are sent back.
    fn vpn_switch(&self, id: &str) {
        let Some(data) = self.data("vpn").filter(|data| healthy(data)) else {
            return;
        };
        if let Some((_, _, _, active)) = vpn_profiles(&data).iter().find(|p| p.0 == id) {
            let method = if *active {
                "vpn.disconnect"
            } else {
                "vpn.connect"
            };
            self.begin(VPN, method, json!({"connection": id}));
        }
    }
    /// N0062: the VPN page's rows and its one status line (QML `NacreControlVpn`): the failure text
    /// until the next attempt, else the busy text, else "none saved"; unavailable when the topic is.
    fn vpn_page(&self) -> Vec<(String, Value)> {
        let data = self.data("vpn").filter(|data| healthy(data));
        let busy = self.quick_pending(VPN);
        let profiles = data.as_deref().map(vpn_profiles).unwrap_or_default();
        let rows: Vec<Json> = profiles
            .iter()
            .map(|(id, name, state, active)| {
                let detail = match state.as_str() {
                    "connected" => tr("bead.common.connected", &[]),
                    "connecting" => tr("bead.panel.control-vpn-connecting", &[]),
                    "disconnecting" => tr("bead.panel.control-vpn-disconnecting", &[]),
                    _ => onoff(false),
                };
                let method = if *active {
                    "vpn.disconnect"
                } else {
                    "vpn.connect"
                };
                json!({"id": id, "label": name, "detail": detail, "on": active,
                    "enabled": !busy && self.can(method)})
            })
            .collect();
        let failed = self.st.borrow().vpn_failed || profiles.iter().any(|p| p.2 == "failed");
        let key = if data.is_none() {
            "bead.panel.control-vpn-unavailable"
        } else if failed {
            "bead.panel.control-vpn-failed"
        } else if busy {
            "bead.panel.control-vpn-working"
        } else if profiles.is_empty() {
            "bead.panel.control-vpn-none"
        } else {
            ""
        };
        vec![
            ("cc.vpn".into(), Value::list(rows)),
            (
                "cc.vpn_message".into(),
                text(if key.is_empty() {
                    String::new()
                } else {
                    tr(key, &[])
                }),
            ),
        ]
    }
    /// A tile asks: `toggle:<source>`, `press:<source>`, `choose:<source>:<id>`, `set:<source>:<percent>`
    /// or `switch:vpn.state:<id>`.
    pub fn act(&self, arg: &str) {
        let mut parts = arg.splitn(3, ':');
        let (verb, source, rest) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        // Stream ids hold a colon: `toggle:stream:<id>` and `set:stream:<id>:<percent>`.
        if source == "stream" {
            let key = format!("{STREAM}{rest}");
            match verb {
                "toggle" => self.toggle(&key),
                "set" => {
                    if let Some((id, percent)) = rest.rsplit_once(':')
                        && stream_id(id).is_some()
                        && let Ok(percent) = percent.parse::<f64>()
                        && percent.is_finite()
                    {
                        self.level(&format!("{STREAM}{id}"), percent.clamp(0., 100.));
                    }
                }
                _ => {}
            }
            return;
        }
        match verb {
            "toggle" => self.toggle(source),
            "press" => self.press(source),
            "choose" => self.choose(source, rest),
            "switch" if source == VPN => self.vpn_switch(rest),
            "clear" if source == "notifications" => {
                self.begin(source, "notifications.clear", json!({"scope": "history"}));
            }
            // The QML gear: open the Settings window, closing the dock as `openSettings` does.
            // `open:settings[:page]` (the gear, Wallpaper) and `open:clipboard`.
            "open" if source == "settings" => self.open_settings(rest),
            "open" if source == "clipboard" => self.open_clipboard(),
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
        let saved: Vec<String> = tiles
            .iter()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect();
        // QML `NacreControlPreferences`: before defaults revision 2 a stored copy of an earlier default set (the
        // registry's nine tiles, or those plus the microphone and keyboard sliders) is the unedited default, so the
        // shown set is the current default (B0091).
        let revision = data["data"]["defaultsRevision"].as_u64().unwrap_or(0);
        let plain = |ids: &[&str]| saved.iter().map(String::as_str).eq(ids.iter().copied());
        if revision < 2 && (PREVIOUS_DEFAULTS.iter().any(|ids| plain(ids))) {
            return None;
        }
        Some(saved)
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
                    let call = st.calls.remove(index);
                    if reply.is_err() && call.source == VPN {
                        st.vpn_failed = true;
                    } else if reply.is_err() {
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
        let battery = self
            .data("battery")
            .filter(|data| healthy(data))
            .and_then(|data| serde_json::from_value::<BatteryState>((*data).clone()).ok());
        let percent = battery
            .as_ref()
            .and_then(|state| state.percentage)
            .filter(|p| p.is_finite())
            .map(|p| p.round().clamp(0., 100.));
        // QML: the glyph shows mains power unless the system reports battery power.
        let on_battery = battery
            .as_ref()
            .is_some_and(|state| state.on_battery == Some(true));
        let estimate = battery
            .as_ref()
            .filter(|_| percent.is_some())
            .map(crate::status_views::battery_estimate)
            .unwrap_or_default();
        let saved = self.saved();
        let frame = crate::placement::Frame::current();
        let frame_height = f64::from(frame.top) + f64::from(frame.bottom);
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
                text(if on_battery {
                    "battery_full"
                } else {
                    "battery_charging_full"
                }),
            ),
            ("cc.battery_estimate".into(), text(estimate)),
            // B0067: the dock is as tall as the framed viewport (QML: the sidebar keeps the full frame height).
            (
                "cc.max_height".into(),
                Value::Number((st.max_height - frame_height).max(240.)),
            ),
        ])
        .into_iter()
        .chain(self.notices())
        .chain(self.vpn_page())
        .collect()
    }
}

#[cfg(test)]
#[path = "control_center_tests.rs"]
mod tests;
