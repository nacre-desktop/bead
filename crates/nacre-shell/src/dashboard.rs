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
    // Match the joined top frame's two-pixel upper inset, avoiding a second
    // full panel SHM geometry when the prepared scene becomes available.
    fallback.bounds = Rect::new(-32., -2., 1048., 684.);
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
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(attachment)),
        engages_decorations: true,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let source = choice.open(page).map_err(|e| e.to_string())?;
            let secondary = match page {
                Page::Today => Some(choice.open_named("dashboard.weather", "weather")),
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
            // Tab visibility and the customization page read the revisioned store.
            let settings = choice.open_named(SETTINGS, "settings").ok();
            let controls = choice.client.clone().map(controls::Controls::new);
            let clock = if page == Page::Today {
                let desktop = owner.get().cloned().ok_or("bar services not ready")?;
                Some(
                    Arc::new(bead::source::Clock::attach(desktop).map_err(|e| e.to_string())?)
                        as Arc<dyn Source>,
                )
            } else {
                None
            };
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
                .and_then(|runtime| {
                    runtime.with_dashboard_view(
                        Dashboard::new(page, source, secondary, choice.fixtures.is_some())
                            .with_clock(clock)
                            .with_player(player)
                            .with_controls(settings, controls, choice.client.clone())
                            .with_prefs(choice.prefs.clone()),
                    )
                })
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
type Notify = Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>;

#[path = "dashboard_controls.rs"]
mod controls;

pub struct Dashboard {
    page: Page,
    source: Arc<dyn Source>,
    secondary: Option<Arc<dyn Source>>,
    /// Today's mini player (the Media page reads its player through `secondary`).
    player: Option<Arc<dyn Source>>,
    /// Thumbnail of the active player's art, decoded once per path while visible.
    art: std::cell::RefCell<Option<Art>>,
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
impl Dashboard {
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
            art: Default::default(),
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
            value.map(|v| if fahrenheit { v * 9. / 5. + 32. } else { v }),
            "°",
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
    pub fn with_clock(mut self, clock: Option<Arc<dyn Source>>) -> Self {
        self.clock = clock;
        self
    }
    pub fn project_with_stores(&self, stores: &bead::core::Stores) -> BTreeMap<String, Value> {
        let mut values = self.project();
        // The countdown redraws once a second only while a session runs in an open panel;
        // the mini player on Today shows no position and needs no wake.
        let open = stores.get("panel.open") != Some(&Value::Bool(false));
        let live = matches!(values.get("dashboard.state"), Some(Value::Text(t)) if t == "running" || (t == "playing" && self.page == Page::Media))
            && open;
        self.tick(live);
        if !open {
            // Hidden: the art thumbnail is released with the card.
            *self.art.borrow_mut() = None;
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
        if self.page == Page::System
            && let Some(updates) = &self.secondary
        {
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
                text(&mut out, "heading", &current.location, 24);
                text(&mut out, "summary", &current.description, 48);
                text(&mut out, "metric", &self.degrees(current.temperature), 24);
                text(
                    &mut out,
                    "detail",
                    &tr(
                        "bead.dashboard.weather-details",
                        &[
                            ("feels", &self.degrees(current.feels_like)),
                            ("wind", &number(current.wind, " km/h")),
                        ],
                    ),
                    96,
                );
                let rain = tr("bead.dashboard.weather-rain-unit", &[]);
                let hours = current.hours.order.iter().take(8).filter_map(|id| current.hours.items.get(id).map(|h| json!({
                    "id":id,"label":line(h.time.get(11..16).unwrap_or(&h.time),20),"value":self.degrees(h.temperature),"detail":number(h.rain,&rain)
                }))).collect();
                let days = current.days.order.iter().take(7).filter_map(|id| current.days.items.get(id).map(|d| json!({
                    "id":id,"label":line(&d.date,20),"value":format!("{} / {}",self.degrees(d.low),self.degrees(d.high)),"detail":number(d.rain,&rain)
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
                out.insert("dashboard.gauges".into(), Value::list(controls::gauges(&s)));
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

/// `desktop.clockFormat`: the daemon clock is "HH:MM"; 12-hour reads "h:MM AM/PM".
fn clock_text(clock: &str, twelve: bool) -> String {
    match clock.split_once(':').map(|(h, m)| (h.parse::<u32>(), m)) {
        Some((Ok(h), m)) if twelve && h < 24 && m.len() == 2 => {
            format!("{}:{m} {}", (h + 11) % 12 + 1, if h < 12 { "AM" } else { "PM" })
        }
        _ => clock.to_owned(),
    }
}
