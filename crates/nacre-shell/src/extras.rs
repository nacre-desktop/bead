// SPDX-License-Identifier: GPL-3.0-only
//! B0077 edge adapter. Layout, filtering and navigation live in shell/extras.
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use nacre_protocol::menu::{MenuActivated, MenuQueryResult};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Overview,
    Palette,
    Keybindings,
    Wallpaper,
}
impl Kind {
    pub const ALL: [Self; 4] = [
        Self::Overview,
        Self::Palette,
        Self::Keybindings,
        Self::Wallpaper,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Palette => "palette",
            Self::Keybindings => "keybindings",
            Self::Wallpaper => "wallpaper",
        }
    }
    fn topic(self) -> Option<&'static str> {
        match self {
            Self::Overview => Some("compositor"),
            Self::Keybindings => Some("shortcuts"),
            Self::Palette => None,
            Self::Wallpaper => Some("wallpaper.active"),
        }
    }
}
/// N0058: the daemon owns palette content, order and matching; Bead only displays `order`.
const PALETTE_MODE: &str = "palette";
const PALETTE_ROUTE: &str = "palette";
struct Query {
    text: String,
    route: String,
    boot: String,
    ticket: CallTicket,
}
#[derive(Default)]
struct State {
    query: Option<Query>,
    wanted: Option<(String, String, String)>,
    result: Option<(String, MenuQueryResult)>,
    action: Option<CallTicket>,
    refreshed: Option<String>,
    message: String,
    close: bool,
    route: Option<String>,
    catalogue: Option<CallTicket>,
    catalogue_boot: Option<String>,
    catalogue_result: Vec<Json>,
    catalogue_request_ids: Vec<String>,
    catalogue_request_handles: Vec<String>,
    catalogue_pending_handles: std::collections::BTreeSet<String>,
    catalogue_visible_handles: std::collections::BTreeSet<String>,
    catalogue_settled_handles: std::collections::BTreeSet<String>,
    wallpaper_open: bool,
    wallpaper_activation: bool,
    wallpaper_pending: Option<String>,
    wallpaper_revision: Option<u64>,
    wallpaper_boot: Option<String>,
    images: BTreeMap<std::path::PathBuf, Arc<[u8]>>,
    image_bytes: usize,
    image_revision: u64,
}
pub struct Extras {
    pub kind: Kind,
    client: Client,
    topic: Option<Arc<dyn Source>>,
    state: RefCell<State>,
    glyphs: crate::workspace_rows::Rows,
    previews: Option<Previews>,
    wallpaper_settings: Option<crate::nacre_source::LeasedTopic>,
}
/// B0090 live previews: the leased capture exists only while the view is open and the
/// Nacre setting `livePreviews` is on; dropping `Extras` (or closing) releases it.
struct Previews {
    hub: Arc<bead::wayland::CaptureHub>,
    settings: Option<crate::nacre_source::Topic>,
    lease: RefCell<Option<bead::wayland::CaptureLease>>,
}
/// QML asks for a frame every 350 ms while the launcher is visible.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(350);
/// Largest thumbnail kept per window, device pixels (the Luau tile fits within its box).
const PREVIEW_BOUND: (u32, u32) = (480, 320);
impl Extras {
    pub fn new(kind: Kind, client: Client) -> bead::core::source::Result<Self> {
        let topic = kind
            .topic()
            .map(|name| match client.topic(name) {
                Ok(topic) => Ok(Arc::new(topic) as Arc<dyn Source>),
                Err(_) => client
                    .lease_topic(name)
                    .map(|topic| Arc::new(topic) as Arc<dyn Source>),
            })
            .transpose()?;
        let wallpaper_settings = if kind == Kind::Wallpaper {
            client.lease_topic(crate::prefs::WALLPAPER_PICKER).ok()
        } else {
            None
        };
        Ok(Self {
            kind,
            client,
            topic,
            state: Default::default(),
            glyphs: Default::default(),
            previews: None,
            wallpaper_settings,
        })
    }
    /// Offer live window previews through `hub` (used only when the compositor supports it).
    pub fn with_capture(mut self, hub: Arc<bead::wayland::CaptureHub>) -> Self {
        if self.kind == Kind::Overview {
            self.previews = Some(Previews {
                hub,
                settings: self.client.topic(crate::prefs::DESKTOP).ok(),
                lease: RefCell::new(None),
            });
        }
        self
    }
    /// Lease captures for the listed windows while open with `livePreviews` on; add each
    /// window's newest thumbnail (`preview`, `preview_width`, `preview_height`).
    fn attach_previews(&self, windows: &mut [Json], open: bool, wake: Option<CallWake>) {
        let Some(p) = &self.previews else { return };
        let on = p.settings.as_ref().is_none_or(|t| {
            let snapshot = t.snapshot();
            snapshot.data["data"]["livePreviews"] != false
        });
        let mut lease = p.lease.borrow_mut();
        if !(open && on && p.hub.supported()) {
            *lease = None;
            return;
        }
        let list: Vec<_> = windows
            .iter()
            .map(|w| bead::wayland::CaptureWindow {
                key: w["id"].as_str().unwrap_or_default().into(),
                app_id: w["app_id"].as_str().unwrap_or_default().into(),
                title: w["title"].as_str().unwrap_or_default().into(),
            })
            .collect();
        match &*lease {
            Some(l) => l.set_windows(list),
            None => {
                *lease = Some(p.hub.lease(
                    list,
                    PREVIEW_INTERVAL,
                    PREVIEW_BOUND,
                    wake.unwrap_or_else(|| Arc::new(|| {})),
                ))
            }
        }
        for w in windows {
            if let Some(preview) = w["id"].as_str().and_then(|id| p.hub.preview(id)) {
                w["preview"] = preview.path.to_string_lossy().into_owned().into();
                w["preview_width"] = preview.width.into();
                w["preview_height"] = preview.height.into();
            }
        }
    }
    pub fn subscribe(&self, wake: CallWake) -> bead::core::source::Result<Vec<Subscription>> {
        let w = wake.clone();
        let mut subscriptions = vec![self.client.subscribe(Arc::new(move |_| w()))?];
        if let Some(topic) = &self.topic {
            let topic_wake = wake.clone();
            subscriptions.push(topic.subscribe(Arc::new(move |_| topic_wake()))?);
        }
        if let Some(settings) = &self.wallpaper_settings {
            subscriptions.push(settings.subscribe(Arc::new(move |_| wake()))?);
        }
        Ok(subscriptions)
    }
    fn begin(&self, method: &str, params: Json, wake: Option<CallWake>) -> Option<CallTicket> {
        let status = self.client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !self.client.snapshot().connected {
            return None;
        }
        self.client
            .begin_call(
                method.into(),
                params,
                Duration::from_millis(u64::from(advertised.timeout).clamp(1, 30_000)),
                CallGuard {
                    boot: status.boot?,
                    version: 1,
                },
                wake,
            )
            .ok()
    }
    pub fn project(&self, stores: &Stores, wake: Option<CallWake>) -> BTreeMap<String, Value> {
        let mut state = self.state.borrow_mut();
        let boot = self
            .client
            .status()
            .boot
            .filter(|_| self.client.snapshot().connected);
        let mut rows = Vec::new();
        let mut windows = Vec::new();
        let mut workspaces = Vec::new();
        let mut available = false;
        let mut per_output = false;
        if let Some(topic) = &self.topic {
            let snapshot = topic.snapshot();
            let data = &snapshot.data;
            available = snapshot.connected
                && match self.kind {
                    Kind::Overview => data["available"] == true,
                    Kind::Wallpaper => data["available"] == true && data["stale"] != true,
                    _ => data["health"]["available"] == true && data["health"]["stale"] != true,
                };
            if available {
                if self.kind == Kind::Overview {
                    per_output = data["capabilities"]["perOutputWorkspaces"] == true;
                    (windows, workspaces) = compositor_rows(data);
                    for window in &mut windows {
                        window["icon"] = self
                            .glyphs
                            .app_glyph(window["app_id"].as_str().unwrap_or_default())
                            .into();
                    }
                    let open = matches!(stores.get("panel.open"), Some(Value::Bool(true)));
                    self.attach_previews(&mut windows, open, wake.clone());
                } else if self.kind == Kind::Keybindings {
                    rows = ordered(&data["items"]["items"], &data["items"]["order"])
                        .map(|(id, row)| json!({"id":id,"key":row["key"].as_str().unwrap_or_default(),"description":row["description"].as_str().unwrap_or_default()})).collect();
                }
            }
        }
        let open = matches!(stores.get("panel.open"), Some(Value::Bool(true)));
        let mut wallpaper_revision = 0;
        let mut wallpaper_kind = "static";
        let mut wallpaper_layout = "carousel";
        if self.kind == Kind::Wallpaper {
            state.wallpaper_open = open;
            if state.wallpaper_boot != boot {
                state.wallpaper_boot = boot.clone();
                state.wallpaper_revision = None;
                state.wallpaper_pending = None;
                state.catalogue = None;
                state.catalogue_boot = None;
                state.catalogue_result.clear();
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue_pending_handles.clear();
                state.catalogue_visible_handles.clear();
                state.catalogue_settled_handles.clear();
                state.images.clear();
                state.image_bytes = 0;
                state.image_revision = state.image_revision.wrapping_add(1);
            }
            if let Some(topic) = &self.topic {
                let active = topic.snapshot();
                wallpaper_revision = active.data["revision"]
                    .as_u64()
                    .unwrap_or_default()
                    .max(state.wallpaper_revision.unwrap_or_default());
            }
            if let Some(settings) = &self.wallpaper_settings {
                let snapshot = settings.snapshot();
                let data = &snapshot.data["data"];
                wallpaper_kind = match data["kind"].as_str() {
                    Some("dynamic") => "dynamic",
                    _ => "static",
                };
                wallpaper_layout = match data["layout"].as_str() {
                    Some("spotlight") => "spotlight",
                    Some("hexagons") => "hexagons",
                    _ => "carousel",
                };
            }
            if open && boot.is_some() && state.catalogue_boot != boot {
                state.catalogue_result.clear();
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue =
                    self.begin("wallpaper.catalogue", json!({"ids": []}), wake.clone());
                // A missing capability can be retried on the next registry wake. An admitted
                // request (including an error reply) is attempted once per opening/daemon boot.
                state.catalogue_boot = state.catalogue.as_ref().and(boot.clone());
                state.message = if state.catalogue.is_some() {
                    String::new()
                } else {
                    "Wallpaper catalogue unavailable".into()
                };
            }
            if !open {
                let had_resources = state.catalogue.is_some()
                    || !state.catalogue_result.is_empty()
                    || !state.catalogue_request_ids.is_empty()
                    || !state.catalogue_pending_handles.is_empty()
                    || !state.catalogue_visible_handles.is_empty()
                    || !state.images.is_empty()
                    || state.image_bytes != 0;
                state.catalogue = None;
                state.catalogue_result.clear();
                state.catalogue_boot = None;
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue_pending_handles.clear();
                state.catalogue_visible_handles.clear();
                state.catalogue_settled_handles.clear();
                state.message.clear();
                state.wallpaper_pending = None;
                state.images.clear();
                state.image_bytes = 0;
                if had_resources {
                    state.image_revision = state.image_revision.wrapping_add(1);
                }
            }
            if let Some(ticket) = &mut state.catalogue
                && let Some(reply) = ticket.try_take()
            {
                state.catalogue = None;
                let request_ids = std::mem::take(&mut state.catalogue_request_ids);
                let request_handles = std::mem::take(&mut state.catalogue_request_handles);
                let catalogue = reply
                    .ok()
                    .and_then(|reply| {
                        serde_json::from_value::<nacre_protocol::wallpaper::WallpaperCatalogue>(
                            reply,
                        )
                        .ok()
                    })
                    .filter(|catalogue| catalogue.validate().is_ok());
                if let Some(catalogue) = catalogue {
                    let mut seen = std::collections::BTreeSet::new();
                    let incoming: Vec<Json> = catalogue
                        .entries
                        .into_iter()
                        .filter(|entry| seen.insert(entry.id.clone()))
                        .map(|entry| {
                            serde_json::to_value(entry).expect("validated wallpaper entry")
                        })
                        .collect();
                    if request_ids.is_empty() {
                        state.catalogue_result = incoming;
                    } else {
                        for entry in incoming {
                            let Some(id) = entry["id"].as_str() else {
                                continue;
                            };
                            if let Some(existing) = state
                                .catalogue_result
                                .iter_mut()
                                .find(|row| row["id"].as_str() == Some(id))
                            {
                                for field in ["thumbnail", "preview"] {
                                    if entry[field].as_str().is_some_and(|path| !path.is_empty()) {
                                        existing[field] = entry[field].clone();
                                    }
                                }
                                existing["active"] = entry["active"].clone();
                            }
                        }
                        state.catalogue_settled_handles.extend(request_handles);
                    }
                    state.image_revision = state.image_revision.wrapping_add(1);
                    state.message.clear();
                } else {
                    for handle in request_handles {
                        state.catalogue_settled_handles.insert(handle);
                    }
                    if request_ids.is_empty() {
                        state.catalogue_result.clear();
                    }
                    state.message = "Wallpaper catalogue unavailable".into();
                }
            }
            if let Some(topic) = &self.topic {
                let snapshot = topic.snapshot();
                let current = &snapshot.data["current"];
                if let Some(id) = current["motion"]["path"]
                    .as_str()
                    .or_else(|| current["path"].as_str())
                {
                    for entry in &mut state.catalogue_result {
                        entry["active"] = (entry["id"].as_str() == Some(id)).into();
                    }
                }
            }
            if open {
                rows = state
                    .catalogue_result
                    .iter()
                    .cloned()
                    .map(|mut entry| {
                        if let Some(id) = entry["id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .map(str::to_owned)
                        {
                            for field in ["thumbnail", "preview"] {
                                entry[field] =
                                    format!("wallpaper:{field}:{}:{id}", state.image_revision)
                                        .into();
                            }
                        }
                        entry
                    })
                    .collect();
                available = boot.is_some() && state.catalogue_boot == boot;
            }
        }
        if self.kind == Kind::Keybindings && boot.is_some() && state.refreshed != boot {
            state.action = self.begin("shortcuts.refresh", json!({}), wake.clone());
            if state.action.is_some() {
                state.refreshed = boot.clone();
            }
        }
        let text = match stores.get("extras.query") {
            Some(Value::Text(s)) => s.as_str(),
            _ => "",
        };
        if self.kind == Kind::Palette {
            let route = state.route.clone().unwrap_or_else(|| PALETTE_ROUTE.into());
            let wanted = boot
                .as_ref()
                .map(|boot| (text.to_owned(), route.clone(), boot.clone()));
            if state.wanted != wanted
                || (state.query.is_none() && state.result.is_none() && state.message.is_empty())
            {
                state.query = None; // cancellation: no obsolete activation can survive a new query/boot
                state.result = None;
                state.wanted = wanted.clone();
                state.message.clear();
                if let Some((text, route, boot)) = wanted {
                    state.query = self
                        .begin(
                            "menu.query",
                            json!({"query":text,"mode":PALETTE_MODE,"route":route}),
                            wake.clone(),
                        )
                        .map(|ticket| Query {
                            text,
                            route,
                            boot,
                            ticket,
                        });
                }
            }
            if let Some(query) = &mut state.query
                && let Some(reply) = query.ticket.try_take()
            {
                let expected = (query.text.clone(), query.route.clone(), query.boot.clone());
                let result = reply
                    .ok()
                    .and_then(|reply| serde_json::from_value::<MenuQueryResult>(reply).ok())
                    .filter(|r| {
                        r.validate().is_ok()
                            && r.query == expected.0
                            && r.route == expected.1
                            && boot.as_ref() == Some(&expected.2)
                    });
                state.query = None;
                state.result = result.map(|r| (expected.2, r));
                if state.result.is_none() {
                    state.message = "Commands unavailable".into();
                }
            }
            if let Some((stamp, result)) = &state.result
                && Some(stamp) == boot.as_ref()
            {
                available = true;
                rows = result.order.iter().filter_map(|id| result.items.get(id).map(|row|
                    json!({"id":id,"name":row.name,"description":row.description.as_deref().unwrap_or_default(),"icon":row.icon.as_deref().unwrap_or("apps"),"kind":row.kind,"disabled":row.disabled}))).collect();
            }
        }
        let mut next_query = None;
        if let Some(ticket) = &mut state.action
            && let Some(reply) = ticket.try_take()
        {
            state.action = None;
            let wallpaper_activation = std::mem::take(&mut state.wallpaper_activation);
            match reply {
                Err(_) => state.message = "Action unavailable; refresh and try again".into(),
                Ok(reply) if self.kind == Kind::Palette => {
                    if let Ok(result) = serde_json::from_value::<MenuActivated>(reply)
                        && result.validate().is_ok()
                    {
                        state.close = !result.keep_open;
                        if let Some(view) = result.view {
                            if let Some(route) = view.route {
                                state.route = Some(route);
                            }
                            next_query = view.query;
                            state.wanted = None;
                            if let Some(wake) = &wake {
                                wake();
                            }
                        }
                    } else {
                        state.message = "Invalid command response".into();
                    }
                }
                Ok(reply) if self.kind == Kind::Wallpaper && wallpaper_activation => {
                    match serde_json::from_value::<nacre_protocol::wallpaper::ActiveWallpaper>(
                        reply,
                    ) {
                        Ok(active) if active.validate().is_ok() => {
                            state.wallpaper_revision = Some(active.revision);
                            state.message.clear();
                        }
                        _ => state.message = "Wallpaper selection response was invalid".into(),
                    }
                }
                Ok(_) => {}
            }
        }
        if self.kind == Kind::Wallpaper
            && state.action.is_none()
            && let Some(id) = state.wallpaper_pending.take()
            && state.catalogue_boot == boot
            && state
                .catalogue_result
                .iter()
                .any(|entry| entry["id"].as_str() == Some(id.as_str()))
        {
            let revision = self
                .topic
                .as_ref()
                .and_then(|topic| {
                    let snapshot = topic.snapshot();
                    snapshot
                        .connected
                        .then(|| snapshot.data["revision"].as_u64())
                        .flatten()
                })
                .into_iter()
                .chain(state.wallpaper_revision)
                .max();
            if let Some(revision) = revision {
                state.action = self.begin(
                    "wallpaper.activate",
                    json!({"revision":revision,"id":id}),
                    wake.clone(),
                );
                state.wallpaper_activation = state.action.is_some();
                if state.action.is_none() {
                    state.message = "Wallpaper action unavailable".into();
                }
            }
        }
        let mut values = BTreeMap::from([
            ("extras.kind".into(), Value::Text(self.kind.name().into())),
            ("extras.available".into(), Value::Bool(available)),
            ("extras.per_output".into(), Value::Bool(per_output)),
            ("extras.message".into(), Value::Text(state.message.clone())),
            ("extras.rows".into(), Value::list(rows)),
            ("extras.windows".into(), Value::list(windows)),
            ("extras.workspaces".into(), Value::list(workspaces)),
        ]);
        if self.kind == Kind::Wallpaper {
            let frame = crate::placement::Frame::current();
            let output_width = match stores.get("bead.viewport.width") {
                Some(Value::Number(w)) => *w as f32,
                _ => 1440.,
            };
            let output_height = match stores.get("bead.viewport.height") {
                Some(Value::Number(h)) => *h as f32,
                _ => 900.,
            };
            values.insert(
                "extras.viewport.width".into(),
                Value::Number((output_width - frame.left - frame.right).max(1.) as f64),
            );
            values.insert(
                "extras.viewport.height".into(),
                Value::Number((output_height - frame.top - frame.bottom).max(1.) as f64),
            );
        }
        if let Some(query) = next_query {
            values.insert("extras.query".into(), Value::Text(query));
        }
        if self.kind == Kind::Wallpaper {
            values.insert(
                "extras.wallpaper.kind".into(),
                Value::Text(wallpaper_kind.into()),
            );
            values.insert(
                "extras.wallpaper.layout".into(),
                Value::Text(wallpaper_layout.into()),
            );
            values.insert(
                "extras.wallpaper.revision".into(),
                Value::Number(wallpaper_revision as f64),
            );
            values.insert(
                "extras.wallpaper.loading".into(),
                Value::Bool(open && state.catalogue.is_some()),
            );
            values.insert(
                "extras.wallpaper.resource_revision".into(),
                Value::Number(state.image_revision as f64),
            );
        }
        // QML builds its search surface per opening, so a closed panel forgets the typed text.
        if matches!(stores.get("panel.open"), Some(Value::Bool(false))) && !text.is_empty() {
            values.insert("extras.query".into(), Value::Text(String::new()));
        }
        if !open && self.kind == Kind::Wallpaper {
            values.insert(
                "extras.wallpaper.selection_id".into(),
                Value::Text(String::new()),
            );
        }
        if std::mem::take(&mut state.close) {
            values.insert("panel.open".into(), Value::Bool(false));
        }
        values
    }
    pub fn act(&self, request: &str, stores: &Stores, wake: CallWake) {
        let mut state = self.state.borrow_mut();
        if self.kind == Kind::Wallpaper
            && let Some(id) = request.strip_prefix("wallpaper:activate:")
        {
            let boot = self.client.status().boot;
            if state.catalogue_boot != boot
                || !state
                    .catalogue_result
                    .iter()
                    .any(|entry| entry["id"].as_str() == Some(id))
            {
                return;
            }
            if state.action.is_some() {
                state.wallpaper_pending = Some(id.into());
                return;
            }
        }
        if state.action.is_some() {
            return;
        }
        if self.kind == Kind::Wallpaper {
            let (method, params) = if let Some(id) = request.strip_prefix("wallpaper:activate:") {
                let boot = self.client.status().boot;
                if state.catalogue_boot != boot
                    || !state
                        .catalogue_result
                        .iter()
                        .any(|entry| entry["id"].as_str() == Some(id))
                {
                    return;
                }
                let Some(topic) = &self.topic else { return };
                let active = topic.snapshot();
                let revision = active.data["revision"]
                    .as_u64()
                    .into_iter()
                    .chain(state.wallpaper_revision)
                    .max();
                let Some(revision) = revision.filter(|_| active.connected) else {
                    return;
                };
                ("wallpaper.activate", json!({"revision":revision,"id":id}))
            } else if let Some(value) = request.strip_prefix("wallpaper:kind:") {
                if !matches!(value, "static" | "dynamic") {
                    return;
                }
                let Some(rev) = self.wallpaper_settings_revision() else {
                    return;
                };
                (
                    "settings.set",
                    json!({"store":"wallpaper-picker","key":["kind"],"value":value,"rev":rev}),
                )
            } else if let Some(value) = request.strip_prefix("wallpaper:layout:") {
                if !matches!(value, "carousel" | "spotlight" | "hexagons") {
                    return;
                }
                let Some(rev) = self.wallpaper_settings_revision() else {
                    return;
                };
                (
                    "settings.set",
                    json!({"store":"wallpaper-picker","key":["layout"],"value":value,"rev":rev}),
                )
            } else {
                return;
            };
            state.wallpaper_activation = method == "wallpaper.activate";
            state.action = self.begin(method, params, Some(wake));
            if state.action.is_none() {
                state.message = "Wallpaper action unavailable".into();
            }
            return;
        }
        let (method, params) = if request == "refresh" {
            state.wanted = None;
            state.refreshed = None;
            return;
        } else if let Some(id) = request.strip_prefix("focus:") {
            let Some(topic) = &self.topic else {
                return;
            };
            let snapshot = topic.snapshot();
            if !snapshot.connected
                || snapshot.data["available"] != true
                || snapshot.data["windows"].get(id).is_none()
            {
                return;
            }
            ("window.focus", json!({"window":id}))
        } else if let Some(index) = request
            .strip_prefix("workspace:")
            .and_then(|s| s.parse::<u16>().ok())
        {
            let Some(topic) = &self.topic else {
                return;
            };
            let snapshot = topic.snapshot();
            if !snapshot.connected || snapshot.data["available"] != true || index == 0 {
                return;
            }
            if snapshot.data["capabilities"]["perOutputWorkspaces"] == true
                && !snapshot.data["workspaces"].as_object().is_some_and(|rows| {
                    rows.values().any(|row| {
                        row["index"].as_u64() == Some(u64::from(index))
                            && row["outputId"] == snapshot.data["focus"]["outputId"]
                    })
                })
            {
                return;
            }
            let Some(output) = snapshot.data["focus"]["outputId"]
                .as_str()
                .filter(|_| snapshot.connected)
            else {
                return;
            };
            (
                "workspace.activate",
                json!({"output":output,"workspace":index}),
            )
        } else if let Some(id) = request.strip_prefix("activate:") {
            let Some((boot, result)) = &state.result else {
                return;
            };
            let query = match stores.get("extras.query") {
                Some(Value::Text(s)) => s.as_str(),
                _ => "",
            };
            if self.client.status().boot.as_ref() != Some(boot)
                || result.query != query
                || !result.order.iter().any(|i| i == id)
            {
                return;
            }
            let Some(item) = result.items.get(id).filter(|i| !i.disabled) else {
                return;
            };
            let Some(token) = &item.activation else {
                return;
            };
            (
                "menu.activate",
                json!({"id":id,"activation":token,"catalog_rev":result.catalog_rev}),
            )
        } else {
            return;
        };
        state.action = self.begin(method, params, Some(wake));
        if state.action.is_none() {
            state.message = "Action unavailable".into();
        } else if method == "window.focus" || method == "workspace.activate" {
            state.close = true;
        }
    }
    /// Resolve only advertised cache paths; the byte-accounted cache dies with this open panel.
    pub fn attach_images(&self, scene: &mut bead::render::Scene, wake: Option<CallWake>) {
        const LIMIT: usize = 16 * bead::core::cache::MIB;
        let mut state = self.state.borrow_mut();
        let mut visible_handles = std::collections::BTreeSet::new();
        let mut attached_bytes = 0usize;
        let scene_handles: std::collections::BTreeSet<String> = scene
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                bead::render::Primitive::Image { path, .. } => path
                    .to_str()
                    .and_then(|name| name.strip_prefix("@icon:wallpaper:")),
                _ => None,
            })
            .filter_map(parse_wallpaper_handle)
            .map(|(id, field)| format!("{field}:{id}"))
            .collect();
        if scene_handles != state.catalogue_visible_handles {
            state.catalogue_settled_handles.clear();
            state.catalogue_visible_handles = scene_handles;
        }
        for primitive in &mut scene.primitives {
            let bead::render::Primitive::Image { path, .. } = primitive else {
                continue;
            };
            let Some(handle) = path
                .to_str()
                .and_then(|name| name.strip_prefix("@icon:wallpaper:"))
            else {
                continue;
            };
            let Some((id, field)) = parse_wallpaper_handle(handle) else {
                continue;
            };
            let Some(entry) = state
                .catalogue_result
                .iter()
                .find(|entry| entry["id"].as_str() == Some(id))
            else {
                continue;
            };
            let logical = format!("{field}:{id}");
            let primary_file = entry[field]
                .as_str()
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from);
            let fallback_file = (field == "preview")
                .then(|| {
                    entry["thumbnail"]
                        .as_str()
                        .filter(|path| !path.is_empty())
                        .map(std::path::PathBuf::from)
                })
                .flatten()
                .filter(|file| Some(file) != primary_file.as_ref());
            let primary_image = primary_file
                .as_ref()
                .and_then(|file| cached_wallpaper_image(&mut state, file, LIMIT));
            let fallback_image = fallback_file
                .as_ref()
                .and_then(|file| cached_wallpaper_image(&mut state, file, LIMIT));
            if primary_image.is_none() && !state.catalogue_settled_handles.contains(&logical) {
                visible_handles.insert(logical.clone());
            }
            let loaded = primary_image
                .zip(primary_file)
                .or_else(|| fallback_image.zip(fallback_file));
            let Some((bytes, file)) = loaded else {
                continue;
            };
            if !scene.asset_data.contains_key(&file) {
                let cost = bytes.len() + file.capacity() + 128;
                if attached_bytes + cost > LIMIT {
                    continue;
                }
                attached_bytes += cost;
                scene.asset_data.insert(file.clone(), bytes);
            }
            *path = file;
        }
        scene.primitives.retain(|primitive| {
            !matches!(primitive, bead::render::Primitive::Image { path, .. }
                if path.to_str().is_some_and(|name| name.starts_with("@icon:wallpaper:")))
        });
        state.catalogue_pending_handles = visible_handles;
        if self.kind == Kind::Wallpaper
            && state.wallpaper_open
            && state.catalogue.is_none()
            && let Some(wake) = wake
            && !state.catalogue_pending_handles.is_empty()
        {
            let mut ids = Vec::new();
            for handle in &state.catalogue_pending_handles {
                if let Some((id, _)) = parse_wallpaper_handle(handle)
                    && !ids.iter().any(|known: &String| known == id)
                {
                    ids.push(id.to_owned());
                    if ids.len() == 128 {
                        break;
                    }
                }
            }
            let requested_handles: Vec<String> = state
                .catalogue_pending_handles
                .iter()
                .filter(|handle| {
                    parse_wallpaper_handle(handle)
                        .is_some_and(|(id, _)| ids.iter().any(|known| known == id))
                })
                .cloned()
                .collect();
            if !ids.is_empty() {
                state.catalogue = self.begin("wallpaper.catalogue", json!({"ids":ids}), Some(wake));
                state.catalogue_request_ids = ids;
                state.catalogue_request_handles = requested_handles;
                if state.catalogue.is_none() {
                    for handle in std::mem::take(&mut state.catalogue_request_handles) {
                        state.catalogue_settled_handles.insert(handle);
                    }
                    state.catalogue_request_ids.clear();
                    state.message = "Wallpaper catalogue unavailable".into();
                }
            }
        }
    }
    pub fn image_bytes(&self) -> usize {
        self.state.borrow().image_bytes
    }

    fn wallpaper_settings_revision(&self) -> Option<u64> {
        let snapshot = self.wallpaper_settings.as_ref()?.snapshot();
        snapshot
            .connected
            .then(|| snapshot.data["rev"].as_u64())
            .flatten()
    }
}
fn ordered<'a>(items: &'a Json, order: &'a Json) -> impl Iterator<Item = (&'a str, &'a Json)> {
    order
        .as_array()
        .into_iter()
        .flatten()
        .take(2048)
        .filter_map(move |id| Some((id.as_str()?, items.get(id.as_str()?)?)))
}
fn parse_wallpaper_handle(handle: &str) -> Option<(&str, &str)> {
    let (field, rest) = handle.split_once(':')?;
    if !matches!(field, "thumbnail" | "preview") {
        return None;
    }
    let id = if let Some((generation, id)) = rest.split_once(':')
        && !generation.is_empty()
        && generation.bytes().all(|byte| byte.is_ascii_digit())
    {
        id
    } else {
        rest
    };
    (!id.is_empty()).then_some((id, field))
}
fn cached_wallpaper_image(
    state: &mut State,
    file: &std::path::PathBuf,
    limit: usize,
) -> Option<Arc<[u8]>> {
    if let Some(bytes) = state.images.get(file) {
        return Some(bytes.clone());
    }
    let bytes = crate::desktop::read_icon_file(file, 2 * bead::core::cache::MIB as u64).ok()?;
    let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 1600 || dimensions.1 > 1000 {
        return None;
    }
    let cost = bytes.len() + file.capacity() + 128;
    if state.image_bytes + cost > limit {
        state.images.clear();
        state.image_bytes = 0;
    }
    let bytes: Arc<[u8]> = Arc::from(bytes);
    state.image_bytes += cost;
    state.images.insert(file.clone(), bytes.clone());
    Some(bytes)
}
fn compositor_rows(data: &Json) -> (Vec<Json>, Vec<Json>) {
    let local = data["capabilities"]["perOutputWorkspaces"] == true;
    let output = &data["focus"]["outputId"];
    let workspaces = ordered(&data["workspaces"], &data["workspaceOrder"])
        .filter(|(_, row)| !local || &row["outputId"] == output)
        .map(|(id, row)| json!({"id":id,"index":row["index"].as_i64().or_else(|| row["id"].as_i64()).or_else(|| id.parse().ok()).unwrap_or(0),"name":row["name"].as_str().unwrap_or_default(),"output":row["outputId"]})).collect();
    let windows = ordered(&data["windows"], &data["windowOrder"])
        .filter(|(_, row)| row["hidden"] != true && (!local || &row["outputId"] == output))
        .map(|(id, row)| json!({"id":id,"title":row["title"].as_str().unwrap_or_default(),"app_id":row["wmClass"].as_str().or_else(|| row["appId"].as_str()).unwrap_or_default(),"workspace":row["workspaceId"],"icon":"apps"})).collect();
    (windows, workspaces)
}

/// The factory retains no view state or topic lease. Worker retirement drops
/// its runtime, subscriptions, pending calls, Luau VM and assets together.
pub fn panel(
    kind: Kind,
    client: Client,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    trace: bool,
    capture: Option<Arc<bead::wayland::CaptureHub>>,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    let mut cold = crate::sound::fallback(colors.get("surface")?);
    if let Some(motion) = &mut cold.motion {
        motion.edge = bead::core::Edge::Bottom;
    }
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::SceneEdge(bead::core::Edge::Bottom),
        )),
        engages_decorations: true,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback: cold.clone(),
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "extras/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(
                    crate::placement::Attachment::SceneEdge(bead::core::Edge::Bottom),
                    &colors,
                )
                .and_then(|runtime| {
                    let extras = Extras::new(kind, client.clone())?;
                    runtime.with_extras(match &capture {
                        Some(hub) => extras.with_capture(hub.clone()),
                        None => extras,
                    })
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|error| error.to_string())
        }),
    })
}

#[cfg(test)]
pub(crate) mod tests;
