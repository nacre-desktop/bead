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
const CATALOGUE_PALETTE_CACHE_LIMIT: usize = 64;
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
    catalogue_request_boot: Option<String>,
    catalogue_result: Vec<Json>,
    catalogue_preloaded: bool,
    catalogue_request_ids: Vec<String>,
    catalogue_request_handles: Vec<String>,
    catalogue_pending_handles: std::collections::BTreeSet<String>,
    catalogue_visible_handles: std::collections::BTreeSet<String>,
    catalogue_settled_handles: std::collections::BTreeSet<String>,
    wallpaper_open: bool,
    wallpaper_prefetched: bool,
    wallpaper_boot: Option<String>,
    images: BTreeMap<std::path::PathBuf, Arc<[u8]>>,
    image_bytes: usize,
    image_revision: u64,
    viewport: (u32, u32),
}
#[derive(Clone)]
struct WallpaperSelection {
    id: String,
    boot: String,
    revision: u64,
    selected_at: std::time::Instant,
    file_bytes: u64,
    source_width: u32,
    source_height: u32,
}
struct WallpaperActivationInFlight {
    ticket: CallTicket,
    boot: String,
    preview_generation: Option<u64>,
    selection_generation: u64,
    selection: WallpaperSelection,
}
/// Panel-factory-owned activation admission survives lazy view retirement without retaining its UI.
struct WallpaperActivationQueue {
    client: Client,
    preview: Option<Arc<crate::wallpaper::Preview>>,
    inflight: Option<WallpaperActivationInFlight>,
    selection_generation: u64,
    confirmed: Option<(String, u64)>,
    error: Option<String>,
    wake: Option<std::sync::Weak<dyn Fn() + Send + Sync>>,
}
impl WallpaperActivationQueue {
    fn new(client: Client) -> Self {
        Self {
            client,
            preview: None,
            inflight: None,
            selection_generation: 0,
            confirmed: None,
            error: None,
            wake: None,
        }
    }
    fn confirmed_revision(&self, boot: &str) -> Option<u64> {
        self.confirmed
            .as_ref()
            .filter(|(current, _)| current == boot)
            .map(|(_, revision)| *revision)
    }
    fn error(&self) -> Option<String> {
        self.error.clone()
    }
    fn invalidate_boot(&mut self, boot: Option<&str>) {
        if self
            .confirmed
            .as_ref()
            .is_some_and(|(current, _)| Some(current.as_str()) != boot)
        {
            self.confirmed = None;
        }
        self.error = None;
    }
    fn submit(
        queue: &Arc<std::sync::Mutex<Self>>,
        selection: WallpaperSelection,
        wake: &CallWake,
    ) -> bool {
        let mut state = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let status = state.client.status();
        if !state.client.snapshot().connected
            || status.boot.as_deref() != Some(selection.boot.as_str())
        {
            state.error = Some("Wallpaper action unavailable".into());
            return false;
        }
        state.wake = Some(Arc::downgrade(wake));
        state.error = None;
        if state
            .confirmed
            .as_ref()
            .is_some_and(|(boot, _)| boot != &selection.boot)
        {
            state.confirmed = None;
        }
        // Each click starts immediately. Dropping the previous ticket abandons its
        // reply locally; it must never serialize a newer selection behind it.
        state.inflight = None;
        state.selection_generation = state.selection_generation.wrapping_add(1).max(1);
        let generation = state.selection_generation;
        let started = Self::start(queue, &mut state, selection, generation);
        if !started {
            state.error = Some("Wallpaper action unavailable".into());
        }
        started
    }
    fn start(
        queue: &Arc<std::sync::Mutex<Self>>,
        state: &mut Self,
        selection: WallpaperSelection,
        selection_generation: u64,
    ) -> bool {
        let status = state.client.status();
        let Some(method) = status
            .methods
            .get("wallpaper.activate")
            .filter(|method| method.v == 1)
        else {
            return false;
        };
        if !state.client.snapshot().connected
            || status.boot.as_deref() != Some(selection.boot.as_str())
        {
            return false;
        }
        let revision = state
            .confirmed_revision(&selection.boot)
            .unwrap_or(selection.revision)
            .max(selection.revision);
        let weak_queue = Arc::downgrade(queue);
        let wake: CallWake =
            Arc::new(move || Self::completed(weak_queue.clone(), selection_generation));
        bead::core::profile::mark("wallpaper_selection_admitted");
        match state.client.begin_call(
            "wallpaper.activate".into(),
            json!({"revision":revision,"id":selection.id}),
            Duration::from_millis(u64::from(method.timeout).clamp(1, 30_000)),
            CallGuard {
                boot: selection.boot.clone(),
                version: 1,
            },
            Some(wake),
        ) {
            Ok(ticket) => {
                wallpaper_switch_phase(&selection, "request_sent");
                state.inflight = Some(WallpaperActivationInFlight {
                    ticket,
                    preview_generation: state
                        .preview
                        .as_ref()
                        .and_then(|preview| preview.generation_for(&selection.boot, &selection.id)),
                    selection_generation,
                    boot: selection.boot.clone(),
                    selection,
                });
                true
            }
            Err(_) => false,
        }
    }
    fn completed(weak_queue: std::sync::Weak<std::sync::Mutex<Self>>, selection_generation: u64) {
        let Some(queue) = weak_queue.upgrade() else {
            return;
        };
        let mut state = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(inflight) = state.inflight.as_mut() else {
            return;
        };
        if inflight.selection_generation != selection_generation {
            return;
        }
        let Some(reply) = inflight.ticket.try_take() else {
            return;
        };
        let boot = inflight.boot.clone();
        let selection = inflight.selection.clone();
        let selection_generation = inflight.selection_generation;
        let preview_generation = inflight.preview_generation;
        let preview = state.preview.clone();
        let mut cancel_preview = Vec::new();
        let mut confirmed = false;
        state.inflight = None;
        if selection_generation == state.selection_generation
            && state.client.status().boot.as_deref() == Some(boot.as_str())
        {
            match reply {
                Ok(reply) => match serde_json::from_value::<
                    nacre_protocol::wallpaper::ActiveWallpaper,
                >(reply)
                {
                    Ok(active) if active.validate().is_ok() => {
                        bead::core::profile::mark("wallpaper_selection_confirmed");
                        wallpaper_switch_phase(&selection, "daemon_ack");
                        let revision = state
                            .confirmed_revision(&boot)
                            .unwrap_or_default()
                            .max(active.revision);
                        state.confirmed = Some((boot, revision));
                        state.error = None;
                        confirmed = true;
                    }
                    _ => state.error = Some("Wallpaper selection response was invalid".into()),
                },
                Err(error) => {
                    println!(
                        "WALLPAPER_ACTIVATE_ERROR revision={} code={}",
                        selection.revision, error.code
                    );
                    state.error = Some("Wallpaper selection failed".into());
                }
            }
        }
        if !confirmed && let Some(generation) = preview_generation {
            cancel_preview.push(generation);
        }
        let wake = state.wake.as_ref().and_then(std::sync::Weak::upgrade);
        drop(state);
        if let Some(preview) = preview {
            for generation in cancel_preview {
                preview.cancel(generation);
            }
        }
        if let Some(wake) = wake {
            wake();
        }
    }
}
pub struct Extras {
    pub kind: Kind,
    client: Client,
    topic: Option<Arc<dyn Source>>,
    state: RefCell<State>,
    glyphs: crate::workspace_rows::Rows,
    previews: Option<Previews>,
    wallpaper_settings: Option<Arc<dyn Source>>,
    wallpaper_actions: Option<Arc<std::sync::Mutex<WallpaperActivationQueue>>>,
}
fn preload_wallpaper_catalogue(
    state: &mut State,
    boot: &str,
    active_path: &str,
    kind: &str,
    layout: &str,
) -> bool {
    if state.catalogue_preloaded || !state.catalogue_result.is_empty() {
        return false;
    }
    let Some(prepared) = crate::wallpaper_warm::prepared_catalogue() else {
        return false;
    };
    if prepared.boot != boot
        || prepared.active_path != active_path
        || prepared.kind != kind
        || prepared.layout != layout
    {
        return false;
    }
    let entry_count = prepared.entries.len();
    let thumbnail_count = prepared
        .entries
        .iter()
        .filter(|entry| {
            entry["thumbnail"]
                .as_str()
                .is_some_and(|path| !path.is_empty())
        })
        .count();
    state.catalogue_result = prepared.entries.to_vec();
    state.catalogue_boot = Some(boot.to_owned());
    state.catalogue_preloaded = true;
    state.image_revision = state.image_revision.wrapping_add(1);
    bead::core::vprintln!(
        "WALLPAPER_PRELOAD entries={} thumbnails={}",
        entry_count,
        thumbnail_count
    );
    true
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
            client
                .topic(crate::prefs::WALLPAPER_PICKER)
                .map(|topic| Arc::new(topic) as Arc<dyn Source>)
                .or_else(|_| {
                    client
                        .lease_topic(crate::prefs::WALLPAPER_PICKER)
                        .map(|topic| Arc::new(topic) as Arc<dyn Source>)
                })
                .ok()
        } else {
            None
        };
        let mut state = State::default();
        if kind == Kind::Wallpaper
            && let Some(boot) = client.status().boot.filter(|_| client.snapshot().connected)
            && let Some(entries) = crate::wallpaper_catalogue_cache::load()
        {
            state.catalogue_result = entries;
            state.catalogue_boot = Some(boot.clone());
            state.wallpaper_boot = Some(boot);
        }
        if kind == Kind::Wallpaper
            && let (Some(topic), Some(settings), Some(boot)) = (
                topic.as_ref(),
                wallpaper_settings.as_ref(),
                client.status().boot.filter(|_| client.snapshot().connected),
            )
        {
            let active = topic.snapshot();
            let active_path = active.data["current"]["motion"]["path"]
                .as_str()
                .or_else(|| active.data["current"]["path"].as_str())
                .unwrap_or_default();
            let preferences = settings.snapshot();
            let data = &preferences.data["data"];
            let wallpaper_kind = if data["kind"] == "dynamic" {
                "dynamic"
            } else {
                "static"
            };
            let wallpaper_layout = match data["layout"].as_str() {
                Some("spotlight") => "spotlight",
                Some("hexagons") => "hexagons",
                _ => "carousel",
            };
            if preload_wallpaper_catalogue(
                &mut state,
                &boot,
                active_path,
                wallpaper_kind,
                wallpaper_layout,
            ) {
                state.wallpaper_boot = Some(boot);
            }
        }
        let wallpaper_actions = (kind == Kind::Wallpaper).then(|| {
            Arc::new(std::sync::Mutex::new(WallpaperActivationQueue::new(
                client.clone(),
            )))
        });
        Ok(Self {
            kind,
            client,
            topic,
            state: RefCell::new(state),
            glyphs: Default::default(),
            previews: None,
            wallpaper_settings,
            wallpaper_actions,
        })
    }
    fn wallpaper_preview(&self) -> Option<Arc<crate::wallpaper::Preview>> {
        self.wallpaper_actions
            .as_ref()?
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .preview
            .clone()
    }
    fn with_wallpaper_actions(
        mut self,
        actions: Arc<std::sync::Mutex<WallpaperActivationQueue>>,
    ) -> Self {
        self.wallpaper_actions = Some(actions);
        self
    }
    pub fn set_viewport(&self, width: u32, height: u32) {
        self.state.borrow_mut().viewport = (width.max(1), height.max(1));
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
        let mut wallpaper_active_path = String::new();
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
                } else if self.kind == Kind::Wallpaper {
                    wallpaper_active_path = data["current"]["motion"]["path"]
                        .as_str()
                        .or_else(|| data["current"]["path"].as_str())
                        .unwrap_or_default()
                        .to_owned();
                }
            }
        }
        let open = matches!(stores.get("panel.open"), Some(Value::Bool(true)));
        if self.kind == Kind::Wallpaper {
            crate::wallpaper_motion::set_picker_open(open);
        }
        let mut wallpaper_revision = 0;
        let mut wallpaper_kind = "static";
        let mut wallpaper_layout = "carousel";
        let mut wallpaper_opened = false;
        if self.kind == Kind::Wallpaper {
            wallpaper_opened = open && !state.wallpaper_open;
            state.wallpaper_open = open;
            if state.wallpaper_boot != boot {
                state.wallpaper_boot = boot.clone();
                if let Some(actions) = &self.wallpaper_actions {
                    actions
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .invalidate_boot(boot.as_deref());
                }
                state.catalogue = None;
                state.catalogue_boot = None;
                state.catalogue_request_boot = None;
                state.catalogue_result.clear();
                state.catalogue_preloaded = false;
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue_pending_handles.clear();
                state.catalogue_visible_handles.clear();
                state.catalogue_settled_handles.clear();
                state.images.clear();
                state.image_bytes = 0;
                state.image_revision = state.image_revision.wrapping_add(1);
                if let Some(boot) = boot.as_ref()
                    && let Some(entries) = crate::wallpaper_catalogue_cache::load()
                {
                    state.catalogue_result = entries;
                    state.catalogue_boot = Some(boot.clone());
                }
            }
            if let Some(topic) = &self.topic {
                let active = topic.snapshot();
                wallpaper_revision = active.data["revision"].as_u64().unwrap_or_default();
            }
            if let (Some(actions), Some(boot)) = (&self.wallpaper_actions, boot.as_deref())
                && let Some(revision) = actions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .confirmed_revision(boot)
            {
                wallpaper_revision = wallpaper_revision.max(revision);
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
            if open && let Some(boot) = boot.as_ref() {
                preload_wallpaper_catalogue(
                    &mut state,
                    boot,
                    &wallpaper_active_path,
                    wallpaper_kind,
                    wallpaper_layout,
                );
            }
            if open
                && boot.is_some()
                && state.catalogue.is_none()
                && !state.catalogue_preloaded
                && state.catalogue_result.is_empty()
                && state.catalogue_boot != boot
                && (wallpaper_opened || state.catalogue_request_boot != boot)
            {
                if state.catalogue_boot != boot {
                    state.catalogue_result.clear();
                }
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue =
                    self.begin("wallpaper.catalogue", json!({"ids": []}), wake.clone());
                // Keep the last valid snapshot across closes. Only an empty/missing catalogue
                // needs a full refresh; otherwise every reopen would respawn the ID scan.
                if state.catalogue.is_some() {
                    state.catalogue_request_boot = boot.clone();
                }
                state.message = if state.catalogue.is_some() {
                    String::new()
                } else {
                    "Wallpaper catalogue unavailable".into()
                };
            }
            if !open {
                // The panel is sliding out. Cancel service work, but keep the last
                // projected content and encoded image leases until the worker retires.
                state.catalogue = None;
                state.catalogue_request_boot = None;
                state.catalogue_request_ids.clear();
                state.catalogue_request_handles.clear();
                state.catalogue_pending_handles.clear();
                state.catalogue_visible_handles.clear();
                state.catalogue_settled_handles.clear();
                state.catalogue_preloaded = false;
            }
            if let Some(ticket) = &mut state.catalogue
                && let Some(reply) = ticket.try_take()
            {
                state.catalogue = None;
                let request_ids = std::mem::take(&mut state.catalogue_request_ids);
                let request_handles = std::mem::take(&mut state.catalogue_request_handles);
                let raw_catalogue = reply.ok();
                let catalogue = raw_catalogue
                    .as_ref()
                    .and_then(|reply| {
                        let mut typed_reply = reply.clone();
                        if let Some(entries) = typed_reply["entries"].as_array_mut() {
                            for entry in entries {
                                if let Some(object) = entry.as_object_mut() {
                                    object.remove("palette");
                                }
                            }
                        }
                        serde_json::from_value::<nacre_protocol::wallpaper::WallpaperCatalogue>(
                            typed_reply,
                        )
                        .ok()
                    })
                    .filter(|catalogue| catalogue.validate().is_ok());
                if let Some(catalogue) = catalogue {
                    let mut resources_changed =
                        request_ids.is_empty() || !state.catalogue_pending_handles.is_empty();
                    let mut seen = std::collections::BTreeSet::new();
                    let mut incoming: Vec<Json> = catalogue
                        .entries
                        .into_iter()
                        .filter(|entry| seen.insert(entry.id.clone()))
                        .map(|entry| {
                            let id = entry.id.clone();
                            let mut row =
                                serde_json::to_value(entry).expect("validated wallpaper entry");
                            if let Some(palette) = raw_catalogue
                                .as_ref()
                                .and_then(|raw| raw_catalogue_palette(raw, &id))
                            {
                                row["palette"] = palette.clone();
                            }
                            row
                        })
                        .collect();
                    if request_ids.is_empty() {
                        for entry in &mut incoming {
                            let Some(id) = entry["id"].as_str() else {
                                continue;
                            };
                            if let Some(previous) = state
                                .catalogue_result
                                .iter()
                                .find(|row| row["id"].as_str() == Some(id))
                            {
                                for field in ["thumbnail", "preview", "palette"] {
                                    if (entry[field].is_null() || entry[field].as_str() == Some(""))
                                        && !previous[field].is_null()
                                        && previous[field].as_str() != Some("")
                                    {
                                        entry[field] = previous[field].clone();
                                    }
                                }
                            }
                        }
                        state.catalogue_result = incoming;
                        state.catalogue_boot = boot.clone();
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
                                        resources_changed |= existing[field] != entry[field];
                                        existing[field] = entry[field].clone();
                                    }
                                }
                                existing["active"] = entry["active"].clone();
                                if request_ids.iter().any(|requested| requested.as_str() == id) {
                                    existing["video_predecode"] = entry["video_predecode"].clone();
                                    if let Some(palette) = entry
                                        .get("palette")
                                        .filter(|palette| valid_catalogue_palette(palette))
                                    {
                                        existing["palette"] = palette.clone();
                                    } else if let Some(object) = existing.as_object_mut() {
                                        object.remove("palette");
                                    }
                                }
                            }
                        }
                        state.catalogue_settled_handles.extend(request_handles);
                    }
                    trim_catalogue_palette_cache(&mut state.catalogue_result, &request_ids);
                    crate::wallpaper_catalogue_cache::store(&state.catalogue_result);
                    if resources_changed {
                        state.image_revision = state.image_revision.wrapping_add(1);
                    }
                    state.message.clear();
                } else {
                    for handle in request_handles {
                        state.catalogue_settled_handles.insert(handle);
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
            // B0171: display-size rasters for every still, so any first selection is a cache hit.
            if !open {
                if std::mem::take(&mut state.wallpaper_prefetched)
                    && let Some(preview) = self.wallpaper_preview()
                {
                    preview.cancel_prefetch();
                }
            } else if !state.wallpaper_prefetched
                && !state.catalogue_result.is_empty()
                && let Some(preview) = self.wallpaper_preview()
            {
                let images: Vec<&Json> = state
                    .catalogue_result
                    .iter()
                    .filter(|entry| entry["kind"] == "image")
                    .collect();
                let ids: Vec<String> = images
                    .iter()
                    .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
                    .collect();
                let active = images.iter().position(|entry| entry["active"] == true);
                state.wallpaper_prefetched = preview.prefetch(&ids, active);
            }
            if boot.is_some() && state.catalogue_boot == boot {
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
                available = true;
            }
            // A cache hit or in-flight refresh is usable picker content even if the
            // active-wallpaper topic has not finished reporting its health yet.
            if boot.is_some() && (state.catalogue_boot == boot || state.catalogue.is_some()) {
                available = true;
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
                Ok(_) => {}
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
            let (width, height) = state.viewport;
            let output_width = if width == 0 { 1440. } else { width as f32 };
            let output_height = if height == 0 { 900. } else { height as f32 };
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
                Value::Bool(state.catalogue.is_some()),
            );
            values.insert(
                "extras.wallpaper.resource_revision".into(),
                Value::Number(state.image_revision as f64),
            );
            if let Some(error) = self.wallpaper_actions.as_ref().and_then(|actions| {
                actions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .error()
            }) {
                values.insert("extras.message".into(), Value::Text(error));
            }
        }
        // QML builds its search surface per opening, so a closed panel forgets the typed text.
        if matches!(stores.get("panel.open"), Some(Value::Bool(false)))
            && self.kind != Kind::Wallpaper
            && !text.is_empty()
        {
            values.insert("extras.query".into(), Value::Text(String::new()));
        }
        if wallpaper_opened && !text.is_empty() {
            values.insert("extras.query".into(), Value::Text(String::new()));
        }
        if wallpaper_opened && self.kind == Kind::Wallpaper {
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
        if self.kind == Kind::Wallpaper
            && let Some(id) = request.strip_prefix("wallpaper:activate:")
        {
            let boot = self.client.status().boot;
            let state = self.state.borrow();
            if state.catalogue_boot != boot {
                return;
            }
            let Some(entry) = state
                .catalogue_result
                .iter()
                .find(|entry| entry["id"].as_str() == Some(id))
                .cloned()
            else {
                return;
            };
            drop(state);
            let Some(boot) = boot else { return };
            let Some(topic) = &self.topic else { return };
            let active = topic.snapshot();
            if !active.connected {
                return;
            }
            let active_state =
                serde_json::from_value::<nacre_protocol::wallpaper::WallpaperActive>(
                    (*active.data).clone(),
                )
                .ok()
                .filter(|state| state.validate().is_ok());
            let palette_template = active_state
                .as_ref()
                .and_then(|state| state.outputs.values().next());
            let revision = active.data["revision"]
                .as_u64()
                .or_else(|| {
                    self.wallpaper_actions.as_ref().and_then(|queue| {
                        queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .confirmed_revision(&boot)
                    })
                })
                .unwrap_or_default();
            let Some(queue) = &self.wallpaper_actions else {
                return;
            };
            let selected_at = std::time::Instant::now();
            let (file_bytes, source_width, source_height) = wallpaper_source_metadata(id);
            let selection = WallpaperSelection {
                id: id.into(),
                boot: boot.clone(),
                revision,
                selected_at,
                file_bytes,
                source_width,
                source_height,
            };
            wallpaper_switch_phase(&selection, "selection");
            let preview = queue
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .preview
                .clone();
            let generation = preview.as_ref().map(|preview| {
                preview.select_at_with_palette(crate::wallpaper::SelectionRequest {
                    boot: boot.clone(),
                    id: id.into(),
                    kind: entry["kind"].as_str().unwrap_or("image").into(),
                    video_predecode: serde_json::from_value(entry["video_predecode"].clone()).ok(),
                    palette_template,
                    palette: entry.get("palette"),
                    requested_at: selected_at,
                    metadata: (file_bytes, source_width, source_height),
                })
            });
            let admitted = WallpaperActivationQueue::submit(queue, selection, &wake);
            if !admitted && let (Some(preview), Some(generation)) = (preview, generation) {
                preview.cancel(generation);
            }
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.action.is_some() {
            return;
        }
        if self.kind == Kind::Wallpaper {
            let (key, value) = if let Some(value) = request.strip_prefix("wallpaper:kind:") {
                if !matches!(value, "static" | "dynamic") {
                    return;
                }
                ("kind", value)
            } else if let Some(value) = request.strip_prefix("wallpaper:layout:") {
                if !matches!(value, "carousel" | "spotlight" | "hexagons") {
                    return;
                }
                ("layout", value)
            } else {
                return;
            };
            let Some(params) = self.wallpaper_preference(key, value) else {
                return;
            };
            // Picker-only fields are helper-owned, not Settings registry descriptors.
            // Publish the revision-fenced document through the same lane as QML;
            // preserving every field avoids deleting unrelated provider preferences.
            state.action = self.begin("settings.provider", params, Some(wake));
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
    /// Resolve advertised cache paths; keep encoded leases only for this scene's images.
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
            if primary_image.is_none() && !state.catalogue_settled_handles.contains(&logical) {
                visible_handles.insert(logical.clone());
            }
            let loaded = primary_image.zip(primary_file).or_else(|| {
                let file = fallback_file?;
                cached_wallpaper_image(&mut state, &file, LIMIT).map(|bytes| (bytes, file))
            });
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
        scene.retain_primitives(|primitive| {
            !matches!(primitive, bead::render::Primitive::Image { path, .. }
                if path.to_str().is_some_and(|name| name.starts_with("@icon:wallpaper:")))
        });
        if self.kind == Kind::Wallpaper {
            // Luau includes the visible cards and their neighbours in this scene.
            // Do not keep encoded files from every earlier carousel position warm.
            state
                .images
                .retain(|path, _| scene.asset_data.contains_key(path));
            state.image_bytes = state
                .images
                .iter()
                .map(|(path, bytes)| bytes.len() + path.capacity() + 128)
                .sum();
        }
        // Persisted visible thumbnails are already usable content. Request
        // catalogue/palette work only for visible assets that missed the local
        // cache, so a normal cache-hit open does not start helper children.
        let request_handles: std::collections::BTreeSet<String> = visible_handles
            .difference(&state.catalogue_settled_handles)
            .cloned()
            .collect();
        state.catalogue_pending_handles = visible_handles;
        if self.kind == Kind::Wallpaper
            && state.wallpaper_open
            && state.catalogue.is_none()
            && let Some(wake) = wake
            && !request_handles.is_empty()
        {
            let mut ids = Vec::new();
            for handle in &request_handles {
                if let Some((id, _)) = parse_wallpaper_handle(handle)
                    && !ids.iter().any(|known: &String| known == id)
                {
                    ids.push(id.to_owned());
                    if ids.len() == 128 {
                        break;
                    }
                }
            }
            let requested_handles: Vec<String> = request_handles
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
                if state.catalogue.is_some() {
                    state.catalogue_request_boot = state.wallpaper_boot.clone();
                }
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
    pub fn content_ready(&self) -> bool {
        // Wallpaper metadata and thumbnails arrive progressively. Never hold the
        // panel's first commit on catalogue or image work; loading/empty states are
        // valid content while the asynchronous requests run.
        true
    }

    pub fn image_bytes(&self) -> usize {
        self.state.borrow().image_bytes
    }

    fn wallpaper_preference(&self, key: &str, value: &str) -> Option<Json> {
        let snapshot = self.wallpaper_settings.as_ref()?.snapshot();
        if !snapshot.connected || snapshot.data.get("error").is_some_and(|e| !e.is_null()) {
            return None;
        }
        let rev = snapshot.data["rev"].as_u64()?;
        let mut data = snapshot.data["data"].clone();
        let document = data.as_object_mut()?;
        // The published wallpaper-picker v1 state schema has keepVersion=false:
        // snapshots omit it, but settings.provider requires it on every write.
        document.insert("version".into(), json!(1));
        document.insert(key.into(), json!(value));
        Some(json!({"store":"wallpaper-picker","value":data,"rev":rev}))
    }
}

fn wallpaper_source_metadata(path: &str) -> (u64, u32, u32) {
    let bytes = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let dimensions = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
        .unwrap_or_default();
    (bytes, dimensions.0, dimensions.1)
}

fn raw_catalogue_palette<'a>(catalogue: &'a Json, id: &str) -> Option<&'a Json> {
    catalogue["entries"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"].as_str() == Some(id))?
        .get("palette")
        .filter(|palette| valid_catalogue_palette(palette))
}

fn valid_catalogue_palette(palette: &Json) -> bool {
    let Some(object) = palette.as_object() else {
        return false;
    };
    if object.len() != 2
        || !matches!(
            object.get("mode").and_then(Json::as_str),
            Some("light" | "dark")
        )
    {
        return false;
    }
    let Some(colours) = object.get("colours").and_then(Json::as_object) else {
        return false;
    };
    if colours.is_empty()
        || colours.len() > 64
        || !["background", "onSurface", "primary"]
            .iter()
            .all(|key| colours.contains_key(*key))
    {
        return false;
    }
    colours.iter().all(|(role, value)| {
        role.len() <= 64
            && !role.is_empty()
            && value.as_str().is_some_and(|colour| {
                colour.len() == 7
                    && colour.starts_with('#')
                    && colour[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    })
}

fn trim_catalogue_palette_cache(entries: &mut [Json], requested_ids: &[String]) {
    let has_palette = |entry: &Json| entry.get("palette").is_some_and(valid_catalogue_palette);
    let mut keep = std::collections::BTreeSet::new();
    for entry in entries.iter().filter(|entry| entry["active"] == true) {
        if keep.len() == CATALOGUE_PALETTE_CACHE_LIMIT {
            break;
        }
        if has_palette(entry)
            && let Some(id) = entry["id"].as_str()
        {
            keep.insert(id.to_owned());
        }
    }
    for id in requested_ids {
        if keep.len() == CATALOGUE_PALETTE_CACHE_LIMIT {
            break;
        }
        if entries
            .iter()
            .any(|entry| entry["id"].as_str() == Some(id) && has_palette(entry))
        {
            keep.insert(id.clone());
        }
    }
    for entry in entries.iter() {
        if keep.len() == CATALOGUE_PALETTE_CACHE_LIMIT {
            break;
        }
        if has_palette(entry)
            && let Some(id) = entry["id"].as_str()
        {
            keep.insert(id.to_owned());
        }
    }
    for entry in entries.iter_mut() {
        if has_palette(entry)
            && !entry["id"].as_str().is_some_and(|id| keep.contains(id))
            && let Some(object) = entry.as_object_mut()
        {
            object.remove("palette");
        }
    }
}

fn wallpaper_switch_phase(selection: &WallpaperSelection, phase: &str) {
    println!(
        "WALLPAPER_SWITCH_PHASE revision={} phase={} ms_since_selection={:.3} file_bytes={} source_width={} source_height={}",
        selection.revision,
        phase,
        selection.selected_at.elapsed().as_secs_f64() * 1000.0,
        selection.file_bytes,
        selection.source_width,
        selection.source_height,
    );
}
fn ordered<'a>(items: &'a Json, order: &'a Json) -> impl Iterator<Item = (&'a str, &'a Json)> {
    order
        .as_array()
        .into_iter()
        .flatten()
        .take(2048)
        .filter_map(move |id| Some((id.as_str()?, items.get(id.as_str()?)?)))
}
pub(crate) fn parse_wallpaper_handle(handle: &str) -> Option<(&str, &str)> {
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
    let bytes = read_wallpaper_image(file)?;
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
pub(crate) fn read_wallpaper_image(file: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = crate::desktop::read_icon_file(file, 2 * bead::core::cache::MIB as u64).ok()?;
    let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 1600 || dimensions.1 > 1000 {
        return None;
    }
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
    preview: Option<Arc<crate::wallpaper::Preview>>,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    let wallpaper_actions = (kind == Kind::Wallpaper).then(|| {
        let mut queue = WallpaperActivationQueue::new(client.clone());
        queue.preview = preview;
        Arc::new(std::sync::Mutex::new(queue))
    });
    let mut cold = crate::sound::fallback(colors.get("surface")?);
    if let Some(motion) = &mut cold.motion {
        motion.edge = bead::core::Edge::Bottom;
    }
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::SceneEdge(bead::core::Edge::Bottom),
        )),
        engages_decorations: true,
        overlay: kind == Kind::Wallpaper,
        retain_when_closed: kind == Kind::Wallpaper,
        passive: false,
        keyboard_focus: true,
        text_focus_only: false,
        initial_open: false,
        fallback: cold.clone(),
        fallback_factory: if kind == Kind::Wallpaper {
            Some(Arc::new(|viewport| {
                crate::wallpaper_warm::prepared_fallback_scene(viewport)
            }))
        } else {
            None
        },
        renderer: if kind == Kind::Wallpaper && cfg!(feature = "native-gl") {
            bead::render::RendererKind::FemtovgGl
        } else {
            bead::render::RendererKind::Cpu
        },
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
                    let extras = match &wallpaper_actions {
                        Some(actions) => extras.with_wallpaper_actions(actions.clone()),
                        None => extras,
                    };
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
