// SPDX-License-Identifier: GPL-3.0-only
//! Lazy dashboard projections. No service/backend, polling, ranking or actions.
use crate::i18n::{tr, tr_n};
use bead::core::{
    Value,
    source::{self, Callback, Publisher, Snapshot, Source, Subscription},
};
use nacre_protocol::{
    core_services::{AudioKind, CoreSnapshot},
    dashboard_extras::{DashboardExtrasSnapshot as Extra, FocusStatus},
    settings::SettingsStore,
    weather_photo,
};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Today,
    Weather,
    Media,
    System,
    /// Optional read-only extras from published core topics (B0024).
    Notifications,
    Devices,
    Network,
    /// Daemon-hosted extras and customization (B0027).
    Focus,
    Clipboard,
    AiUsage,
    Screenshots,
    Brain,
    Customize,
}
impl Page {
    /// The four core tabs, in Nacre's tab order.
    pub const ALL: [Self; 4] = [Self::Today, Self::Weather, Self::Media, Self::System];
    pub const EXTRAS: [Self; 3] = [Self::Notifications, Self::Devices, Self::Network];
    pub const MORE: [Self; 5] = [
        Self::Focus,
        Self::Clipboard,
        Self::AiUsage,
        Self::Screenshots,
        Self::Brain,
    ];
    /// Optional pages in Nacre's registry order; customization lists exactly these.
    pub const REGISTRY: [Self; 8] = [
        Self::Focus,
        Self::Notifications,
        Self::Clipboard,
        Self::Devices,
        Self::Network,
        Self::AiUsage,
        Self::Screenshots,
        Self::Brain,
    ];
    fn every() -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .chain(Self::EXTRAS)
            .chain(Self::MORE)
            .chain([Self::Customize])
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Weather => "weather",
            Self::Media => "media",
            Self::System => "system",
            Self::Notifications => "notifications",
            Self::Devices => "devices",
            Self::Network => "network",
            Self::Focus => "focus",
            Self::Clipboard => "clipboard",
            Self::AiUsage => "ai-usage",
            Self::Screenshots => "screenshots",
            Self::Brain => "brain",
            Self::Customize => "customize",
        }
    }
    /// Published topic that feeds the page (and its fixture file name).
    pub fn topic(self) -> String {
        match self {
            Self::Notifications => "notifications".into(),
            Self::Devices => "bluetooth".into(),
            Self::Network => "network".into(),
            Self::Focus => "dashboard.focus".into(),
            Self::Clipboard => "clipboard".into(),
            Self::AiUsage => "ai.usage".into(),
            Self::Screenshots => "screenshots".into(),
            Self::Brain => "brain.recent".into(),
            Self::Customize => SETTINGS.into(),
            Self::Today => "dashboard.calendar".into(),
            _ => format!("dashboard.{}", self.name()),
        }
    }
    fn fixture(self) -> &'static str {
        match self {
            Self::Today => "calendar",
            Self::Devices => "bluetooth",
            Self::Customize => "settings",
            other => other.name(),
        }
    }
    pub fn from_view(view: &str) -> Option<Self> {
        let name = view.strip_prefix("dashboard-")?;
        Self::every().find(|page| page.name() == name)
    }
    /// Index in Nacre's dashboard registry (`nacre tab N`). Customization has no
    /// registry slot (it reports 0 and is never mapped from an index).
    pub fn tab(self) -> usize {
        match self {
            Self::Focus => 4,
            Self::Notifications => 5,
            Self::Clipboard => 6,
            Self::Devices => 7,
            Self::Network => 8,
            Self::AiUsage => 9,
            Self::Screenshots => 10,
            Self::Brain => 11,
            Self::Customize => 0,
            core => Self::ALL.iter().position(|page| *page == core).unwrap_or(0),
        }
    }
    pub fn from_tab(index: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain(Self::EXTRAS)
            .chain(Self::MORE)
            .find(|page| page.tab() == index)
    }
    pub fn title(self) -> String {
        tr(
            match self {
                Self::Today => "bead.dashboard.page-today",
                Self::Weather => "bead.dashboard.page-weather",
                Self::Media => "bead.dashboard.page-media",
                Self::System => "bead.dashboard.page-system",
                Self::Notifications => "bead.dashboard.page-notifications",
                Self::Devices => "bead.dashboard.page-devices",
                Self::Network => "bead.dashboard.page-network",
                Self::Focus => "bead.dashboard.page-focus",
                Self::Clipboard => "bead.dashboard.page-clipboard",
                Self::AiUsage => "bead.dashboard.page-ai-usage",
                Self::Screenshots => "bead.dashboard.page-screenshots",
                Self::Brain => "bead.dashboard.page-brain",
                Self::Customize => "bead.dashboard.page-customize",
            },
            &[],
        )
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Today => "dashboard",
            Self::Weather => "partly_cloudy_day",
            Self::Media => "music_note",
            Self::System => "monitoring",
            Self::Notifications => "notifications",
            Self::Devices => "devices",
            Self::Network => "wifi",
            Self::Focus => "timer",
            Self::Clipboard => "content_paste",
            Self::AiUsage => "data_usage",
            Self::Screenshots => "screenshot",
            Self::Brain => "psychology",
            Self::Customize => "tune",
        }
    }
}
const SETTINGS: &str = "settings.store.dashboard-extras";

/// Only transport configuration survives a page's idle retirement.
#[derive(Clone)]
pub struct Choice {
    pub client: Option<crate::nacre_source::Client>,
    pub fixtures: Option<PathBuf>,
    /// B0035 effective Nacre preferences; none in previews and fixture runs.
    pub prefs: Option<Arc<dyn Source>>,
}
impl Choice {
    pub fn open(&self, page: Page) -> source::Result<Arc<dyn Source>> {
        self.open_named(&page.topic(), page.fixture())
    }
    fn open_named(&self, topic: &str, fixture: &str) -> source::Result<Arc<dyn Source>> {
        if let Some(directory) = &self.fixtures {
            // Extras' fixtures live beside the frozen core fixture tree.
            let mut path = directory.join(format!("{fixture}.json"));
            if !path.exists() {
                path = directory
                    .with_file_name("dashboard-extras")
                    .join(format!("{fixture}.json"));
            }
            return Ok(Arc::new(ReadOnly::fixture(&path, topic)?));
        }
        if let Some(client) = &self.client {
            return Ok(Arc::new(client.lease_topic(topic)?));
        }
        Ok(Arc::new(ReadOnly::unavailable()?))
    }
}

pub fn spec(
    page: Page,
    palette: Arc<PathBuf>,
    colors: Arc<bead::render::Palette>,
    choice: Choice,
    owner: Arc<std::sync::OnceLock<Arc<bead::services::Desktop>>>,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    use bead::render::{Primitive, Rect};
    let attachment = crate::placement::Attachment::Attached(bead::core::Edge::Top);
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 984.;
    // Match the renderer's full padded companion geometry before the joined
    // frame art is prepared; that decorator leaves scene bounds and offset intact.
    fallback.bounds = Rect::new(-32., -32., 1048., 714.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 984., 650.),
        radius: 18.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "dashboard".into();
        motion.edge = bead::core::Edge::Top;
        // QML: the panel grows out of the bar over 360 ms, in-out cubic, opaque throughout.
        motion.duration = 0.36;
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    let factory: Factory = {
        let choice = choice.clone();
        let owner = owner.clone();
        Arc::new(move |page| {
            let source = choice.open(page).map_err(|e| e.to_string())?;
            let secondary = match page {
                Page::Today => Some(choice.open_named("dashboard.weather", "weather")),
                // N0056: only the visible Weather page leases the city photo; previews without one go on.
                Page::Weather => choice
                    .open_named(weather_photo::TOPIC, "weather_photo")
                    .ok()
                    .map(Ok),
                Page::System => Some(choice.open_named("dashboard.updates", "updates")),
                Page::Devices => Some(choice.open_named("audio", "audio")),
                _ => None,
            }
            .transpose()
            .map_err(|e| e.to_string())?;
            // Playback is optional: no daemon player topic leaves the lyrics page as is.
            let secondary = if page == Page::Media {
                choice.open_named("dashboard.player", "player").ok()
            } else {
                secondary
            };
            // The Today mini player shares the topic; the lease follows the visible page.
            let player = if page == Page::Today {
                choice.open_named("dashboard.player", "player").ok()
            } else {
                None
            };
            let system = (page == Page::Today)
                .then(|| choice.open_named("dashboard.system", "system").ok())
                .flatten();
            let wallpaper = (page == Page::Today)
                .then(|| choice.open_named("wallpaper.active", "wallpaper").ok())
                .flatten();
            let summary_updates = (page == Page::Today)
                .then(|| choice.open_named("dashboard.updates", "updates").ok())
                .flatten();
            // Tab visibility and the customization page read the revisioned store.
            let settings = choice.open_named(SETTINGS, "settings").ok();
            let controls = choice.client.clone().map(controls::Controls::new);
            let clock = if page == Page::Today {
                let desktop = owner
                    .get()
                    .cloned()
                    .ok_or_else(|| "bar services not ready".to_owned())?;
                Some(
                    Arc::new(bead::source::Clock::attach(desktop).map_err(|e| e.to_string())?)
                        as Arc<dyn Source>,
                )
            } else {
                None
            };
            Ok(Dashboard::new(page, source, secondary, choice.fixtures.is_some())
                .with_clock(clock)
                .with_player(player)
                .with_overview(system, wallpaper, summary_updates)
                .with_controls(settings, controls, choice.client.clone())
                .with_prefs(choice.prefs.clone()))
        })
    };
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(attachment)),
        engages_decorations: true,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let initial = factory(page)?;
            let session = bead::ui::Session::new(
                "shell".into(),
                format!("dashboard/{}/main.luau", page.name()).into(),
                (*palette).clone(),
                (*colors).clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(attachment, &colors)
                .and_then(|runtime| runtime.with_dashboard_factory(initial, factory.clone()))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

struct ReadOnly {
    publisher: Publisher,
}
pub(crate) fn unavailable_audio_source() -> source::Result<Arc<dyn Source>> {
    Ok(Arc::new(ReadOnly::unavailable()?))
}
impl ReadOnly {
    fn unavailable() -> source::Result<Self> {
        let publisher = Publisher::new("dashboard-unavailable".into(), json!({}))?;
        publisher.disconnect()?;
        Ok(Self { publisher })
    }
    fn fixture(path: &Path, topic: &str) -> source::Result<Self> {
        let file = std::fs::File::open(path).map_err(|_| invalid())?;
        let mut bytes = Vec::new();
        file.take(nacre_protocol::MAX_FRAME as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() > nacre_protocol::MAX_FRAME {
            return Err(invalid());
        }
        let data: Json = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        match topic {
            SETTINGS => {
                serde_json::from_value::<SettingsStore>(data.clone()).map_err(|_| invalid())?;
            }
            weather_photo::TOPIC => {
                serde_json::from_value::<weather_photo::State>(data.clone())
                    .map_err(|_| invalid())?;
            }
            "wallpaper.active" => {
                serde_json::from_value::<nacre_protocol::wallpaper::WallpaperActive>(data.clone())
                    .map_err(|_| invalid())?
                    .validate()
                    .map_err(|_| invalid())?;
            }
            _ if extra(topic, &data).is_some() => {}
            _ => {
                checked(topic, &data)?;
            }
        }
        Ok(Self {
            publisher: Publisher::new("dashboard-fixture".into(), data)?,
        })
    }
}
impl Source for ReadOnly {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> source::Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, _: String, _: Json) -> source::Reply<'_> {
        Box::pin(async {
            Err(source::Error::new(
                "unavailable",
                "dashboard operation unavailable",
                false,
            ))
        })
    }
    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
    fn stop(&self) -> source::Reply<'_> {
        Box::pin(async {
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}
fn invalid() -> source::Error {
    source::Error::new("validation", "invalid dashboard snapshot", false)
}
fn checked(topic: &str, data: &Json) -> source::Result<CoreSnapshot> {
    // Decode only the selected published dashboard payload types. Deserializing the whole
    // service enum links unrelated device/notification parsers into this view.
    let snapshot = match topic {
        "dashboard.calendar" => {
            CoreSnapshot::Calendar(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "dashboard.weather" => {
            CoreSnapshot::Weather(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "dashboard.media" => {
            CoreSnapshot::Media(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "dashboard.system" => {
            CoreSnapshot::System(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "dashboard.player" => {
            CoreSnapshot::Player(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "dashboard.updates" => {
            CoreSnapshot::Updates(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "notifications" => CoreSnapshot::Notifications(
            serde_json::from_value(data.clone()).map_err(|_| invalid())?,
        ),
        "bluetooth" => {
            CoreSnapshot::Bluetooth(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "audio" => {
            CoreSnapshot::Audio(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        "network" => {
            CoreSnapshot::Network(serde_json::from_value(data.clone()).map_err(|_| invalid())?)
        }
        _ => return Err(invalid()),
    };
    if !snapshot.valid() {
        return Err(invalid());
    }
    Ok(snapshot)
}

/// Decode one of the five N0034 topics; `None` for any other or invalid payload.
fn extra(topic: &str, data: &Json) -> Option<Extra> {
    if !matches!(
        topic,
        "dashboard.focus" | "clipboard" | "screenshots" | "ai.usage" | "brain.recent"
    ) {
        return None;
    }
    let snapshot: Extra = serde_json::from_value(json!({"topic": topic, "data": data})).ok()?;
    snapshot.validate().ok()?;
    Some(snapshot)
}
/// Stops the one-second countdown wake when dropped.
struct Ticker {
    _stop: std::sync::mpsc::Sender<()>,
}
/// Art path and its thumbnail (`None` when the file was refused).
type Art = (String, Option<Arc<[u8]>>);
/// A published city photo: path, version (`checked`) and its bytes and size (`None` when refused).
type Photo = (String, u64, Option<(Arc<[u8]>, u32, u32)>);
type Poster = (
    nacre_protocol::wallpaper::ActiveWallpaper,
    u32,
    u32,
    Option<Arc<[u8]>>,
);
// The decode never runs on the panel preparation thread. Retirement joins it so
// an old page cannot leave a decoder or an image allocation behind.
struct PosterJob {
    result: std::sync::mpsc::Receiver<Poster>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for PosterJob {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
            // The encoded poster remains live, but the decoder/encoder scratch
            // has gone. Reclaim its arena pages once, after the job's thread cache
            // is gone; the ticking open page may never reach the UI idle trim.
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            {
                // SAFETY: process-local allocator trim after the job has joined.
                unsafe { nix::libc::malloc_trim(0) };
                bead::core::profile::mark("dashboard_poster_trimmed");
            }
        }
    }
}
type Notify = Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>;

#[path = "dashboard_controls.rs"]
mod controls;

pub struct Dashboard {
    page: Page,
    source: Arc<dyn Source>,
    secondary: Option<Arc<dyn Source>>,
    /// Today's mini player (the Media page reads its player through `secondary`).
    player: Option<Arc<dyn Source>>,
    system: Option<Arc<dyn Source>>,
    wallpaper: Option<Arc<dyn Source>>,
    summary_updates: Option<Arc<dyn Source>>,
    raster_scale: std::cell::Cell<f64>,
    poster: std::cell::RefCell<Option<Poster>>,
    poster_job: std::cell::RefCell<Option<PosterJob>>,
    /// Thumbnail of the active player's art, decoded once per path while visible.
    art: std::cell::RefCell<Option<Art>>,
    /// The Weather page's daemon city photo, held only while the page is shown.
    photo: std::cell::RefCell<Option<Photo>>,
    /// Screenshot row thumbnails by capture id, only for the listed rows while shown.
    shots: std::cell::RefCell<BTreeMap<String, Art>>,
    clock: Option<Arc<dyn Source>>,
    settings: Option<Arc<dyn Source>>,
    /// Effective Nacre preferences (B0035): temperature unit and core tab order.
    prefs: Option<Arc<dyn Source>>,
    controls: Option<controls::Controls>,
    client: Option<crate::nacre_source::Client>,
    notify: std::cell::RefCell<Option<Notify>>,
    ticker: std::cell::RefCell<Option<Ticker>>,
    now: fn() -> u64,
    preview: bool,
}
pub type Factory = Arc<dyn Fn(Page) -> Result<Dashboard, String> + Send + Sync>;
impl Dashboard {
    pub fn page(&self) -> Page {
        self.page
    }
    pub fn new(
        page: Page,
        source: Arc<dyn Source>,
        updates: Option<Arc<dyn Source>>,
        preview: bool,
    ) -> Self {
        Self {
            page,
            source,
            secondary: updates,
            player: None,
            system: None,
            wallpaper: None,
            summary_updates: None,
            raster_scale: std::cell::Cell::new(2.),
            poster: Default::default(),
            poster_job: Default::default(),
            art: Default::default(),
            photo: Default::default(),
            shots: Default::default(),
            clock: None,
            settings: None,
            prefs: None,
            controls: None,
            client: None,
            notify: Default::default(),
            ticker: Default::default(),
            now: now_ms,
            preview,
        }
    }
    pub fn with_controls(
        mut self,
        settings: Option<Arc<dyn Source>>,
        controls: Option<controls::Controls>,
        client: Option<crate::nacre_source::Client>,
    ) -> Self {
        self.settings = settings;
        self.controls = controls;
        self.client = client;
        self
    }
    pub fn with_prefs(mut self, prefs: Option<Arc<dyn Source>>) -> Self {
        self.prefs = prefs;
        self
    }
    fn prefs(&self) -> Option<crate::prefs::Prefs> {
        self.prefs
            .as_ref()
            .map(|p| crate::prefs::Prefs::from_source(&**p))
    }
    /// A temperature in the owner's unit; the daemon publishes degrees Celsius.
    fn degrees(&self, value: Option<f64>) -> String {
        let fahrenheit = self.prefs().is_some_and(|p| p.fahrenheit);
        number(
            value.map(|v| (if fahrenheit { v * 9. / 5. + 32. } else { v } + 0.5).floor()),
            if fahrenheit { "°F" } else { "°C" },
        )
    }
    #[cfg(test)]
    fn with_now(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }
    pub fn with_player(mut self, player: Option<Arc<dyn Source>>) -> Self {
        self.player = player;
        self
    }
    pub fn with_overview(
        mut self,
        system: Option<Arc<dyn Source>>,
        wallpaper: Option<Arc<dyn Source>>,
        updates: Option<Arc<dyn Source>>,
    ) -> Self {
        self.system = system;
        self.wallpaper = wallpaper;
        self.summary_updates = updates;
        self
    }
    pub fn with_clock(mut self, clock: Option<Arc<dyn Source>>) -> Self {
        self.clock = clock;
        self
    }
    pub fn set_raster_scale(&self, scale: f32) {
        if scale.is_finite() && scale > 0. {
            self.raster_scale.set(f64::from(scale));
        }
    }
    pub fn project_with_stores(&self, stores: &bead::core::Stores) -> BTreeMap<String, Value> {
        let mut values = self.project();
        let open = stores.get("panel.open") != Some(&Value::Bool(false));
        if self.page == Page::Today && open {
            self.project_wallpaper(stores, &mut values);
        } else if self.page == Page::Today {
            // Preserve the visible poster through the closing reveal. The existing
            // panel retirement drops it and joins any pending job after the grace.
            values.insert(
                "dashboard.wallpaper".into(),
                Value::Bool(
                    self.poster
                        .borrow()
                        .as_ref()
                        .is_some_and(|(_, _, _, bytes)| bytes.is_some()),
                ),
            );
        }
        // The countdown redraws once a second only while a session runs in an open panel;
        // the mini player on Today shows no position and needs no wake.
        let live = matches!(values.get("dashboard.state"), Some(Value::Text(t)) if t == "running" || (t == "playing" && self.page == Page::Media))
            && open;
        self.tick(live);
        // Keep art stable through the closing reveal: clearing it invalidates
        // Media's prepared blur just before the UI joins the retiring worker.
        // Runtime retirement drops the thumbnail after the existing close grace.
        if !open {
            *self.photo.borrow_mut() = None;
            self.shots.borrow_mut().clear();
        }
        if let Some(clock) = &self.clock {
            values.extend(crate::calendar::Calendar::new(clock.clone()).project(stores));
            let snapshot = clock.snapshot();
            let clock = snapshot
                .data
                .get("clock")
                .and_then(Json::as_str)
                .filter(|clock| !clock.is_empty())
                .unwrap_or("—");
            let twelve = self.prefs().is_some_and(|p| p.clock_12h);
            text(&mut values, "clock", &clock_text(clock, twelve), 16);
            if !snapshot.connected {
                values.insert("dashboard.stale".into(), Value::Bool(true));
            }
        }
        values
    }
    pub fn subscribe(
        &self,
        notify: Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>,
    ) -> source::Result<Vec<Subscription>> {
        *self.notify.borrow_mut() = Some(notify.clone());
        let callback: Callback = {
            let notify = notify.clone();
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())))
        };
        let mut subscriptions = vec![self.source.subscribe(callback.clone())?];
        if let Some(settings) = &self.settings {
            subscriptions.push(settings.subscribe(callback.clone())?);
        }
        if let Some(prefs) = &self.prefs {
            subscriptions.push(prefs.subscribe(callback.clone())?);
        }
        if let Some(client) = &self.client {
            // Method advertisement arrives with the link, not with a topic.
            subscriptions.push(client.subscribe(callback.clone())?);
        }
        if let Some(secondary) = &self.secondary {
            subscriptions.push(secondary.subscribe(callback.clone())?);
        }
        if let Some(player) = &self.player {
            subscriptions.push(player.subscribe(callback.clone())?);
        }
        for source in [&self.system, &self.wallpaper, &self.summary_updates]
            .into_iter()
            .flatten()
        {
            subscriptions.push(source.subscribe(callback.clone())?);
        }
        if let Some(clock) = &self.clock {
            subscriptions.push(clock.subscribe(callback)?);
        }
        Ok(subscriptions)
    }
    pub fn project(&self) -> BTreeMap<String, Value> {
        let mut out = self.project_page();
        self.chrome(&mut out);
        out
    }
    fn project_page(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let mut out = BTreeMap::from([
            (
                "dashboard.page".into(),
                Value::Text(self.page.name().into()),
            ),
            ("dashboard.title".into(), Value::Text(self.page.title())),
            ("dashboard.preview".into(), Value::Bool(self.preview)),
            ("dashboard.ready".into(), Value::Bool(false)),
            ("dashboard.stale".into(), Value::Bool(false)),
            (
                "dashboard.heading".into(),
                Value::Text(tr(
                    "bead.dashboard.page-unavailable",
                    &[("page", &self.page.title())],
                )),
            ),
            (
                "dashboard.summary".into(),
                Value::Text(tr("bead.dashboard.data-pending", &[])),
            ),
            ("dashboard.detail".into(), Value::Text(String::new())),
            ("dashboard.metric".into(), Value::Text("—".into())),
            ("dashboard.rows".into(), Value::list(Vec::new())),
            ("dashboard.secondary".into(), Value::list(Vec::new())),
            (
                "dashboard.updates".into(),
                Value::Text(tr("bead.dashboard.updates-unavailable", &[])),
            ),
            (
                "dashboard.updates_detail".into(),
                Value::Text(String::new()),
            ),
        ]);
        if let Some(updates) = if self.page == Page::System {
            self.secondary.as_ref()
        } else {
            self.summary_updates.as_ref()
        } {
            let update = updates.snapshot();
            if let Ok(CoreSnapshot::Updates(s)) = checked("dashboard.updates", &update.data) {
                if s.health.available
                    && let Some(count) = s.count
                {
                    text(
                        &mut out,
                        "updates",
                        &if count == 0 {
                            tr("bead.dashboard.updates-current", &[])
                        } else {
                            tr_n("bead.dashboard.updates-count", u64::from(count), &[])
                        },
                        48,
                    );
                }
                text(
                    &mut out,
                    "updates_detail",
                    &if s.health.stale || !update.connected {
                        tr("bead.dashboard.state-stale", &[])
                    } else {
                        s.text.clone()
                    },
                    80,
                );
            }
        }
        if matches!(self.page, Page::Media | Page::Today) {
            self.project_player(&mut out);
        }
        if self.page == Page::Today {
            out.insert("dashboard.wallpaper".into(), Value::Bool(false));
            out.insert("dashboard.host".into(), Value::list(Vec::new()));
            if let Some(source) = &self.system
                && let Ok(CoreSnapshot::System(s)) =
                    checked("dashboard.system", &source.snapshot().data)
                && s.health.available
            {
                controls::host(&s, self.now, &mut out);
                let mut gauges = controls::gauges(&s);
                let disk = s.disks.order.iter().find_map(|id| s.disks.items.get(id));
                gauges[2] = json!({"id":"storage","label":tr("bead.dashboard.gauge-storage", &[]),"icon":"storage",
                    "used":disk.map(|d| d.used),"total":disk.map(|d| d.total),
                        "value":disk.map_or("—".into(), |d| format!("{:.0}%", d.used as f64 * 100. / d.total as f64)),
                    "fraction":disk.map(|d| (d.used as f64 / d.total as f64).clamp(0.,1.)),
                    "detail":disk.map_or(String::new(), |d| format!("{} / {}", bytes(d.used), bytes(d.total)))});
                out.insert("dashboard.gauges".into(), Value::list(gauges));
            }
            text(&mut out, "clock", "—", 16);
            if let Some(source) = &self.secondary {
                let weather = source.snapshot();
                if let Ok(CoreSnapshot::Weather(s)) = checked("dashboard.weather", &weather.data) {
                    if s.health.available
                        && let Some(c) = s.current
                    {
                        text(
                            &mut out,
                            "summary",
                            &format!(
                                "{}  {} · {}",
                                self.degrees(c.temperature),
                                c.description,
                                c.location
                            ),
                            96,
                        );
                    } else {
                        text(
                            &mut out,
                            "summary",
                            &tr("bead.dashboard.weather-unavailable", &[]),
                            96,
                        );
                    }
                    out.insert(
                        "dashboard.stale".into(),
                        Value::Bool(s.health.stale || !weather.connected),
                    );
                } else {
                    text(
                        &mut out,
                        "summary",
                        &tr("bead.dashboard.weather-unavailable", &[]),
                        96,
                    );
                }
            } else {
                text(
                    &mut out,
                    "summary",
                    &tr("bead.dashboard.weather-unavailable", &[]),
                    96,
                );
            }
        }
        if Page::MORE.contains(&self.page) || self.page == Page::Customize {
            self.project_more(&mut out, &snapshot);
            return out;
        }
        let Ok(data) = checked(&self.page.topic(), &snapshot.data) else {
            return out;
        };
        let health = match &data {
            CoreSnapshot::Calendar(s) => &s.health,
            CoreSnapshot::Weather(s) => &s.health,
            CoreSnapshot::Media(s) => &s.health,
            CoreSnapshot::System(s) => &s.health,
            CoreSnapshot::Notifications(s) => &s.health,
            CoreSnapshot::Bluetooth(s) => &s.health,
            CoreSnapshot::Network(s) => &s.health,
            _ => return out,
        };
        out.insert("dashboard.ready".into(), Value::Bool(health.available));
        out.insert(
            "dashboard.stale".into(),
            Value::Bool(
                health.stale
                    || !snapshot.connected
                    || out.get("dashboard.stale") == Some(&Value::Bool(true)),
            ),
        );
        if !health.available {
            return out;
        }
        match data {
            CoreSnapshot::Calendar(s) => {
                text(
                    &mut out,
                    "heading",
                    &tr("bead.dashboard.calendar-heading", &[]),
                    48,
                );
                text(
                    &mut out,
                    "detail",
                    &if s.configured {
                        if s.events.order.is_empty() {
                            tr("bead.dashboard.calendar-none", &[])
                        } else {
                            String::new()
                        }
                    } else {
                        tr("bead.dashboard.calendar-unconfigured", &[])
                    },
                    80,
                );
                let rows = s.events.order.iter().filter_map(|id| s.events.items.get(id).map(|e| json!({
                    "id":id,"label":line(&e.title,64),"value":line(&e.time,24),"detail":line(&e.date,24)
                }))).collect();
                out.insert("dashboard.rows".into(), Value::list(rows));
            }
            CoreSnapshot::Weather(s) => {
                let Some(current) = s.current else {
                    out.insert("dashboard.ready".into(), Value::Bool(false));
                    return out;
                };
                out.insert(
                    "dashboard.weather.code".into(),
                    Value::Number(current.wmo.map_or(-1., f64::from)),
                );
                out.insert("dashboard.weather.day".into(), Value::Bool(current.is_day));
                let photos_enabled = self
                    .settings
                    .as_ref()
                    .and_then(|source| {
                        serde_json::from_value::<SettingsStore>((*source.snapshot().data).clone())
                            .ok()
                    })
                    .is_none_or(|store| {
                        store.data.get("weatherPhotos") != Some(&Json::Bool(false))
                    });
                out.insert(
                    "dashboard.weather.photos_enabled".into(),
                    Value::Bool(photos_enabled),
                );
                self.project_photo(&mut out, photos_enabled);
                text(&mut out, "weather.city", current.location.trim(), 100);
                text(&mut out, "heading", &current.location, 24);
                text(&mut out, "summary", &current.description, 48);
                text(&mut out, "metric", &self.degrees(current.temperature), 24);
                text(
                    &mut out,
                    "detail",
                    &tr(
                        "bead.dashboard.weather-details",
                        &[("feels", &self.degrees(current.feels_like))],
                    ),
                    96,
                );
                let day = current
                    .days
                    .order
                    .first()
                    .and_then(|id| current.days.items.get(id));
                let hour = current
                    .hours
                    .order
                    .first()
                    .and_then(|id| current.hours.items.get(id));
                let metrics = [
                    ("wind", "bead.dashboard.weather-wind", current.wind, " km/h"),
                    (
                        "humidity",
                        "bead.dashboard.weather-humidity",
                        current.humidity,
                        "%",
                    ),
                    (
                        "uv",
                        "bead.dashboard.weather-uv",
                        day.and_then(|d| d.uv),
                        "",
                    ),
                    (
                        "rain",
                        "bead.dashboard.weather-rain",
                        hour.and_then(|h| h.rain),
                        "%",
                    ),
                    (
                        "air-quality",
                        "bead.dashboard.weather-air-quality",
                        current.aqi,
                        " AQI",
                    ),
                    (
                        "pressure",
                        "bead.dashboard.weather-pressure",
                        current.pressure,
                        " hPa",
                    ),
                ]
                .into_iter()
                .map(|(id, label, value, suffix)| {
                    json!({"id":id,
                    "label":tr(label, &[]),
                    "value":number(value, suffix)})
                })
                .collect();
                out.insert("dashboard.weather.metrics".into(), Value::list(metrics));
                let time = |value: Option<&str>| {
                    value
                        .and_then(|v| v.rsplit('T').next())
                        .and_then(|v| v.get(..5))
                        .unwrap_or("—")
                        .to_owned()
                };
                let rise = time(day.and_then(|d| d.sunrise.as_deref()));
                let set = time(day.and_then(|d| d.sunset.as_deref()));
                text(
                    &mut out,
                    "weather.sun_times",
                    &tr(
                        "bead.dashboard.weather-sun-times",
                        &[("rise", &rise), ("set", &set)],
                    ),
                    80,
                );
                let rain = tr("bead.dashboard.weather-rain-unit", &[]);
                let hours = current.hours.order.iter().take(24).filter_map(|id| current.hours.items.get(id).map(|h| json!({
                    "id":id,"label":line(h.time.get(11..16).unwrap_or(&h.time),20),"temperature":h.temperature,"code":h.wmo,
                    "rain_text":number(h.rain,"%"),"value":self.degrees(h.temperature),"detail":number(h.rain,&rain)
                }))).collect();
                let days = current.days.order.iter().take(7).filter_map(|id| current.days.items.get(id).map(|d| json!({
                    "id":id,"label":line(&d.date,20),"low":d.low,"high":d.high,"code":d.wmo,
                    "low_text":self.degrees(d.low),"high_text":self.degrees(d.high),"rain_text":number(d.rain,"%"),"value":format!("{} / {}",self.degrees(d.low),self.degrees(d.high)),"detail":number(d.rain,&rain)
                }))).collect();
                out.insert("dashboard.rows".into(), Value::list(hours));
                out.insert("dashboard.secondary".into(), Value::list(days));
            }
            CoreSnapshot::Media(s) => {
                // This topic is lyrics/palette enrichment. It does not claim a
                // playback identity, progress clock or transport capability.
                text(
                    &mut out,
                    "heading",
                    &tr("bead.dashboard.media-lyrics", &[]),
                    48,
                );
                text(&mut out, "summary", &s.lyrics_message, 120);
                text(&mut out, "detail", &s.plain, 320);
                let rows = s
                    .lines
                    .order
                    .iter()
                    .take(5)
                    .filter_map(|id| {
                        s.lines.items.get(id).map(|l| {
                            json!({
                                "id":id,"label":line(&l.text,40),"value":"","detail":""
                            })
                        })
                    })
                    .collect();
                out.insert("dashboard.rows".into(), Value::list(rows));
            }
            CoreSnapshot::System(s) => {
                text(
                    &mut out,
                    "heading",
                    &tr("bead.dashboard.system-processes", &[]),
                    48,
                );
                text(
                    &mut out,
                    "summary",
                    &tr("bead.dashboard.gauge-memory", &[]),
                    48,
                );
                text(
                    &mut out,
                    "detail",
                    &tr("bead.dashboard.system-cpu-note", &[]),
                    80,
                );
                let mut processes = s
                    .processes
                    .order
                    .iter()
                    .filter_map(|id| s.processes.items.get(id).map(|p| (id, p)))
                    .collect::<Vec<_>>();
                processes.sort_by(|(aid, a), (bid, b)| {
                    b.memory.cmp(&a.memory).then_with(|| aid.cmp(bid))
                });
                let rows = processes.into_iter().take(5).map(|(id,p)| json!({
                    "id":id,"label":line(&p.name,20),"value":bytes(p.memory),"detail":tr("bead.dashboard.system-process-cpu", &[("seconds", &format!("{:.1}", p.cpu_seconds))])
                })).collect();
                let disks = s.disks.order.iter().take(4).filter_map(|id| s.disks.items.get(id).map(|d| json!({
                    "id":id,"label":line(&d.path,10),"value":format!("{} / {}",bytes(d.used),bytes(d.total)),"detail":tr("bead.dashboard.system-drive-detail", &[])
                }))).collect();
                out.insert("dashboard.rows".into(), Value::list(rows));
                out.insert("dashboard.secondary".into(), Value::list(disks));
                out.insert("dashboard.gauges".into(), Value::list({
                    let mut gauges = controls::gauges(&s);
                    let disk = s.disks.order.iter().find_map(|id| s.disks.items.get(id));
                    gauges.push(json!({"id":"storage","label":tr("bead.dashboard.gauge-storage", &[]),"icon":"hard_drive",
                        "used":disk.map(|d| d.used),"total":disk.map(|d| d.total),
                        "value":disk.map_or("—".into(), |d| format!("{:.0}%", d.used as f64 * 100. / d.total as f64)),
                        "fraction":disk.map(|d| (d.used as f64 / d.total as f64).clamp(0.,1.)),
                        "detail":disk.map_or(String::new(), |d| format!("{} / {}", bytes(d.used), bytes(d.total)))}));
                    gauges
                }));
                controls::traffic(&s, &mut out);
                controls::host(&s, self.now, &mut out);
            }
            CoreSnapshot::Notifications(s) => {
                text(
                    &mut out,
                    "heading",
                    &tr("bead.dashboard.notifications-heading", &[]),
                    48,
                );
                text(
                    &mut out,
                    "summary",
                    &tr(
                        if s.do_not_disturb {
                            "bead.dashboard.dnd-summary-on"
                        } else {
                            "bead.dashboard.dnd-summary-off"
                        },
                        &[],
                    ),
                    48,
                );
                out.insert("dashboard.dnd".into(), Value::Bool(s.do_not_disturb));
                // Active first, then the service-ordered history window. A row's
                // `act` carries the displayed revision; the daemon refuses a stale one.
                let rows: Vec<Json> = s
                    .active
                    .order
                    .iter()
                    .filter_map(|id| s.active.items.get(id).map(|n| (id, n, "dismiss")))
                    .chain(s.history.order.iter().filter_map(|id| {
                        s.history.items.get(id).map(|n| (id, n, "forget"))
                    }))
                    .take(12)
                    .map(|(id, n, op)| {
                        let verb = tr(
                            if op == "dismiss" {
                                "bead.dashboard.notification-dismiss"
                            } else {
                                "bead.dashboard.notification-forget"
                            },
                            &[],
                        );
                        json!({"id":id,"label":line(&format!("{}: {}", n.app_name, n.summary), 52),
                            "value":verb,"act":format!("notif.{op}:{}|{}", id.0, n.revision.unwrap_or(0))})
                    })
                    .collect();
                text(
                    &mut out,
                    "detail",
                    &if rows.is_empty() {
                        tr("bead.dashboard.notifications-none", &[])
                    } else if s.history_more == Some(true) {
                        tr(
                            "bead.dashboard.notifications-shown",
                            &[
                                ("shown", &s.history.items.len().to_string()),
                                ("total", &s.history_count.unwrap_or(0).to_string()),
                            ],
                        )
                    } else if s
                        .history_storage
                        .as_ref()
                        .is_some_and(|h| h.error.is_some())
                    {
                        tr("bead.dashboard.notifications-storage-problem", &[])
                    } else {
                        String::new()
                    },
                    48,
                );
                out.insert("dashboard.rows".into(), Value::list(rows));
            }
            CoreSnapshot::Bluetooth(s) => {
                let powered = s.adapters.items.values().any(|a| a.powered);
                text(&mut out, "heading", &tr("bead.panel.bluetooth", &[]), 48);
                text(
                    &mut out,
                    "summary",
                    &tr(
                        match (s.adapters.items.is_empty(), powered) {
                            (true, _) => "bead.dashboard.bluetooth-no-adapter",
                            (_, true) => "bead.dashboard.bluetooth-on",
                            _ => "bead.dashboard.bluetooth-off",
                        },
                        &[],
                    ),
                    48,
                );
                let mut devices = s
                    .devices
                    .order
                    .iter()
                    .filter_map(|id| s.devices.items.get(id).map(|d| (id, d)))
                    .filter(|(_, d)| d.paired || d.connected)
                    .collect::<Vec<_>>();
                devices.sort_by_key(|(_, d)| !d.connected); // stable: keeps service order
                let rows = devices.into_iter().take(6).map(|(id,d)| json!({
                    "id":id,"label":line(&d.name,28),
                    "value":match d.battery {Some(b) => tr("bead.dashboard.bluetooth-battery", &[("percent", &b.to_string())]), None if d.connected => tr("bead.common.connected", &[]), None => tr("bead.dashboard.bluetooth-saved", &[])},
                    "detail":""
                })).collect();
                out.insert("dashboard.rows".into(), Value::list(rows));
                if let Some(audio) = &self.secondary {
                    let audio = audio.snapshot();
                    if let Ok(CoreSnapshot::Audio(a)) = checked("audio", &audio.data)
                        && a.health.available
                    {
                        let nodes = a
                            .nodes
                            .order
                            .iter()
                            .filter_map(|id| a.nodes.items.get(id).map(|n| (id, n)))
                            .filter(|(_, n)| {
                                n.ready && matches!(n.kind, AudioKind::Output | AudioKind::Input)
                            })
                            .take(6)
                            .map(|(id, n)| {
                                let default = a.default_output.as_ref() == Some(id)
                                    || a.default_input.as_ref() == Some(id);
                                json!({"id":id,"label":line(&n.name,28),
                                "value":tr(if n.kind == AudioKind::Output {"bead.dashboard.audio-output"} else {"bead.dashboard.audio-input"}, &[]),
                                "detail":if default {tr("bead.dashboard.audio-default", &[])} else {String::new()}})
                            })
                            .collect();
                        out.insert("dashboard.secondary".into(), Value::list(nodes));
                        if a.health.stale || !audio.connected {
                            out.insert("dashboard.stale".into(), Value::Bool(true));
                        }
                    }
                }
            }
            CoreSnapshot::Network(s) => {
                let primary = s
                    .primary_connection
                    .as_ref()
                    .and_then(|id| s.connections.items.get(id));
                text(
                    &mut out,
                    "heading",
                    &primary
                        .map_or_else(|| tr("bead.common.not-connected", &[]), |c| c.name.clone()),
                    32,
                );
                text(
                    &mut out,
                    "summary",
                    &tr(
                        if s.wireless_enabled {
                            "bead.dashboard.network-wifi-on"
                        } else {
                            "bead.dashboard.network-wifi-off"
                        },
                        &[],
                    ),
                    48,
                );
                let active = s.access_points.items.values().find(|ap| ap.active);
                let signal = active.map_or("—".into(), |ap| format!("{}%", ap.strength));
                text(&mut out, "metric", &signal, 8);
                // Unknown frequency stays blank, never zero.
                text(
                    &mut out,
                    "detail",
                    &active.and_then(frequency).unwrap_or_default(),
                    64,
                );
                let connections = s.connections.order.iter().take(6).filter_map(|id| s.connections.items.get(id).map(|c| json!({
                    "id":id,"label":line(&c.name,28),"value":if c.active {tr("bead.dashboard.network-active", &[])} else {String::new()},"detail":line(&c.kind,20)
                }))).collect();
                let networks = s
                    .access_points
                    .order
                    .iter()
                    .take(6)
                    .filter_map(|id| {
                        s.access_points.items.get(id).map(|ap| json!({
                    "id":id,"label":line(&ap.name,28),"value":format!("{}%",ap.strength),
                    "detail":access_detail(ap)
                }))
                    })
                    .collect();
                out.insert("dashboard.rows".into(), Value::list(connections));
                out.insert("dashboard.secondary".into(), Value::list(networks));
            }
            _ => {}
        }
        out
    }
}
/// "Secured" or "Open", then the radio frequency when the daemon reports one.
fn access_detail(ap: &nacre_protocol::core_services::AccessPoint) -> String {
    let mut parts = vec![tr(
        if ap.secured {
            "bead.dashboard.network-secured"
        } else {
            "bead.dashboard.network-open"
        },
        &[],
    )];
    parts.extend(frequency(ap));
    parts.join(" · ")
}
fn frequency(ap: &nacre_protocol::core_services::AccessPoint) -> Option<String> {
    ap.frequency_mhz.map(|f| format!("{f} MHz"))
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
impl Dashboard {
    /// Today's bounded still poster, retained only while its page is alive.
    fn project_wallpaper(&self, stores: &bead::core::Stores, out: &mut BTreeMap<String, Value>) {
        let pair = self.wallpaper.as_ref().and_then(|source| {
            let snapshot = source.snapshot();
            let state: nacre_protocol::wallpaper::WallpaperActive =
                serde_json::from_value((*snapshot.data).clone()).ok()?;
            (snapshot.connected && state.available && !state.stale && state.validate().is_ok())
                .then_some(state.current)
                .flatten()
        });
        // Geometry belongs to the shell's theme. Decode with the existing short-lived,
        // descriptor-pinned worker; never retain a source-sized bitmap in the UI.
        let dimension = |key| match stores.get(key) {
            Some(Value::Number(n)) if n.is_finite() && *n > 0. => {
                (*n * self.raster_scale.get()).ceil().min(4096.) as u32
            }
            _ => 0,
        };
        let (w, h) = (
            dimension("dashboard.hero.width"),
            dimension("dashboard.hero.height"),
        );
        let mut held = self.poster.borrow_mut();
        let mut pending = self.poster_job.borrow_mut();
        if let Some(job) = pending.as_ref() {
            match job.result.try_recv() {
                Ok(poster) => {
                    *held = Some(poster);
                    pending.take();
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    pending.take();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(pair) = pair.filter(|_| w > 0 && h > 0) {
            if held
                .as_ref()
                .is_none_or(|(old, ow, oh, _)| old != &pair || *ow != w || *oh != h)
            {
                *held = None;
                // At most one job per page, even when the published pair or size
                // changes during decode. Stale results are discarded above.
                if pending.is_none() {
                    let (send, result) = std::sync::mpsc::channel();
                    let notify = self.notify.borrow().clone();
                    let thread = std::thread::Builder::new()
                        .name("dashboard-poster".into())
                        .spawn(move || {
                            bead::core::profile::mark("dashboard_poster_start");
                            let mut request = pair.clone();
                            request.presentation = nacre_protocol::wallpaper::Presentation::Crop;
                            let decoded = crate::wallpaper::decode_pair(request, w, h).ok();
                            bead::core::profile::mark("dashboard_poster_decoded");
                            let bytes = decoded.and_then(|decoded| image::RgbaImage::from_raw(w, h, decoded.rgba))
                                .and_then(|image| crate::player_art::encode(&image));
                            bead::core::profile::mark("dashboard_poster_ready");
                            if bead::core::profile::enabled() {
                                println!("DASHBOARD_POSTER_BYTES width={w} height={h} requested_rgba_bytes={} encoded_bytes={}",
                                    w as usize*h as usize*4, bytes.as_ref().map_or(0, |bytes| bytes.len()));
                            }
                            if send.send((pair, w, h, bytes)).is_ok()
                                && let Some(notify) = notify
                            {
                                notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
                            }
                        });
                    if let Ok(thread) = thread {
                        *pending = Some(PosterJob {
                            result,
                            thread: Some(thread),
                        });
                    }
                }
            }
        } else {
            // An unavailable source must not join an unfinished decode during
            // projection. Poll it on later updates; retirement still joins it.
            *held = None;
        }
        out.insert(
            "dashboard.wallpaper".into(),
            Value::Bool(
                held.as_ref()
                    .is_some_and(|(_, _, _, bytes)| bytes.is_some()),
            ),
        );
    }

    /// N0056: the leased city photo, shown only with consent and a ready daemon asset.
    fn project_photo(&self, out: &mut BTreeMap<String, Value>, enabled: bool) {
        let asset = enabled
            .then_some(self.secondary.as_ref())
            .flatten()
            .and_then(|source| {
                let snapshot = source.snapshot();
                let state =
                    serde_json::from_value::<weather_photo::State>((*snapshot.data).clone())
                        .ok()?;
                (snapshot.connected
                    && state.enabled
                    && state.ready
                    && state.status == weather_photo::Status::Ready)
                    .then_some(state.asset)
                    .flatten()
            });
        let mut held = self.photo.borrow_mut();
        let shown = asset.and_then(|asset| {
            let version = asset.checked as u64;
            if held
                .as_ref()
                .is_none_or(|(path, checked, _)| *path != asset.path || *checked != version)
            {
                let loaded = crate::player_art::photo(&asset.path);
                *held = Some((asset.path.clone(), version, loaded));
            }
            let (_, _, loaded) = held.as_ref()?;
            loaded.as_ref().map(|(_, w, h)| (asset, *w, *h))
        });
        if shown.is_none() {
            *held = None;
        }
        let (w, h) = shown.as_ref().map_or((0, 0), |(_, w, h)| (*w, *h));
        out.insert(
            "dashboard.weather.photo".into(),
            Value::Bool(shown.is_some()),
        );
        out.insert(
            "dashboard.weather.photo_w".into(),
            Value::Number(f64::from(w)),
        );
        out.insert(
            "dashboard.weather.photo_h".into(),
            Value::Number(f64::from(h)),
        );
        let (author, license, link) = shown.as_ref().map_or(("", "", false), |(asset, _, _)| {
            (
                &asset.author,
                &asset.license,
                credit_url(&asset.source).is_some(),
            )
        });
        text(out, "weather.photo_author", author, 160);
        text(out, "weather.photo_license", license, 40);
        out.insert("dashboard.weather.photo_link".into(), Value::Bool(link));
    }
    /// The Commons file page the credit opens, if the daemon's source is one.
    fn credit_url(&self) -> Option<String> {
        let source = self.secondary.as_ref()?.snapshot();
        let state = serde_json::from_value::<weather_photo::State>((*source.data).clone()).ok()?;
        credit_url(&state.asset?.source).map(str::to_owned)
    }
}
/// `https://commons.wikimedia.org/wiki/File:...` without query, fragment, controls or spaces.
fn credit_url(url: &str) -> Option<&str> {
    let name = url.strip_prefix("https://commons.wikimedia.org/wiki/File:")?;
    (!name.is_empty()
        && url.len() <= 2048
        && !name.contains(['?', '#', '\\'])
        && !url.chars().any(|c| c.is_control() || c.is_whitespace()))
    .then_some(url)
}
fn text(out: &mut BTreeMap<String, Value>, key: &str, value: &str, limit: usize) {
    out.insert(format!("dashboard.{key}"), Value::Text(line(value, limit)));
}
fn line(value: &str, limit: usize) -> String {
    let mut chars = value.chars().map(|c| if c.is_control() { ' ' } else { c });
    let mut out: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
fn number(value: Option<f64>, unit: &str) -> String {
    value
        .filter(|v| v.is_finite())
        .map(|v| {
            if v.abs() >= 10000. {
                format!("{v:.0e}{unit}")
            } else {
                format!("{v:.0}{unit}")
            }
        })
        .unwrap_or_else(|| "—".into())
}
fn bytes(value: u64) -> String {
    if value >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", value as f64 / (1024. * 1024. * 1024.))
    } else {
        format!("{:.1} MiB", value as f64 / (1024. * 1024.))
    }
}

#[cfg(test)]
#[path = "dashboard_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dashboard_more_tests.rs"]
mod more_tests;

#[cfg(test)]
#[path = "dashboard_overview_tests.rs"]
mod overview_tests;

/// `desktop.clockFormat`: the daemon clock is "HH:MM"; 12-hour reads "h:MM AM/PM".
fn clock_text(clock: &str, twelve: bool) -> String {
    match clock.split_once(':').map(|(h, m)| (h.parse::<u32>(), m)) {
        Some((Ok(h), m)) if twelve && h < 24 && m.len() == 2 => {
            format!(
                "{}:{m} {}",
                (h + 11) % 12 + 1,
                if h < 12 { "AM" } else { "PM" }
            )
        }
        _ => clock.to_owned(),
    }
}
