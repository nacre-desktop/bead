// SPDX-License-Identifier: GPL-3.0-only
//! B0060 Colors app adapter: the wallpaper colour explorer as an on-demand window (B0052).
//! The app follows `settings.store.colors` and `wallpaper.active` and calls the colours
//! methods the daemon advertises (`colors.catalog`, `colors.preview`, `colors.apply`,
//! `colors.export`; the shapes are in the B0060 spec). Without them it shows the colours of
//! the active wallpaper and nothing can be applied. Layout is `shell/colors/*.luau`; this
//! file projects plain data, runs the one-at-a-time preview loop and decodes the daemon's
//! cached thumbnails. Nothing is resident until the window opens.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, Client};
use crate::player_art::{self, Cache};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use serde_json::{Map, Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
    time::Duration,
};

pub const CATALOG: &str = "colors.catalog";
pub const PREVIEW: &str = "colors.preview";
pub const APPLY: &str = "colors.apply";
pub const EXPORT: &str = "colors.export";
pub const STORE: &str = "settings.store.colors";
pub const ACTIVE: &str = "wallpaper.active";
/// Topics the app process follows besides the ones every app does.
pub const TOPICS: [&str; 2] = [STORE, ACTIVE];

/// Daemon-cached PNGs: list thumbnails (at most 256 px) and the preview poster (at most 1280 px).
const THUMBS: Cache = Cache {
    dir: "nacre/shell/colors-thumbs",
    max_file: 256 * 1024,
    w: 0,
    h: 0,
};
const POSTERS: Cache = Cache {
    dir: "nacre/shell/colors-preview",
    max_file: 2 * 1024 * 1024,
    w: 0,
    h: 0,
};
const MAX_ENTRIES: usize = 512;
const MAX_THUMBS: usize = 40;
const MAX_ASSETS: usize = 64;
const MAX_ERROR: usize = 240;
/// Roles the miniature desktop draws; the view receives only these.
const CARD_ROLES: [&str; 17] = [
    "frame",
    "outline",
    "primary",
    "secondary",
    "tertiary",
    "primaryFixed",
    "onSurface",
    "onPrimary",
    "onSecondary",
    "onTertiary",
    "surfaceContainer",
    "surfaceContainerHigh",
    "surfaceContainerLowest",
    "green",
    "yellow",
    "primaryContainer",
    "onPrimaryContainer",
];
const ACCENTS: [(&str, &str, &str); 3] = [
    (
        "primary",
        "bead.colors.target-main",
        "bead.colors.lock-main",
    ),
    (
        "secondary",
        "bead.colors.target-second",
        "bead.colors.lock-second",
    ),
    (
        "tertiary",
        "bead.colors.target-third",
        "bead.colors.lock-third",
    ),
];
const STYLES: [(&str, &str, &str); 7] = [
    (
        "natural",
        "bead.colors.style-natural",
        "bead.colors.style-natural-text",
    ),
    (
        "pigment",
        "bead.colors.style-pigment",
        "bead.colors.style-pigment-text",
    ),
    (
        "harmony",
        "bead.colors.style-harmony",
        "bead.colors.style-harmony-text",
    ),
    ("pop", "bead.colors.style-pop", "bead.colors.style-pop-text"),
    (
        "mist",
        "bead.colors.style-mist",
        "bead.colors.style-mist-text",
    ),
    (
        "vivid",
        "bead.colors.style-vivid",
        "bead.colors.style-vivid-text",
    ),
    (
        "pearl",
        "bead.colors.style-pearl",
        "bead.colors.style-pearl-text",
    ),
];
/// The eighth style, which the Colors window only reaches through the time-of-day tint.
const TIDE: (&str, &str, &str) = (
    "tide",
    "bead.colors.style-tide",
    "bead.colors.style-tide-text",
);
const DEMO_SCENES: usize = 4;
const VISIONS: [(&str, &str); 4] = [
    ("normal", "bead.colors.vision-normal"),
    ("protan", "bead.colors.vision-protan"),
    ("deutan", "bead.colors.vision-deutan"),
    ("tritan", "bead.colors.vision-tritan"),
];
const PAGES: [&str; 3] = ["studio", "compare", "accessibility"];
/// Option keys a request may carry (the rest of a saved choice is ignored).
const KEYS: [&str; 11] = [
    "mode",
    "personality",
    "backgroundFromWallpaper",
    "overrides",
    "brightness",
    "saturation",
    "autoMode",
    "tideAutomatic",
    "timeTint",
    "workspaceColors",
    "accent",
];

/// `#rrggbb` or `rrggbb` as an opaque RGBA array.
fn rgba(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?, 255])
}
fn hex6(text: &str) -> Option<String> {
    rgba(text).map(|_| text.trim_start_matches('#').to_ascii_lowercase())
}
fn clip(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}
fn local_hour() -> i64 {
    // SAFETY: localtime_r writes only into the zeroed `tm` it is given.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return 12;
        }
        i64::from(tm.tm_hour).clamp(0, 23)
    }
}
/// A colour map `{role: hex}` as `{role: [r,g,b,255]}` for the roles in `only` (all when empty).
pub(crate) fn colour_map(map: &Json, only: &[&str]) -> Json {
    let mut out = Map::new();
    for (role, value) in map.as_object().into_iter().flatten() {
        if (only.is_empty() || only.contains(&role.as_str()))
            && let Some(color) = value.as_str().and_then(rgba)
        {
            out.insert(role.clone(), json!(color));
        }
    }
    Json::Object(out)
}

struct Entry {
    path: String,
    name: String,
    dynamic: bool,
    thumbnail: String,
}
#[derive(Default)]
struct Catalog {
    current: String,
    entries: Vec<Entry>,
}
impl Catalog {
    fn parse(value: &Json) -> Self {
        let entries = value["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_ENTRIES)
            .filter_map(|e| {
                Some(Entry {
                    path: e["path"].as_str().filter(|p| p.starts_with('/'))?.into(),
                    name: clip(e["name"].as_str().unwrap_or(""), 120),
                    dynamic: e["dynamic"] == json!(true),
                    thumbnail: e["thumbnail"].as_str().unwrap_or("").into(),
                })
            })
            .collect();
        Self {
            current: value["current"].as_str().unwrap_or("").into(),
            entries,
        }
    }
}

struct Flight {
    ticket: CallTicket,
    generation: u64,
}
#[derive(Default)]
struct State {
    /// Offline demo palettes, keyed by image/mode/style; empty in the Colors app.
    demo_cache: BTreeMap<String, Json>,
    catalog: Option<Catalog>,
    catalog_call: Option<CallTicket>,
    catalog_failed: bool,
    /// The request being explored; `Null` until a wallpaper is chosen.
    options: Json,
    preview: Option<Json>,
    generation: u64,
    queued: bool,
    flight: Option<Flight>,
    action: Option<(CallTicket, &'static str)>,
    status: String,
    error: String,
    seen_open: String,
    /// The accent role waiting for the next sample (no drag and drop here).
    armed: Option<String>,
}
/// Decoded cache images and the PNGs made from them for each slot size.
#[derive(Default)]
struct Pictures {
    poster: Option<(String, Option<image::DynamicImage>)>,
    posters: BTreeMap<String, image::DynamicImage>,
    thumbs: BTreeMap<String, Option<image::DynamicImage>>,
    rendered: BTreeMap<String, Arc<[u8]>>,
}
type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Colors {
    client: Option<Client>,
    store: Option<Arc<dyn Source>>,
    active: Option<Arc<dyn Source>>,
    subscriptions: RefCell<Vec<Subscription>>,
    wake: RefCell<Option<Wake>>,
    state: RefCell<State>,
    pictures: RefCell<Pictures>,
    width: Cell<u32>,
    /// The last painted size of the photo slot, to map a click to image coordinates.
    photo_box: Cell<(f32, f32)>,
}

impl Colors {
    pub fn new(client: Option<Client>) -> Self {
        let follow = |name: &str| {
            client
                .as_ref()
                .and_then(|c| c.topic(name).ok())
                .map(|t| Arc::new(t) as Arc<dyn Source>)
        };
        Self {
            store: follow(STORE),
            active: follow(ACTIVE),
            client,
            subscriptions: Default::default(),
            wake: Default::default(),
            state: Default::default(),
            pictures: Default::default(),
            width: Cell::new(1100),
            photo_box: Cell::new((0., 0.)),
        }
    }
    /// Welcome's bundled demo is ready before its first frame, including cold deep links.
    /// Generated through Nacre's helper, never through an engine implementation in Bead.
    pub fn bundled_demo(root: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let folder = root.join("welcome/demo");
        let bundle: Json = serde_json::from_slice(&std::fs::read(folder.join("palettes.json"))?)?;
        let scenes = bundle["scenes"]
            .as_array()
            .filter(|s| s.len() == DEMO_SCENES)
            .ok_or("Incomplete bundled Welcome scenes")?;
        let demo = Self::new(None);
        let mut state = demo.state.borrow_mut();
        let mut pictures = demo.pictures.borrow_mut();
        let mut catalog = Catalog::default();
        for (index, scene) in scenes.iter().enumerate() {
            let path = folder.join(scene["image"].as_str().ok_or("Missing demo image")?);
            let thumb = folder.join(
                scene["thumbnail"]
                    .as_str()
                    .ok_or("Missing demo thumbnail")?,
            );
            let (path, thumb) = (
                path.to_string_lossy().into_owned(),
                thumb.to_string_lossy().into_owned(),
            );
            pictures.posters.insert(path.clone(), image::open(&path)?);
            pictures
                .thumbs
                .insert(thumb.clone(), Some(image::open(&thumb)?));
            for mode in ["dark", "light"] {
                let palettes = scene["palettes"][mode]
                    .as_object()
                    .filter(|p| p.len() == 8)
                    .ok_or("Incomplete bundled Welcome palettes")?;
                for (style, colours) in palettes {
                    if CARD_ROLES
                        .iter()
                        .any(|role| colours[*role].as_str().and_then(rgba).is_none())
                    {
                        return Err("Invalid bundled Welcome colour role".into());
                    }
                    let request = json!({"image": path, "mode": mode, "personality": style});
                    state.demo_cache.insert(
                        demo_key(&request),
                        json!({
                        "id": format!("demo-{index}-{mode}-{style}"), "request": request,
                        "colours": colours, "thumbnail": path}),
                    );
                }
            }
            catalog.entries.push(Entry {
                path,
                thumbnail: thumb,
                name: scene["name"].as_str().unwrap_or("").into(),
                dynamic: false,
            });
        }
        let first = catalog.entries[0].path.clone();
        state.catalog = Some(catalog);
        demo.choose(&mut state, &first, false);
        drop(pictures);
        drop(state);
        demo.refresh_poster(&demo.state.borrow());
        Ok(demo)
    }
    #[cfg(test)]
    pub fn with_sources(mut self, store: Arc<dyn Source>, active: Arc<dyn Source>) -> Self {
        self.store = Some(store);
        self.active = Some(active);
        self
    }
    pub fn set_viewport(&self, width: u32) {
        self.width.set(width);
    }
    /// Topic changes wake the window.
    pub fn subscribe(&self, wake: Wake) {
        let mut held = self.subscriptions.borrow_mut();
        held.clear();
        for source in [&self.store, &self.active].into_iter().flatten() {
            let wake = wake.clone();
            if let Ok(subscription) = source.subscribe(Arc::new(move |_| wake())) {
                held.push(subscription);
            }
        }
        *self.wake.borrow_mut() = Some(wake);
    }

    // ---- daemon -----------------------------------------------------------------

    fn ready(&self, method: &str) -> Option<(String, Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot.clone()?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 60_000)),
        ))
    }
    fn call(&self, method: &str, params: Json) -> Option<CallTicket> {
        let (boot, timeout) = self.ready(method)?;
        self.client
            .as_ref()?
            .begin_call(
                method.into(),
                params,
                timeout,
                CallGuard { boot, version: 1 },
                self.wake.borrow().clone(),
            )
            .ok()
    }
    fn explorer(&self) -> bool {
        self.ready(CATALOG).is_some() && self.ready(PREVIEW).is_some()
    }
    fn saved(&self) -> Json {
        self.store
            .as_ref()
            .map(|s| s.snapshot())
            .filter(|s| s.connected)
            .map(|s| s.data["data"].clone())
            .unwrap_or(Json::Null)
    }
    /// The pair the compositor shows now: its image path, mode and colour map.
    fn active(&self) -> (String, String, Json) {
        let Some(snapshot) = self.active.as_ref().map(|s| s.snapshot()) else {
            return Default::default();
        };
        let data = &*snapshot.data;
        let pair = if data["current"].is_object() {
            &data["current"]
        } else {
            data["outputs"]
                .as_object()
                .and_then(|o| o.values().next())
                .unwrap_or(&Json::Null)
        };
        (
            pair["path"].as_str().unwrap_or("").into(),
            pair["palette"]["mode"].as_str().unwrap_or("dark").into(),
            pair["palette"]["colours"].clone(),
        )
    }

    // ---- request state ----------------------------------------------------------

    /// The request for a freshly chosen wallpaper: defaults, its saved choice, the image.
    fn choose(&self, st: &mut State, image: &str, compare: bool) {
        let saved = self.saved()["wallpapers"][image].clone();
        let mode = if st.demo_cache.is_empty() {
            self.active().1
        } else {
            st.options["mode"].as_str().unwrap_or("dark").into()
        };
        let mut options = json!({
            "mode": mode, "personality": "natural", "backgroundFromWallpaper": false,
            "overrides": {}, "brightness": 0, "saturation": 1, "hour": local_hour(),
            "autoMode": false, "tideAutomatic": false, "timeTint": false, "workspaceColors": false
        });
        for key in KEYS {
            if !saved[key].is_null() {
                options[key] = saved[key].clone();
            }
        }
        // Older choices stored a personality instead of the background switch.
        if saved["backgroundFromWallpaper"].is_null()
            && let Some(personality) = saved["personality"].as_str()
        {
            options["backgroundFromWallpaper"] = json!(personality != "natural");
        }
        if options["personality"] == "source" {
            options["personality"] = "natural".into();
        }
        options["image"] = image.into();
        options["compare"] = compare.into();
        st.options = options;
        st.armed = None;
        self.changed(st);
    }
    fn changed(&self, st: &mut State) {
        st.generation += 1;
        st.queued = true;
        if let Some(preview) = st.demo_cache.get(&demo_key(&st.options)) {
            st.preview = Some(preview.clone());
            st.queued = false;
        }
        st.error.clear();
        st.status.clear();
    }
    /// Merge `values` into the request; a no-op change does nothing.
    fn change(&self, st: &mut State, values: &[(&str, Json)]) {
        if st.options.is_null() {
            return;
        }
        let mut next = st.options.clone();
        for (key, value) in values {
            next[*key] = value.clone();
        }
        if values.iter().any(|(k, _)| {
            [
                "personality",
                "backgroundFromWallpaper",
                "overrides",
                "brightness",
                "saturation",
                "accent",
                "pick",
            ]
            .contains(k)
        }) {
            next.as_object_mut().map(|o| o.remove("favoriteId"));
        }
        if next == st.options && st.error.is_empty() && (st.preview.is_some() || st.queued) {
            return;
        }
        st.options = next;
        self.changed(st);
    }
    fn set_role(&self, st: &mut State, role: &str, value: Option<&str>) {
        let mut overrides = st.options["overrides"].clone();
        if !overrides.is_object() {
            overrides = json!({});
        }
        match value {
            Some(hex) => overrides[role] = hex.into(),
            None => {
                overrides.as_object_mut().map(|o| o.remove(role));
            }
        }
        self.change(st, &[("overrides", overrides)]);
    }

    /// Polls finished calls and starts the next one: the catalog once, then one preview at a
    /// time (a newer request replaces a queued one, an older reply is dropped).
    fn pump(&self) {
        let mut guard = self.state.borrow_mut();
        let st = &mut *guard;
        if let Some(ticket) = &mut st.catalog_call
            && let Some(reply) = ticket.try_take()
        {
            st.catalog_call = None;
            match reply {
                Ok(value) => {
                    let catalog = Catalog::parse(&value);
                    if st.options.is_null() {
                        let first = Some(catalog.current.clone())
                            .filter(|c| !c.is_empty())
                            .or_else(|| catalog.entries.first().map(|e| e.path.clone()));
                        st.catalog = Some(catalog);
                        if let Some(image) = first {
                            self.choose(st, &image, false);
                        }
                    } else {
                        st.catalog = Some(catalog);
                    }
                }
                Err(_) => {
                    st.catalog_failed = true;
                    st.error = tr("bead.colors.error-catalog", &[]);
                }
            }
        }
        if st.catalog.is_none()
            && st.catalog_call.is_none()
            && !st.catalog_failed
            && self.explorer()
        {
            st.catalog_call = self.call(CATALOG, json!({}));
        }
        if let Some(flight) = &mut st.flight
            && let Some(reply) = flight.ticket.try_take()
        {
            let current = flight.generation == st.generation;
            st.flight = None;
            if current {
                match reply {
                    Ok(result) if result["id"].is_string() && result["colours"].is_object() => {
                        if result["request"].is_object() {
                            st.options = result["request"].clone();
                        }
                        st.preview = Some(result);
                    }
                    Ok(_) => st.error = tr("bead.colors.error-preview-invalid", &[]),
                    Err(e) => {
                        st.error = if e.message.is_empty() {
                            tr("bead.colors.error-preview", &[])
                        } else {
                            clip(&e.message, MAX_ERROR)
                        }
                    }
                }
            }
        }
        // N0050: a newer preview cancels the running older one in the daemon (its call fails
        // `busy`, which is dropped as stale), so the newest request goes out at once.
        if st.queued && st.options["image"].is_string() && self.ready(PREVIEW).is_some() {
            st.queued = false;
            let generation = st.generation;
            match self.call(PREVIEW, json!({"request": st.options})) {
                Some(ticket) => st.flight = Some(Flight { ticket, generation }),
                None => st.error = tr("bead.colors.error-preview", &[]),
            }
        }
        if let Some((ticket, what)) = &mut st.action
            && let Some(reply) = ticket.try_take()
        {
            let what = *what;
            st.action = None;
            match reply {
                Ok(result) if what == "apply" && result["applied"] == json!(false) => {
                    st.error = tr("bead.colors.error-action", &[]);
                }
                Ok(_) if what == "apply" => st.status = tr("bead.colors.status-applied", &[]),
                Ok(result) => {
                    st.status = tr(
                        "bead.colors.status-exported",
                        &[(
                            "directory",
                            &clip(result["directory"].as_str().unwrap_or(""), 120),
                        )],
                    )
                }
                Err(e) if e.code == "busy" => st.error = tr("bead.colors.error-busy", &[]),
                Err(e) => {
                    st.error = if e.message.is_empty() {
                        tr("bead.colors.error-action", &[])
                    } else {
                        clip(&e.message, MAX_ERROR)
                    }
                }
            }
        }
    }

    fn applied(&self, st: &State) -> bool {
        let Some(preview) = &st.preview else {
            return false;
        };
        let (path, _, colours) = self.active();
        let surface = |map: &Json| map["surface"].as_str().and_then(hex6);
        preview["id"].as_str() == self.saved()["lastApplied"].as_str()
            && !path.is_empty()
            && Some(path.as_str()) == preview["image"].as_str()
            && surface(&colours).is_some()
            && surface(&colours) == surface(&preview["colours"])
    }
    fn ready_to_act(&self, st: &State) -> bool {
        st.preview
            .as_ref()
            .is_some_and(|p| p["request"] == st.options)
            && st.flight.is_none()
            && !st.queued
            && st.error.is_empty()
            && st.action.is_none()
    }

    // ---- projection -------------------------------------------------------------

    /// The poster (the large preview) is decoded once per path.
    fn refresh_poster(&self, st: &State) {
        let path = st.preview.as_ref().and_then(|p| p["thumbnail"].as_str());
        let mut pictures = self.pictures.borrow_mut();
        if pictures.poster.as_ref().map(|(held, _)| held.as_str()) != path {
            if st.demo_cache.is_empty() {
                pictures.rendered.clear();
            }
            pictures.poster = path.map(|p| {
                (
                    p.to_owned(),
                    pictures
                        .posters
                        .get(p)
                        .cloned()
                        .or_else(|| player_art::decode(&POSTERS, p)),
                )
            });
        }
    }

    /// Welcome's offline demo: the bundled palette and pixels are prepared at app startup.
    pub fn demo(&self) -> Json {
        self.refresh_poster(&self.state.borrow());
        let st = self.state.borrow();
        let Some(preview) = st.preview.as_ref() else {
            return json!({"id": "demo", "available": false, "loading": false,
                "error": st.error});
        };
        let catalog = st.catalog.as_ref();
        let mut pictures = self.pictures.borrow_mut();
        let mut scenes = Vec::new();
        let picks = catalog
            .into_iter()
            .flat_map(|c| c.entries.iter().enumerate());
        let statics = picks.clone().filter(|(_, e)| !e.dynamic);
        let rest = picks.filter(|(_, e)| e.dynamic);
        for (index, entry) in statics.chain(rest).take(DEMO_SCENES) {
            let held = pictures
                .thumbs
                .entry(entry.thumbnail.clone())
                .or_insert_with(|| player_art::decode(&THUMBS, &entry.thumbnail));
            scenes.push(json!({"id": index.to_string(), "name": entry.name,
                "selected": st.options["image"].as_str() == Some(entry.path.as_str()),
                "thumb": if held.is_some() { format!("colors-thumb-{index}") } else { String::new() },
                "action": format!("demo:choose:{index}")}));
        }
        let personality = st.options["personality"].as_str().unwrap_or("natural");
        let styles: Vec<Json> = STYLES
            .iter()
            .chain(std::iter::once(&TIDE))
            .map(|(id, label, _)| json!({"id": *id, "label": tr(label, &[]), "selected": personality == *id}))
            .collect();
        let style_text = STYLES
            .iter()
            .find(|(id, ..)| *id == personality)
            .map_or_else(|| tr(TIDE.2, &[]), |(.., text)| tr(text, &[]));
        let colours = &preview["colours"];
        let swatches: Vec<Json> = ["primary", "secondary", "tertiary", "surface"]
            .iter()
            .filter_map(|role| {
                Some(json!({"role": *role, "color": colours[*role].as_str().and_then(rgba)?}))
            })
            .collect();
        let has_photo = pictures
            .poster
            .as_ref()
            .is_some_and(|(_, image)| image.is_some());
        json!({"id": "demo", "available": true, "loading": false, "scenes": scenes, "styles": styles,
            "style_text": style_text, "mode": st.options["mode"].as_str().unwrap_or("dark"),
            "colors": colour_map(colours, &CARD_ROLES), "swatches": swatches, "has_photo": has_photo,
            "busy": st.queued || st.flight.is_some(), "error": st.error})
    }

    fn entries(&self, st: &State, query: &str) -> Vec<Json> {
        let Some(catalog) = &st.catalog else {
            return Vec::new();
        };
        let needle = query.trim().to_lowercase();
        let mut pictures = self.pictures.borrow_mut();
        let mut shown = 0;
        let mut out = Vec::new();
        for (index, entry) in catalog.entries.iter().enumerate() {
            if !needle.is_empty() && !entry.name.to_lowercase().contains(&needle) {
                continue;
            }
            let thumb = if shown < MAX_THUMBS {
                let held = pictures
                    .thumbs
                    .entry(entry.thumbnail.clone())
                    .or_insert_with(|| player_art::decode(&THUMBS, &entry.thumbnail));
                held.is_some()
            } else {
                false
            };
            shown += 1;
            out.push(json!({
                "id": index.to_string(), "name": entry.name,
                "kind": tr(if entry.dynamic { "bead.colors.animated" } else { "bead.colors.static" }, &[]),
                "selected": st.options["image"].as_str() == Some(entry.path.as_str()),
                "thumb": if thumb { format!("colors-thumb-{index}") } else { String::new() },
                "action": format!("choose:{index}"),
            }));
        }
        out
    }

    fn view(&self, st: &State, vision: &str, fine: bool) -> Json {
        let explorer = self.explorer();
        let preview = st.preview.as_ref();
        let (active_path, _, active_colours) = self.active();
        let colours = preview.map_or(&active_colours, |p| &p["colours"]);
        let busy = st.queued || st.flight.is_some();
        let name = preview
            .and_then(|p| p["name"].as_str())
            .map(String::from)
            .or_else(|| {
                std::path::Path::new(&active_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| tr("bead.colors.explore", &[]));
        let subtitle = if !explorer && preview.is_none() {
            tr("bead.colors.unavailable", &[])
        } else if busy {
            tr("bead.colors.preparing", &[])
        } else if preview.is_some_and(|p| p["frames"].as_u64().unwrap_or(1) > 1) {
            tr("bead.colors.animated", &[])
        } else {
            tr("bead.colors.pick-hint", &[])
        };
        let options = &st.options;
        let overrides = &options["overrides"];
        // Samples: the image's colour families, or the active palette's accents without a preview.
        let mut samples = Vec::new();
        let mut other = 0.;
        if let Some(p) = preview {
            for (i, item) in p["imageColors"]
                .as_array()
                .into_iter()
                .flatten()
                .take(8)
                .enumerate()
            {
                let Some(color) = item["hex"].as_str().and_then(rgba) else {
                    continue;
                };
                let coverage = item["coverage"].as_f64().unwrap_or(0.);
                let percent = format!("{}%", (coverage * 1000.).round() / 10.);
                samples.push(json!({"id": format!("s{i}"), "color": color, "caption": percent,
                    "aria_label": tr("bead.colors.sample-aria", &[("hex", item["hex"].as_str().unwrap_or("")), ("percent", &percent)]),
                    "action": format!("sample:{}", item["hex"].as_str().and_then(hex6).unwrap_or_default())}));
            }
            other = p["otherCoverage"].as_f64().unwrap_or(0.);
        } else {
            for (i, role) in ["primary", "secondary", "tertiary", "surface", "onSurface"]
                .iter()
                .enumerate()
            {
                if let Some(color) = active_colours[*role].as_str().and_then(rgba) {
                    samples.push(json!({"id": format!("s{i}"), "color": color, "caption": *role, "aria_label": *role}));
                }
            }
        }
        let targets: Vec<Json> = ACCENTS
            .iter()
            .filter_map(|(role, label, lock)| {
                let hex = colours[*role].as_str().and_then(hex6)?;
                let pinned = overrides[*role].is_string();
                Some(json!({"id": *role, "label": tr(label, &[]), "value": format!("#{hex}"),
                    "color": rgba(&hex), "locked": pinned, "armed": st.armed.as_deref() == Some(*role),
                    "lock_label": tr(lock, &[]),
                    "arm": format!("arm:{role}"), "lock": format!("lock:{role}")}))
            })
            .collect();
        let personality = options["personality"].as_str().unwrap_or("natural");
        let favorite = options["favoriteId"].is_string();
        let styles: Vec<Json> = STYLES
            .iter()
            .map(|(id, label, _)| json!({"id": *id, "label": tr(label, &[]), "selected": !favorite && personality == *id}))
            .collect();
        let style_text = STYLES
            .iter()
            .find(|(id, ..)| *id == personality)
            .map_or_else(
                || tr("bead.colors.style-tide-text", &[]),
                |(.., text)| tr(text, &[]),
            );
        let audit = preview.map_or(&Json::Null, |p| &p["audit"]);
        let sim = |role: &str| {
            if role == "normal" || audit["simulated"][role].is_null() {
                colour_map(colours, &CARD_ROLES)
            } else {
                colour_map(&audit["simulated"][role], &CARD_ROLES)
            }
        };
        let compare: Vec<Json> = preview
            .into_iter()
            .flat_map(|p| p["comparisons"].as_array().into_iter().flatten())
            .take(8)
            .filter_map(|c| {
                let id = c["personality"].as_str()?;
                let label = STYLES.iter().find(|(s, ..)| *s == id).map(|(_, l, _)| tr(l, &[]))?;
                let contrast = c["minimumContrast"].as_f64().map(|v| {
                    tr("bead.colors.compare-contrast", &[("contrast", &format!("{}", (v * 100.).round() / 100.))])
                });
                Some(json!({"id": id, "label": label, "colors": colour_map(&c["colours"], &CARD_ROLES),
                    "contrast": contrast.unwrap_or_default()}))
            })
            .collect();
        let rows: Vec<Json> = audit["text"]
            .as_array()
            .into_iter()
            .flatten()
            .take(40)
            .enumerate()
            .map(|(i, r)| {
                let ratio = r["ratio"]
                    .as_f64()
                    .map(|v| format!("{}", (v * 100.).round() / 100.))
                    .unwrap_or_default();
                json!({"id": format!("r{i}"), "text": tr("bead.colors.access-row", &[
                    ("foreground", r["foreground"].as_str().unwrap_or("")),
                    ("background", r["background"].as_str().unwrap_or("")),
                    ("ratio", &ratio)])})
            })
            .collect();
        let contrast = audit["minimumTextContrast"]
            .as_f64()
            .map(|v| format!("{}", (v * 100.).round() / 100.))
            .unwrap_or_else(|| "0".into());
        let warnings: Vec<Json> = audit["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .take(8)
            .enumerate()
            .filter_map(|(i, w)| {
                w.as_str()
                    .map(|w| json!({"id": format!("w{i}"), "text": clip(w, 200)}))
            })
            .collect();
        let applied = self.applied(st);
        let acting = st.action.is_some();
        let go = self.ready_to_act(st);
        let status_error = !st.error.is_empty();
        let status = if status_error {
            st.error.clone()
        } else if !st.status.is_empty() {
            st.status.clone()
        } else if explorer {
            tr("bead.colors.preview-only", &[])
        } else {
            String::new()
        };
        let pictures = self.pictures.borrow();
        let has_photo = pictures
            .poster
            .as_ref()
            .is_some_and(|(_, image)| image.is_some());
        json!({
            "id": "view", "available": explorer, "title": name, "subtitle": subtitle,
            "has_photo": has_photo, "samples": samples,
            "other": if other > 0.005 { tr("bead.colors.other-shades", &[("percent", &format!("{}", (other * 100.).round()))]) } else { String::new() },
            "styles": styles, "style_text": style_text,
            "colors": colour_map(colours, &CARD_ROLES),
            "readable": tr(if audit["textPasses"] == json!(true) { "bead.colors.readable" } else { "bead.colors.check-readability" }, &[]),
            "targets": targets, "fine": fine,
            "tint_wallpaper": options["backgroundFromWallpaper"] == json!(true),
            "tint_wallpaper_enabled": personality != "pearl",
            "brightness": options["brightness"].as_f64().unwrap_or(0.),
            "saturation": options["saturation"].as_f64().unwrap_or(1.),
            "time_tint": options["timeTint"] == json!(true),
            "workspace_colors": options["workspaceColors"] == json!(true),
            "mode": options["mode"].as_str().unwrap_or(&self.active().1),
            "compare": compare,
            "vision": VISIONS.iter().map(|(id, label)| json!({"id": *id, "label": tr(label, &[]), "selected": vision == *id})).collect::<Vec<_>>(),
            "vision_colors": sim(vision),
            "contrast": tr("bead.colors.access-contrast", &[("contrast", &contrast)]),
            "warnings": warnings, "rows": rows,
            "can_apply": self.ready(APPLY).is_some() && go && !applied,
            "can_export": self.ready(EXPORT).is_some() && go,
            "applied": applied, "busy": busy || acting,
            "status": status, "status_error": status_error,
        })
    }

    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let text = |name: &str| match stores.get(name) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let flag = |name: &str| matches!(stores.get(name), Some(Value::Bool(true)));
        let page = Some(text("colors.page"))
            .filter(|p| PAGES.contains(&p.as_str()))
            .unwrap_or_else(|| PAGES[0].into());
        let vision = Some(text("colors.vision"))
            .filter(|v| VISIONS.iter().any(|(id, _)| id == v))
            .unwrap_or_else(|| "normal".into());
        let open = text("colors.open");
        {
            let mut guard = self.state.borrow_mut();
            let st = &mut *guard;
            if !open.is_empty() && open != st.seen_open && open.starts_with('/') {
                st.seen_open = open.clone();
                self.choose(st, &open, page == "compare");
            }
        }
        self.pump();
        let st = self.state.borrow();
        self.refresh_poster(&st);
        let entries = self.entries(&st, &text("colors.query"));
        let empty = entries.is_empty() && self.explorer();
        let view = self.view(&st, &vision, flag("colors.fine"));
        BTreeMap::from([
            ("colors.page".into(), Value::Text(page)),
            ("colors.vision".into(), Value::Text(vision)),
            ("colors.view".into(), Value::list(vec![view])),
            ("colors.entries".into(), Value::list(entries)),
            ("colors.empty".into(), Value::Bool(empty)),
            (
                "colors.win_w".into(),
                Value::Number(f64::from(self.width.get())),
            ),
        ])
    }

    // ---- actions ----------------------------------------------------------------

    /// One host action `colors.do`, argument `op:payload`. Returns store changes for the window.
    pub fn act(&self, arg: &str, stores: &Stores) -> BTreeMap<String, Value> {
        let (op, payload) = arg.split_once(':').unwrap_or((arg, ""));
        let mut changes = BTreeMap::new();
        let mut guard = self.state.borrow_mut();
        let st = &mut *guard;
        match op {
            "page" if PAGES.contains(&payload) => {
                changes.insert("colors.page".into(), Value::Text(payload.into()));
                self.change(st, &[("compare", (payload == "compare").into())]);
            }
            "vision" => {
                changes.insert("colors.vision".into(), Value::Text(payload.into()));
            }
            "fine" => {
                let on = matches!(stores.get("colors.fine"), Some(Value::Bool(true)));
                changes.insert("colors.fine".into(), Value::Bool(!on));
            }
            "mode" if payload == "dark" || payload == "light" => {
                self.change(st, &[("mode", payload.into()), ("autoMode", false.into())]);
            }
            "style" if payload == TIDE.0 || STYLES.iter().any(|(id, ..)| *id == payload) => {
                self.change(st, &[("personality", payload.into())]);
            }
            "bg" => {
                self.change(
                    st,
                    &[("backgroundFromWallpaper", (payload == "wallpaper").into())],
                );
            }
            "brightness" | "saturation" => {
                let (low, high) = if op == "brightness" {
                    (-0.15, 0.15)
                } else {
                    (0., 1.6)
                };
                if let Some(v) = payload.parse::<f64>().ok().filter(|v| v.is_finite()) {
                    self.change(
                        st,
                        &[(op, json!((v.clamp(low, high) * 1000.).round() / 1000.))],
                    );
                }
            }
            "tint" => {
                let on = st.options["timeTint"] != json!(true);
                self.change(st, &[("timeTint", on.into()), ("tideAutomatic", on.into())]);
            }
            "wsc" => {
                let on = st.options["workspaceColors"] != json!(true);
                self.change(st, &[("workspaceColors", on.into())]);
            }
            "reset" => {
                st.armed = None;
                self.change(
                    st,
                    &[
                        ("overrides", json!({})),
                        ("brightness", json!(0)),
                        ("saturation", json!(1)),
                        ("accent", Json::Null),
                    ],
                );
            }
            "choose" => {
                let path = payload
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| st.catalog.as_ref()?.entries.get(i))
                    .map(|e| e.path.clone());
                if let Some(path) = path {
                    let compare =
                        matches!(stores.get("colors.page"), Some(Value::Text(p)) if p == "compare");
                    self.choose(st, &path, compare);
                }
            }
            "sample" => {
                if let Some(hex) = hex6(payload) {
                    match st.armed.take() {
                        Some(role) => self.set_role(st, &role, Some(&hex)),
                        None => self.change(st, &[("accent", hex.into())]),
                    }
                }
            }
            "arm" => {
                st.armed = ACCENTS
                    .iter()
                    .find(|(role, ..)| *role == payload)
                    .map(|(role, ..)| role.to_string())
                    .filter(|role| st.armed.as_deref() != Some(role.as_str()));
            }
            "lock" if ACCENTS.iter().any(|(role, ..)| *role == payload) => {
                if st.options["overrides"][payload].is_string() {
                    self.set_role(st, payload, None);
                } else if let Some(hex) = st
                    .preview
                    .as_ref()
                    .and_then(|p| p["colours"][payload].as_str())
                    .and_then(hex6)
                {
                    self.set_role(st, payload, Some(&hex));
                }
            }
            "pick" => {
                let mut parts = payload.split(',').filter_map(|v| v.parse::<f64>().ok());
                if let (Some(x), Some(y)) = (parts.next(), parts.next())
                    && let Some((x, y)) = self.painted(x, y)
                {
                    self.change(st, &[("pick", json!({"x": x, "y": y}))]);
                }
            }
            "apply" | "export" => {
                if self.ready_to_act(st)
                    && let Some(id) = st.preview.as_ref().and_then(|p| p["id"].as_str())
                {
                    let (method, what) = if op == "apply" {
                        (APPLY, "apply")
                    } else {
                        (EXPORT, "export")
                    };
                    st.error.clear();
                    st.status.clear();
                    if let Some(ticket) = self.call(method, json!({"id": id})) {
                        st.action = Some((ticket, what));
                    }
                }
            }
            _ => {}
        }
        drop(guard);
        self.pump();
        changes
    }

    /// A click on the photo box (fractions of the box) as fractions of the painted image.
    fn painted(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let (bw, bh) = self.photo_box.get();
        let pictures = self.pictures.borrow();
        let Some((_, Some(image))) = &pictures.poster else {
            return None;
        };
        if bw <= 0. || bh <= 0. {
            return Some((x, y));
        }
        let (box_ratio, image_ratio) = (
            f64::from(bw / bh),
            f64::from(image.width()) / f64::from(image.height()),
        );
        let (pw, ph) = if image_ratio > box_ratio {
            (1., box_ratio / image_ratio)
        } else {
            (image_ratio / box_ratio, 1.)
        };
        let (px, py) = ((x - (1. - pw) / 2.) / pw, (y - (1. - ph) / 2.) / ph);
        ((0. ..=1.).contains(&px) && (0. ..=1.).contains(&py)).then_some((px, py))
    }

    // ---- pictures ---------------------------------------------------------------

    /// Gives the view's image slots their pixels: `@icon:colors-photo` (the poster, fitted whole),
    /// `@icon:colors-card` (the poster, cropped to fill) and `@icon:colors-thumb-N` (a list
    /// thumbnail, cropped). The slot's own size decides the PNG, at twice the logical size.
    pub fn attach_images(&self, scene: &mut bead::render::Scene) {
        let mut guard = self.pictures.borrow_mut();
        let pictures = &mut *guard;
        let state = self.state.borrow();
        for draw in &mut scene.primitives {
            let bead::render::Primitive::Image { path, rect, .. } = draw else {
                continue;
            };
            let Some(slot) = path.to_str().and_then(|s| s.strip_prefix("@icon:colors-")) else {
                continue;
            };
            let (w, h) = ((rect.w * 2.).ceil() as u32, (rect.h * 2.).ceil() as u32);
            if w == 0 || h == 0 || w > 4096 || h > 4096 {
                continue;
            }
            let (identity, contain) = match slot {
                "photo" | "card" => match &pictures.poster {
                    Some((held, Some(_))) => (held.clone(), slot == "photo"),
                    _ => continue,
                },
                other => {
                    let Some(entry) = other
                        .strip_prefix("thumb-")
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| state.catalog.as_ref()?.entries.get(i))
                    else {
                        continue;
                    };
                    (entry.thumbnail.clone(), false)
                }
            };
            if slot == "photo" {
                self.photo_box.set((rect.w, rect.h));
            }
            let key = format!("@colors/{slot}/{w}x{h}/{identity}");
            if !pictures.rendered.contains_key(&key) {
                let source = if slot == "photo" || slot == "card" {
                    pictures
                        .poster
                        .as_ref()
                        .and_then(|(_, image)| image.as_ref())
                } else {
                    pictures.thumbs.get(&identity).and_then(Option::as_ref)
                };
                let Some(png) = source.and_then(|image| render(image, w, h, contain)) else {
                    continue;
                };
                if pictures.rendered.len() >= MAX_ASSETS {
                    pictures.rendered.clear();
                }
                pictures.rendered.insert(key.clone(), png);
            }
            if let Some(png) = pictures.rendered.get(&key) {
                let key = std::path::PathBuf::from(key);
                scene.asset_data.insert(key.clone(), png.clone());
                *path = key;
            }
        }
    }
}

fn demo_key(request: &Json) -> String {
    format!(
        "{}|{}|{}",
        request["image"], request["mode"], request["personality"]
    )
}

/// The image fitted whole inside `w`x`h` on a transparent canvas, or cropped to fill it.
fn render(source: &image::DynamicImage, w: u32, h: u32, contain: bool) -> Option<Arc<[u8]>> {
    use image::imageops::{FilterType, overlay};
    let canvas = if contain {
        let fitted = source.resize(w, h, FilterType::Triangle).to_rgba8();
        let mut canvas = image::RgbaImage::new(w, h);
        overlay(
            &mut canvas,
            &fitted,
            i64::from((w - fitted.width()) / 2),
            i64::from((h - fitted.height()) / 2),
        );
        canvas
    } else {
        source.resize_to_fill(w, h, FilterType::Triangle).to_rgba8()
    };
    player_art::encode(&canvas)
}

/// The Colors window as a framework `WindowSpec`; a deep link carries the image to explore.
pub fn window_spec(
    root: std::path::PathBuf,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    client: Option<Client>,
    trace: bool,
) -> Result<bead::wayland::WindowSpec, Box<dyn std::error::Error>> {
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.motion = None;
    fallback.primitives.clear();
    fallback.offset = (0., 0.);
    fallback.width = 800.;
    fallback.bounds = bead::render::Rect::new(0., 0., 800., 600.);
    let cold = fallback.clone();
    Ok(bead::wayland::WindowSpec {
        command_targets: Vec::new(),
        app_id: "nacre-colors".into(),
        title: tr("bead.colors.title", &[]),
        size: (1100, 760),
        size_for_output: None,
        on_resize: None,
        min_size: (760, 560),
        fallback,
        factory: Arc::new(move |_| {
            let session = bead::ui::Session::new(
                root.clone(),
                "colors/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::new(session)
                .with_trace(trace)
                .with_colors(client.clone())
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
        model: Arc::new(|args| {
            args.first()
                .and_then(Json::as_str)
                .map(|image| BTreeMap::from([("colors.open".into(), Value::Text(image.into()))]))
                .unwrap_or_default()
        }),
    })
}

#[cfg(test)]
#[path = "colors_tests.rs"]
pub(crate) mod tests;
