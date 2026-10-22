// SPDX-License-Identifier: GPL-3.0-only
//! Lazy dashboard projections. No service/backend, polling, ranking or actions.
use bead::core::{
    Value,
    source::{self, Callback, Publisher, Snapshot, Source, Subscription},
};
use nacre_protocol::core_services::{AudioKind, CoreSnapshot};
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
}
impl Page {
    /// The four core tabs, in Nacre's tab order.
    pub const ALL: [Self; 4] = [Self::Today, Self::Weather, Self::Media, Self::System];
    pub const EXTRAS: [Self; 3] = [Self::Notifications, Self::Devices, Self::Network];
    pub fn name(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Weather => "weather",
            Self::Media => "media",
            Self::System => "system",
            Self::Notifications => "notifications",
            Self::Devices => "devices",
            Self::Network => "network",
        }
    }
    /// Published topic that feeds the page (and its fixture file name).
    pub fn topic(self) -> String {
        match self {
            Self::Notifications => "notifications".into(),
            Self::Devices => "bluetooth".into(),
            Self::Network => "network".into(),
            Self::Today => "dashboard.calendar".into(),
            _ => format!("dashboard.{}", self.name()),
        }
    }
    fn fixture(self) -> &'static str {
        match self {
            Self::Today => "calendar",
            Self::Devices => "bluetooth",
            other => other.name(),
        }
    }
    pub fn from_view(view: &str) -> Option<Self> {
        let name = view.strip_prefix("dashboard-")?;
        Self::ALL
            .into_iter()
            .chain(Self::EXTRAS)
            .find(|page| page.name() == name)
    }
    /// Index in Nacre's dashboard registry (`nacre tab N`); 4 focus, 6 clipboard
    /// and 9..11 have no published contract and stay unmapped.
    pub fn tab(self) -> usize {
        match self {
            Self::Notifications => 5,
            Self::Devices => 7,
            Self::Network => 8,
            core => Self::ALL.iter().position(|page| *page == core).unwrap_or(0),
        }
    }
    pub fn from_tab(index: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain(Self::EXTRAS)
            .find(|page| page.tab() == index)
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Weather => "Weather",
            Self::Media => "Media",
            Self::System => "System",
            Self::Notifications => "Notifications",
            Self::Devices => "Devices",
            Self::Network => "Network",
        }
    }
}

/// Only transport configuration survives a page's idle retirement.
#[derive(Clone)]
pub struct Choice {
    pub client: Option<crate::nacre_source::Client>,
    pub fixtures: Option<PathBuf>,
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
                    runtime.with_dashboard(
                        page,
                        source,
                        secondary,
                        clock,
                        choice.fixtures.is_some(),
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
        checked(topic, &data)?;
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

pub struct Dashboard {
    page: Page,
    source: Arc<dyn Source>,
    secondary: Option<Arc<dyn Source>>,
    clock: Option<Arc<dyn Source>>,
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
            clock: None,
            preview,
        }
    }
    pub fn with_clock(mut self, clock: Option<Arc<dyn Source>>) -> Self {
        self.clock = clock;
        self
    }
    pub fn project_with_stores(&self, stores: &bead::core::Stores) -> BTreeMap<String, Value> {
        let mut values = self.project();
        if let Some(clock) = &self.clock {
            values.extend(crate::calendar::Calendar::new(clock.clone()).project(stores));
            let snapshot = clock.snapshot();
            let clock = snapshot
                .data
                .get("clock")
                .and_then(Json::as_str)
                .filter(|clock| !clock.is_empty())
                .unwrap_or("—");
            text(&mut values, "clock", clock, 16);
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
        let callback: Callback =
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())));
        let mut subscriptions = vec![self.source.subscribe(callback.clone())?];
        if let Some(secondary) = &self.secondary {
            subscriptions.push(secondary.subscribe(callback.clone())?);
        }
        if let Some(clock) = &self.clock {
            subscriptions.push(clock.subscribe(callback)?);
        }
        Ok(subscriptions)
    }
    pub fn project(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let mut out = BTreeMap::from([
            (
                "dashboard.page".into(),
                Value::Text(self.page.name().into()),
            ),
            (
                "dashboard.title".into(),
                Value::Text(self.page.title().into()),
            ),
            ("dashboard.preview".into(), Value::Bool(self.preview)),
            ("dashboard.ready".into(), Value::Bool(false)),
            ("dashboard.stale".into(), Value::Bool(false)),
            (
                "dashboard.heading".into(),
                Value::Text(format!("{} unavailable", self.page.title())),
            ),
            (
                "dashboard.summary".into(),
                Value::Text("Data is not available yet".into()),
            ),
            ("dashboard.detail".into(), Value::Text(String::new())),
            ("dashboard.metric".into(), Value::Text("—".into())),
            ("dashboard.rows".into(), Value::list(Vec::new())),
            ("dashboard.secondary".into(), Value::list(Vec::new())),
            (
                "dashboard.updates".into(),
                Value::Text("Updates unavailable".into()),
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
                            "Up to date".into()
                        } else {
                            format!("{count} updates")
                        },
                        48,
                    );
                }
                text(
                    &mut out,
                    "updates_detail",
                    if s.health.stale || !update.connected {
                        "Stale data"
                    } else {
                        &s.text
                    },
                    80,
                );
            }
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
                                number(c.temperature, "°"),
                                c.description,
                                c.location
                            ),
                            96,
                        );
                    } else {
                        text(&mut out, "summary", "Weather unavailable", 96);
                    }
                    out.insert(
                        "dashboard.stale".into(),
                        Value::Bool(s.health.stale || !weather.connected),
                    );
                } else {
                    text(&mut out, "summary", "Weather unavailable", 96);
                }
            } else {
                text(&mut out, "summary", "Weather unavailable", 96);
            }
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
                text(&mut out, "heading", "Upcoming events", 48);
                text(
                    &mut out,
                    "detail",
                    if s.configured {
                        if s.events.order.is_empty() {
                            "No upcoming events"
                        } else {
                            ""
                        }
                    } else {
                        "Calendar is not configured"
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
                text(&mut out, "metric", &number(current.temperature, "°"), 24);
                text(
                    &mut out,
                    "detail",
                    &format!(
                        "Feels like {}    Wind {}",
                        number(current.feels_like, "°"),
                        number(current.wind, " km/h")
                    ),
                    96,
                );
                let hours = current.hours.order.iter().take(8).filter_map(|id| current.hours.items.get(id).map(|h| json!({
                    "id":id,"label":line(h.time.get(11..16).unwrap_or(&h.time),20),"value":number(h.temperature,"°"),"detail":number(h.rain,"% rain")
                }))).collect();
                let days = current.days.order.iter().take(7).filter_map(|id| current.days.items.get(id).map(|d| json!({
                    "id":id,"label":line(&d.date,20),"value":format!("{} / {}",number(d.low,"°"),number(d.high,"°")),"detail":number(d.rain,"% rain")
                }))).collect();
                out.insert("dashboard.rows".into(), Value::list(hours));
                out.insert("dashboard.secondary".into(), Value::list(days));
            }
            CoreSnapshot::Media(s) => {
                // This topic is lyrics/palette enrichment. It does not claim a
                // playback identity, progress clock or transport capability.
                text(&mut out, "heading", "Lyrics", 48);
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
                text(&mut out, "heading", "Top processes", 48);
                text(&mut out, "summary", "Memory", 48);
                text(
                    &mut out,
                    "detail",
                    "CPU values are accumulated process time",
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
                    "id":id,"label":line(&p.name,20),"value":bytes(p.memory),"detail":format!("{:.1} s CPU",p.cpu_seconds)
                })).collect();
                let disks = s.disks.order.iter().take(4).filter_map(|id| s.disks.items.get(id).map(|d| json!({
                    "id":id,"label":line(&d.path,10),"value":format!("{} / {}",bytes(d.used),bytes(d.total)),"detail":"Used / total"
                }))).collect();
                out.insert("dashboard.rows".into(), Value::list(rows));
                out.insert("dashboard.secondary".into(), Value::list(disks));
            }
            CoreSnapshot::Notifications(s) => {
                text(&mut out, "heading", "Recent notifications", 48);
                text(
                    &mut out,
                    "summary",
                    if s.do_not_disturb {
                        "Do not disturb is on"
                    } else {
                        "Do not disturb is off"
                    },
                    48,
                );
                // Active first, then the service-ordered history window.
                let rows: Vec<Json> = s
                    .active
                    .order
                    .iter()
                    .filter_map(|id| s.active.items.get(id).map(|n| (id, n)))
                    .chain(
                        s.history
                            .order
                            .iter()
                            .filter_map(|id| s.history.items.get(id).map(|n| (id, n))),
                    )
                    .take(8)
                    .map(|(id, n)| {
                        json!({"id":id,"label":line(&n.summary,40),"value":line(&n.app_name,20),
                            "detail":line(n.body.lines().next().unwrap_or(""),64)})
                    })
                    .collect();
                text(
                    &mut out,
                    "detail",
                    if rows.is_empty() {
                        "No notifications"
                    } else {
                        ""
                    },
                    48,
                );
                out.insert("dashboard.rows".into(), Value::list(rows));
            }
            CoreSnapshot::Bluetooth(s) => {
                let powered = s.adapters.items.values().any(|a| a.powered);
                text(&mut out, "heading", "Bluetooth", 48);
                text(
                    &mut out,
                    "summary",
                    match (s.adapters.items.is_empty(), powered) {
                        (true, _) => "No adapter",
                        (_, true) => "Powered on",
                        _ => "Powered off",
                    },
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
                    "value":match d.battery {Some(b) => format!("{b}% battery"), None if d.connected => "Connected".into(), None => "Saved".into()},
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
                                "value":if n.kind == AudioKind::Output {"Output"} else {"Input"},
                                "detail":if default {"Default"} else {""}})
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
                    primary.map_or("Not connected", |c| &c.name),
                    32,
                );
                text(
                    &mut out,
                    "summary",
                    if s.wireless_enabled {
                        "Wi-Fi is on"
                    } else {
                        "Wi-Fi is off"
                    },
                    48,
                );
                let signal = s
                    .access_points
                    .items
                    .values()
                    .find(|ap| ap.active)
                    .map(|ap| format!("{}%", ap.strength))
                    .unwrap_or_else(|| "—".into());
                text(&mut out, "metric", &signal, 8);
                text(
                    &mut out,
                    "detail",
                    "Traffic rates are not published yet",
                    64,
                );
                let connections = s.connections.order.iter().take(6).filter_map(|id| s.connections.items.get(id).map(|c| json!({
                    "id":id,"label":line(&c.name,28),"value":if c.active {"Active"} else {""},"detail":line(&c.kind,20)
                }))).collect();
                let networks = s
                    .access_points
                    .order
                    .iter()
                    .take(6)
                    .filter_map(|id| {
                        s.access_points.items.get(id).map(|ap| json!({
                    "id":id,"label":line(&ap.name,28),"value":format!("{}%",ap.strength),
                    "detail":if ap.secured {"Secured"} else {"Open"}
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
