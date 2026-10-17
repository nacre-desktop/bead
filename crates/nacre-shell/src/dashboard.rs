// SPDX-License-Identifier: GPL-3.0-only
//! Lazy dashboard projections. No service/backend, polling, ranking or actions.
use bead::core::{
    Value,
    source::{self, Callback, Publisher, Snapshot, Source, Subscription},
};
use nacre_protocol::core_services::CoreSnapshot;
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Weather,
    Media,
    System,
}
impl Page {
    pub const ALL: [Self; 3] = [Self::Weather, Self::Media, Self::System];
    pub fn name(self) -> &'static str {
        match self {
            Self::Weather => "weather",
            Self::Media => "media",
            Self::System => "system",
        }
    }
    pub fn topic(self) -> String {
        format!("dashboard.{}", self.name())
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Weather => "Weather",
            Self::Media => "Media",
            Self::System => "System",
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
        self.open_named(&page.topic(), page.name())
    }
    fn open_named(&self, topic: &str, fixture: &str) -> source::Result<Arc<dyn Source>> {
        if let Some(directory) = &self.fixtures {
            return Ok(Arc::new(ReadOnly::fixture(
                &directory.join(format!("{fixture}.json")),
                topic,
            )?));
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
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let source = choice.open(page).map_err(|e| e.to_string())?;
            let updates = if page == Page::System {
                Some(
                    choice
                        .open_named("dashboard.updates", "updates")
                        .map_err(|e| e.to_string())?,
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
                    runtime.with_dashboard(page, source, updates, choice.fixtures.is_some())
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

struct ReadOnly {
    publisher: Publisher,
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
    let snapshot: CoreSnapshot =
        serde_json::from_value(json!({"topic":topic,"data":data})).map_err(|_| invalid())?;
    if !snapshot.valid() {
        return Err(invalid());
    }
    Ok(snapshot)
}

pub struct Dashboard {
    page: Page,
    source: Arc<dyn Source>,
    updates: Option<Arc<dyn Source>>,
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
            updates,
            preview,
        }
    }
    pub fn subscribe(
        &self,
        notify: Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>,
    ) -> source::Result<Vec<Subscription>> {
        let callback: Callback =
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())));
        let mut subscriptions = vec![self.source.subscribe(callback.clone())?];
        if let Some(updates) = &self.updates {
            subscriptions.push(updates.subscribe(callback)?);
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
        if let Some(updates) = &self.updates {
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
        let Ok(data) = checked(&self.page.topic(), &snapshot.data) else {
            return out;
        };
        let health = match &data {
            CoreSnapshot::Weather(s) => &s.health,
            CoreSnapshot::Media(s) => &s.health,
            CoreSnapshot::System(s) => &s.health,
            _ => return out,
        };
        out.insert("dashboard.ready".into(), Value::Bool(health.available));
        out.insert(
            "dashboard.stale".into(),
            Value::Bool(health.stale || !snapshot.connected),
        );
        if !health.available {
            return out;
        }
        match data {
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
