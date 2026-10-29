// SPDX-License-Identifier: GPL-3.0-only
//! Shared widget drag model and the private Widgets-app relay.
use bead::core::Value;
use bead::services::ipc::{Call, Reply};
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

/// B0198: the bar's measured widget rects per output (output-local), published while a widget
/// drag is active so a desktop-widget drop onto the bar can find the pill and its neighbours.
static BAR_GEOMETRY: Mutex<BTreeMap<String, Vec<Json>>> = Mutex::new(BTreeMap::new());

pub fn note_bar_geometry(output: &str, rects: Vec<Json>) {
    if let Ok(mut map) = BAR_GEOMETRY.lock() {
        map.insert(output.to_owned(), rects);
    }
}

pub fn bar_geometry(output: &str) -> Vec<Json> {
    BAR_GEOMETRY
        .lock()
        .ok()
        .and_then(|map| map.get(output).cloned())
        .unwrap_or_default()
}

/// Headless app behavior used by shell bar/canvas drag hosts. It is intentionally lazy and owns
/// no services; the app's Luau behavior remains the only placement/settings model.
pub struct Controller {
    session: bead::ui::Session,
}

fn source_object(value: &mut Json) -> Option<&mut serde_json::Map<String, Json>> {
    match value.get_mut("source")? {
        Json::Array(rows) => rows.first_mut()?.as_object_mut(),
        Json::Object(source) => Some(source),
        _ => None,
    }
}
fn source_record(value: &Json) -> Option<&Json> {
    let source = value.get("source")?;
    source
        .as_array()
        .and_then(|rows| rows.first())
        .or(Some(source))
}
fn source_is_app(value: &Json) -> bool {
    source_record(value).is_some_and(|source| source["app"] == true)
}
fn gesture_key(event: &Json) -> String {
    let source = source_record(event).cloned().unwrap_or(Json::Null);
    format!(
        "{}:{}:{}",
        source["widget"].as_str().unwrap_or(""),
        source["group"].as_str().unwrap_or(""),
        source["iid"].as_str().unwrap_or("")
    )
}

/// Return output dimensions in logical, post-transform coordinates. Prefer the compositor's
/// already-logical dimensions; otherwise scale mode pixels and swap quarter-turn transforms.
pub(crate) fn logical_dimensions(record: &Json) -> Option<(f64, f64)> {
    let logical = record.get("logical").unwrap_or(&Json::Null);
    if let (Some(width), Some(height)) = (
        record["logicalWidth"]
            .as_f64()
            .or_else(|| record["logical_width"].as_f64())
            .or_else(|| logical["width"].as_f64()),
        record["logicalHeight"]
            .as_f64()
            .or_else(|| record["logical_height"].as_f64())
            .or_else(|| logical["height"].as_f64()),
    ) && width.is_finite()
        && height.is_finite()
        && width > 0.
        && height > 0.
    {
        return Some((width, height));
    }
    let scale = record["scale"]
        .as_f64()
        .filter(|scale| scale.is_finite() && *scale > 0.)
        .or_else(|| {
            logical["scale"]
                .as_f64()
                .filter(|scale| scale.is_finite() && *scale > 0.)
        })
        .unwrap_or(1.);
    let mut width = record["width"].as_f64()? / scale;
    let mut height = record["height"].as_f64()? / scale;
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    let transform = record.get("transform").or_else(|| logical.get("transform"));
    let quarter_turn = match transform {
        Some(Json::Number(value)) => {
            value
                .as_f64()
                .is_some_and(|angle| angle == 90. || angle == 270.)
                || value.as_i64().is_some_and(|turn| turn == 1 || turn == 3)
        }
        Some(Json::String(value)) => value.contains("90") || value.contains("270"),
        _ => false,
    };
    if quarter_turn {
        std::mem::swap(&mut width, &mut height);
    }
    Some((width, height))
}

fn output_records(stores: &bead::core::Stores) -> Vec<Json> {
    let value = stores
        .get("bead.outputs")
        .or_else(|| stores.get("widgets.outputs"));
    match value {
        Some(Value::List(rows)) => rows.iter().cloned().collect(),
        _ => Vec::new(),
    }
}
fn output(record: &Json) -> Option<(String, f64, f64, f64, f64)> {
    let number = |key: &str| record[key].as_f64().filter(|n| n.is_finite());
    let x = number("x").filter(|v| v.abs() <= 20_000.).unwrap_or(0.);
    let y = number("y").filter(|v| v.abs() <= 20_000.).unwrap_or(0.);
    let (width, height) = logical_dimensions(record)?;
    if width <= 0. || height <= 0. || width > 20_000. || height > 20_000. {
        return None;
    }
    Some((
        record["name"].as_str().unwrap_or_default().to_owned(),
        x,
        y,
        width,
        height,
    ))
}

/// Add trusted surface origins and target output-local coordinates to one transfer record.
pub fn prepare_event(
    argument: &Value,
    stores: &bead::core::Stores,
    app_source: bool,
    source_output: &str,
) -> Value {
    let Value::List(records) = argument else {
        return argument.clone();
    };
    let Some(first) = records.first() else {
        return argument.clone();
    };
    let Ok(mut event) = serde_json::to_value(first) else {
        return argument.clone();
    };
    if !event.is_object() {
        return argument.clone();
    }
    let mut outputs: Vec<_> = output_records(stores).iter().filter_map(output).collect();
    if outputs.is_empty() {
        let width = stores.get("widgets.screen_width").and_then(|v| {
            if let Value::Number(n) = v {
                Some(*n)
            } else {
                None
            }
        });
        let height = stores.get("widgets.screen_height").and_then(|v| {
            if let Value::Number(n) = v {
                Some(*n)
            } else {
                None
            }
        });
        if let (Some(width), Some(height)) = (width, height) {
            let x = stores
                .get("widgets.screen_origin_x")
                .and_then(|v| {
                    if let Value::Number(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.);
            let y = stores
                .get("widgets.screen_origin_y")
                .and_then(|v| {
                    if let Value::Number(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.);
            let output_name = if source_output.is_empty() {
                event["app_output_id"].as_str().unwrap_or_default()
            } else {
                source_output
            };
            outputs.push((output_name.to_owned(), x, y, width, height));
        }
    }
    let active_output = if source_output.is_empty() {
        event["app_output_id"]
            .as_str()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                stores.get("widgets.drag.app_output_id").and_then(|value| {
                    if let Value::Text(name) = value {
                        (!name.is_empty()).then_some(name.as_str())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
    } else {
        source_output
    };
    if !active_output.is_empty()
        && !outputs.iter().any(|output| output.0 == active_output)
        && let Some((name, ..)) = outputs.iter_mut().find(|output| output.0.is_empty())
    {
        *name = active_output.to_owned();
    }
    let pointer = |key: &str| event[key].as_f64().filter(|n| n.is_finite()).unwrap_or(0.);
    let has_global = event["global_x"].as_f64().is_some() && event["global_y"].as_f64().is_some();
    let source_output_geometry = outputs
        .iter()
        .find(|(name, ..)| name == source_output)
        .or_else(|| outputs.first());
    let source_x = if has_global {
        event["source_origin_x"]
            .as_f64()
            .or_else(|| event["origin_x"].as_f64())
            .unwrap_or(0.)
    } else if app_source {
        stores
            .get("widgets.drag.app_origin_x")
            .and_then(|v| {
                if let Value::Number(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0.)
    } else {
        source_output_geometry.map_or(0., |(_, x, _, _, _)| *x)
            + event["source_origin_x"].as_f64().unwrap_or(0.)
    };
    let source_y = if has_global {
        event["source_origin_y"]
            .as_f64()
            .or_else(|| event["origin_y"].as_f64())
            .unwrap_or(0.)
    } else if app_source {
        stores
            .get("widgets.drag.app_origin_y")
            .and_then(|v| {
                if let Value::Number(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0.)
    } else {
        source_output_geometry.map_or(0., |(_, _, y, _, _)| *y)
            + event["source_origin_y"].as_f64().unwrap_or(0.)
    };
    let source_valid = if has_global {
        event["source_origin_valid"].as_bool().unwrap_or(true)
    } else {
        !app_source || stores.get("widgets.drag.app_origin_valid") == Some(&Value::Bool(true))
    };
    let app_source = app_source || source_is_app(&event);
    let app_x = if has_global {
        event["app_origin_x"].as_f64().unwrap_or(source_x)
    } else {
        source_x
    };
    let app_y = if has_global {
        event["app_origin_y"].as_f64().unwrap_or(source_y)
    } else {
        source_y
    };
    let app_width = if has_global {
        event["app_width"].as_f64().unwrap_or(0.)
    } else {
        stores
            .get("widgets.drag.app_width")
            .and_then(|v| {
                if let Value::Number(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0.)
    };
    let app_height = if has_global {
        event["app_height"].as_f64().unwrap_or(0.)
    } else {
        stores
            .get("widgets.drag.app_height")
            .and_then(|v| {
                if let Value::Number(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(0.)
    };
    let app_output = event["app_output_id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            stores.get("widgets.drag.app_output_id").and_then(|v| {
                if let Value::Text(name) = v {
                    Some(name.clone())
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();
    let surface_global_x = source_x;
    let surface_global_y = source_y;
    let global_x = event["global_x"]
        .as_f64()
        .filter(|n| n.is_finite())
        .unwrap_or(pointer("x") + surface_global_x);
    let global_y = event["global_y"]
        .as_f64()
        .filter(|n| n.is_finite())
        .unwrap_or(pointer("y") + surface_global_y);
    let target = outputs
        .iter()
        .find(|(_, x, y, w, h)| {
            global_x >= *x && global_y >= *y && global_x < *x + *w && global_y < *y + *h
        })
        .or(source_output_geometry);
    let bar_height = stores
        .get("bar.height")
        .and_then(|v| {
            if let Value::Number(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .unwrap_or(50.);
    let destination = target.and_then(|(name, x, y, w, h)| {
        let local_x = global_x - *x;
        let local_y = global_y - *y;
        (local_x >= 0. && local_x < *w && local_y >= 0. && local_y < *h).then_some((
            local_x,
            local_y,
            *x,
            *y,
            *w,
            *h,
            name.clone(),
        ))
    });
    let from_canvas =
        !app_source && source_record(&event).is_some_and(|source| source["host"] == "canvas");
    let inside_app = app_source
        && app_width > 0.
        && app_height > 0.
        && global_x >= app_x
        && global_y >= app_y
        && global_x < app_x + app_width
        && global_y < app_y + app_height;
    let empty_inside = inside_app && event["target_action"].as_str().is_none();
    if let Some(object) = event.as_object_mut() {
        object.insert("source_origin_x".into(), json!(source_x));
        object.insert("source_origin_y".into(), json!(source_y));
        object.insert("source_origin_valid".into(), json!(source_valid));
        object.insert("global_x".into(), json!(global_x));
        object.insert("global_y".into(), json!(global_y));
        object.insert("app_origin_x".into(), json!(app_x));
        object.insert("app_origin_y".into(), json!(app_y));
        object.insert("app_width".into(), json!(app_width));
        object.insert("app_height".into(), json!(app_height));
        object.insert("app_output_id".into(), json!(app_output));
        if empty_inside {
            object.insert("destination".into(), json!(""));
            object.insert("target_output_id".into(), json!(app_output));
            if let Some((_, origin_x, origin_y, width, height)) =
                outputs.iter().find(|(name, ..)| name == &app_output)
            {
                object.insert("target_origin_x".into(), json!(origin_x));
                object.insert("target_origin_y".into(), json!(origin_y));
                object.insert("output_origin_x".into(), json!(origin_x));
                object.insert("output_origin_y".into(), json!(origin_y));
                object.insert("screen_width".into(), json!(width));
                object.insert("screen_height".into(), json!(height));
            }
        } else if let Some((local_x, local_y, origin_x, origin_y, width, height, target_name)) =
            destination
        {
            // B0198: compositors confine a desktop-widget (Bottom layer) drag to the work
            // area, so its pointer stops at the bottom of the reserved top edge (about 67 px
            // under a 50 px bar) and never reaches the bar. Within half a bar height below the
            // bar counts as the bar for those drags (the overlay's desktop zone starts there).
            let reach = if from_canvas {
                bar_height * 1.5
            } else {
                bar_height
            };
            object.insert(
                "destination".into(),
                json!(if local_y < reach { "bar" } else { "desktop" }),
            );
            object.insert("destination_x".into(), json!(local_x));
            object.insert("destination_y".into(), json!(local_y));
            object.insert("output_origin_x".into(), json!(origin_x));
            object.insert("output_origin_y".into(), json!(origin_y));
            object.insert("target_origin_x".into(), json!(origin_x));
            object.insert("target_origin_y".into(), json!(origin_y));
            object.insert("screen_width".into(), json!(width));
            object.insert("screen_height".into(), json!(height));
            object.insert("target_output_id".into(), json!(target_name));
        } else {
            object.insert("destination".into(), json!(""));
        }
    }
    if app_source && let Some(source) = source_object(&mut event) {
        source.insert("app".into(), json!(true));
    }
    Value::list(vec![event])
}

impl Controller {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let relative = PathBuf::from("shell");
        let root = if relative.join("palette.json").is_file() {
            relative
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell")
        };
        let palette_path = root.join("palette.json");
        let palette = bead::render::Palette::read(&palette_path)?;
        let mut fallback = crate::sound::fallback(palette.get("surface")?);
        fallback.width = 1.;
        fallback.bounds = bead::render::Rect::new(0., 0., 1., 1.);
        fallback.primitives.clear();
        fallback.motion = None;
        let mut session = bead::ui::Session::new(
            root,
            "widgets/app/controller.luau".into(),
            palette_path,
            palette,
            fallback,
        );
        session.set_viewport(1, 1);
        let scene = session.reload()?;
        session.check_sources()?;
        session.commit(scene);
        Ok(Self { session })
    }

    /// Synchronize only stores declared by the canonical Widgets app behavior. Bar/canvas stores
    /// outside that schema remain owned by their actual surface session.
    pub fn sync(&mut self, values: &BTreeMap<String, Value>) -> Result<(), Box<dyn Error>> {
        let compatible: BTreeMap<_, _> = values
            .iter()
            .filter(|(name, _)| {
                !name.starts_with("widgets.drag.") && self.session.stores.get(name).is_some()
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        self.session.stores.commit(compatible)?;
        Ok(())
    }

    pub fn drag(&mut self, argument: Value) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
        let changes = self.session.action_values("widgets.drag", argument)?;
        if let Some(scene) = self.session.update(changes)? {
            self.session.commit(scene);
        }
        Ok(self.session.stores.values().clone())
    }
    pub fn edit(&mut self, argument: Value) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
        self.session.stores.commit(BTreeMap::from([(
            "widgets.write".into(),
            Value::list(Vec::new()),
        )]))?;
        let changes = self.session.action_values("widgets.edit", argument)?;
        if let Some(scene) = self.session.update(changes)? {
            self.session.commit(scene);
        }
        Ok(self.session.stores.values().clone())
    }
    #[cfg(test)]
    pub fn apply_parent_history(
        &mut self,
        previous_document: Json,
        document: Json,
        revision: u64,
    ) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
        let changes = self.session.action_values(
            "widgets.history.parent",
            Value::list(vec![json!({
                "previous_document":previous_document,"document":document,"revision":revision
            })]),
        )?;
        if let Some(scene) = self.session.update(changes)? {
            self.session.commit(scene);
        }
        Ok(self.session.stores.values().clone())
    }
    pub fn drag_epoch(&self) -> u64 {
        match self.session.stores.get("widgets.drag.epoch") {
            Some(Value::Number(epoch)) if epoch.is_finite() && *epoch >= 0. => *epoch as u64,
            _ => 0,
        }
    }
}

#[derive(Clone)]
pub enum PrivateEvent {
    AppState {
        pid: u32,
        generation: u64,
        open: bool,
        floating: bool,
    },
    Motion {
        pid: u32,
        epoch: u64,
        event: Json,
    },
}
type Callback = Arc<dyn Fn(PrivateEvent) + Send + Sync>;
enum QueueEvent {
    AppState {
        pid: u32,
        open: bool,
        floating: bool,
    },
    AppStateClose {
        pid: u32,
        done: mpsc::SyncSender<bool>,
    },
    AppExit {
        pid: u32,
        generation: u64,
    },
    Motion {
        pid: u32,
        epoch: u64,
        event: Json,
    },
}
fn retain_close_barriers(queue: &mut VecDeque<QueueEvent>, pid: u32, done: mpsc::SyncSender<bool>) {
    queue.retain(|event| {
        !matches!(event,
        QueueEvent::Motion{pid:queued,event,..} if *queued==pid && event["phase"]=="move")
    });
    queue.push_back(QueueEvent::AppStateClose { pid, done });
}
fn coalesces(previous: &QueueEvent, next: &QueueEvent) -> bool {
    matches!((previous,next),
        (QueueEvent::Motion{pid:a,epoch:ae,event:old},QueueEvent::Motion{pid:b,epoch:be,event:new})
            if a==b && ae==be && old["phase"]=="move" && new["phase"]=="move")
}
struct Queue {
    events: Mutex<VecDeque<QueueEvent>>,
    running: AtomicBool,
    outbound: bool,
}
impl Queue {
    fn new(outbound: bool) -> Self {
        Self {
            events: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
            outbound,
        }
    }
    fn enqueue(self: &Arc<Self>, event: QueueEvent) {
        let Ok(mut queue) = self.events.lock() else {
            return;
        };
        let replace = queue
            .back()
            .is_some_and(|previous| coalesces(previous, &event));
        if replace {
            queue.pop_back();
        }
        queue.push_back(event);
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let this = self.clone();
        if std::thread::Builder::new()
            .name(
                if self.outbound {
                    "widgets-drag-ipc"
                } else {
                    "widgets-drag-relay"
                }
                .into(),
            )
            .stack_size(128 * 1024)
            .spawn(move || this.run())
            .is_err()
        {
            self.running.store(false, Ordering::Release);
        }
    }
    fn close_and_wait(self: &Arc<Self>, pid: u32, timeout: Duration) -> bool {
        let (done, received) = mpsc::sync_channel(1);
        let Ok(mut queue) = self.events.lock() else {
            return false;
        };
        retain_close_barriers(&mut queue, pid, done);
        if !self.running.swap(true, Ordering::AcqRel) {
            let this = self.clone();
            if std::thread::Builder::new()
                .name("widgets-drag-close".into())
                .stack_size(128 * 1024)
                .spawn(move || this.run())
                .is_err()
            {
                self.running.store(false, Ordering::Release);
                return false;
            }
        }
        drop(queue);
        received.recv_timeout(timeout).unwrap_or(false)
    }
    fn run(self: Arc<Self>) {
        loop {
            let next = {
                let mut queue = match self.events.lock() {
                    Ok(queue) => queue,
                    Err(_) => {
                        self.running.store(false, Ordering::Release);
                        return;
                    }
                };
                match queue.pop_front() {
                    Some(event) => Some(event),
                    None => {
                        self.running.store(false, Ordering::Release);
                        return;
                    }
                }
            };
            let Some(event) = next else { continue };
            if self.outbound {
                match event {
                    QueueEvent::AppStateClose { pid, done } => {
                        let sent = send_to_parent(
                            QueueEvent::AppState {
                                pid,
                                open: false,
                                floating: false,
                            },
                            Duration::from_millis(75),
                        );
                        if !sent {
                            dispatch(QueueEvent::AppState {
                                pid,
                                open: false,
                                floating: false,
                            });
                        }
                        let _ = done.send(sent);
                    }
                    QueueEvent::AppState {
                        pid,
                        open,
                        floating,
                    } => {
                        if !send_to_parent(
                            QueueEvent::AppState {
                                pid,
                                open,
                                floating,
                            },
                            Duration::from_millis(60),
                        ) {
                            dispatch(QueueEvent::AppState {
                                pid,
                                open,
                                floating,
                            });
                        }
                    }
                    event => {
                        send_to_parent(event, Duration::from_millis(60));
                    }
                }
            } else {
                dispatch(event);
            }
        }
    }
}
static OUTBOUND: OnceLock<Arc<Queue>> = OnceLock::new();
static INBOUND: OnceLock<Arc<Queue>> = OnceLock::new();
static LISTENERS: OnceLock<Mutex<BTreeMap<u64, Callback>>> = OnceLock::new();
static NEXT_LISTENER: AtomicU64 = AtomicU64::new(1);
static GESTURE: Mutex<(u64, u64, String)> = Mutex::new((0, 0, String::new()));

fn send_to_parent(event: QueueEvent, timeout: Duration) -> bool {
    let (method, payload) = match event {
        QueueEvent::AppState {
            pid,
            open,
            floating,
        } => (
            "state",
            json!({"app":"nacre-widgets","pid":pid,"open":open,"floating":floating}),
        ),
        QueueEvent::AppStateClose { .. } | QueueEvent::AppExit { .. } => return false,
        QueueEvent::Motion { pid, epoch, event } => (
            "motion",
            json!({"app":"nacre-widgets","pid":pid,"epoch":epoch,"event":event}),
        ),
    };
    match crate::app::widget_private_call_timeout(method, payload, timeout) {
        Ok(()) => true,
        Err(_) => {
            eprintln!("WIDGET_DRAG_IPC_ERROR method={method}");
            false
        }
    }
}
#[derive(Default)]
struct AppLifecycle {
    next_generation: u64,
    active: Option<(u32, u64)>,
    floating: bool,
}
impl AppLifecycle {
    fn open(&mut self, pid: u32, floating: bool) -> (u64, bool, Option<(u32, u64)>) {
        if let Some((active, generation)) = self.active
            && active == pid
        {
            self.floating = floating;
            return (generation, false, None);
        }
        let previous = self.active.filter(|(active, _)| *active != pid);
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.active = Some((pid, generation));
        self.floating = floating;
        (generation, true, previous)
    }
    fn close(&mut self, pid: u32, generation: Option<u64>) -> Option<u64> {
        let (active, current) = self.active?;
        if active != pid || generation.is_some_and(|value| value != current) {
            return None;
        }
        self.active = None;
        self.floating = false;
        Some(current)
    }
    fn generation(&self, pid: u32) -> Option<u64> {
        self.active
            .filter(|(active, _)| *active == pid)
            .map(|(_, generation)| generation)
    }
    fn active(&self) -> Option<(u32, u64)> {
        self.active
    }
    fn active_policy(&self) -> Option<(u32, u64, bool)> {
        self.active
            .map(|(pid, generation)| (pid, generation, self.floating))
    }
}
static APP_LIFECYCLE: OnceLock<Mutex<AppLifecycle>> = OnceLock::new();
#[derive(Default)]
struct WidgetFloatingClaim(Option<(u32, u64)>);
impl WidgetFloatingClaim {
    fn claim(&mut self, active: Option<(u32, u64)>, identity: (u32, u64)) -> bool {
        if active != Some(identity) || self.0 == Some(identity) {
            return false;
        }
        self.0 = Some(identity);
        true
    }
    fn release(&mut self, identity: (u32, u64)) {
        if self.0 == Some(identity) {
            self.0 = None;
        }
    }
}
static WIDGET_FLOATING_CLAIM: Mutex<WidgetFloatingClaim> = Mutex::new(WidgetFloatingClaim(None));
static ACTIVE_GESTURES: OnceLock<Mutex<BTreeMap<(u32, u64), Json>>> = OnceLock::new();
fn callbacks() -> Vec<Callback> {
    LISTENERS
        .get_or_init(Default::default)
        .lock()
        .map(|listeners| listeners.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}
fn notify(event: PrivateEvent) {
    for callback in callbacks() {
        callback(event.clone());
    }
}
fn take_cancelled_gestures(
    gestures: &mut BTreeMap<(u32, u64), Json>,
    pid: u32,
) -> Vec<(u32, u64, Json)> {
    let keys = gestures
        .keys()
        .filter(|(active_pid, _)| *active_pid == pid)
        .copied()
        .collect::<Vec<_>>();
    keys.into_iter()
        .filter_map(|(active_pid, epoch)| {
            gestures.remove(&(active_pid, epoch)).map(|mut event| {
                event["phase"] = json!("cancel");
                (active_pid, epoch, event)
            })
        })
        .collect()
}
fn close_gestures(pid: u32) {
    let pending = ACTIVE_GESTURES
        .get_or_init(Default::default)
        .lock()
        .map(|mut gestures| take_cancelled_gestures(&mut gestures, pid))
        .unwrap_or_default();
    for (pid, epoch, event) in pending {
        notify(PrivateEvent::Motion { pid, epoch, event });
    }
}
fn track_gesture(pid: u32, epoch: u64, event: &Json) {
    let mut gestures = ACTIVE_GESTURES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match event["phase"].as_str() {
        Some("move") => {
            gestures.insert((pid, epoch), event.clone());
        }
        Some("drop" | "cancel") => {
            gestures.remove(&(pid, epoch));
        }
        _ => {}
    }
}
fn open_pidfd(pid: u32) -> std::io::Result<OwnedFd> {
    #[cfg(target_os = "linux")]
    {
        let fd =
            unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) as libc::c_int };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "pidfd is Linux-only",
        ))
    }
}
fn wait_for_pid_exit(pidfd: OwnedFd) -> bool {
    #[cfg(target_os = "linux")]
    {
        let mut descriptor = libc::pollfd {
            fd: pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            let result = unsafe { libc::poll(&mut descriptor, 1, -1) };
            if result > 0 {
                return true;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return false;
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pidfd;
        false
    }
}
fn enqueue_app_exit(pid: u32, generation: u64) {
    INBOUND
        .get_or_init(|| Arc::new(Queue::new(false)))
        .enqueue(QueueEvent::AppExit { pid, generation });
}
fn watch_app(pid: u32, generation: u64, pidfd: OwnedFd) {
    let spawn = std::thread::Builder::new()
        .name("widgets-app-pidfd".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            if !wait_for_pid_exit(pidfd) {
                eprintln!("WIDGET_APP_PIDFD_ERROR pid={pid}");
                return;
            }
            enqueue_app_exit(pid, generation);
        });
    if spawn.is_err() {
        eprintln!("WIDGET_APP_PIDFD_THREAD_ERROR pid={pid}");
        enqueue_app_exit(pid, generation);
    }
}

fn dispatch(event: QueueEvent) {
    match event {
        QueueEvent::AppState {
            pid,
            open: true,
            floating,
        } => {
            let pidfd = if pid == std::process::id() {
                None
            } else {
                match open_pidfd(pid) {
                    Ok(pidfd) => Some(pidfd),
                    Err(error) if error.raw_os_error() == Some(libc::ESRCH) => return,
                    Err(error) => {
                        eprintln!("WIDGET_APP_PIDFD_OPEN_ERROR pid={pid} error={error}");
                        None
                    }
                }
            };
            let (generation, start_watch, previous) = APP_LIFECYCLE
                .get_or_init(Default::default)
                .lock()
                .map(|mut state| state.open(pid, floating))
                .unwrap_or((0, false, None));
            if let Some((previous, previous_generation)) = previous {
                close_gestures(previous);
                notify(PrivateEvent::AppState {
                    pid: previous,
                    generation: previous_generation,
                    open: false,
                    floating: false,
                });
            }
            notify(PrivateEvent::AppState {
                pid,
                generation,
                open: true,
                floating,
            });
            if start_watch {
                if let Some(pidfd) = pidfd {
                    watch_app(pid, generation, pidfd);
                } else if pid != std::process::id() {
                    enqueue_app_exit(pid, generation);
                }
            }
        }
        QueueEvent::AppState {
            pid, open: false, ..
        } => {
            let generation = APP_LIFECYCLE
                .get_or_init(Default::default)
                .lock()
                .ok()
                .and_then(|mut state| state.close(pid, None));
            if let Some(generation) = generation {
                close_gestures(pid);
                notify(PrivateEvent::AppState {
                    pid,
                    generation,
                    open: false,
                    floating: false,
                });
            }
        }
        QueueEvent::AppExit { pid, generation } => {
            let closed = APP_LIFECYCLE
                .get_or_init(Default::default)
                .lock()
                .ok()
                .and_then(|mut state| state.close(pid, Some(generation)));
            if let Some(generation) = closed {
                close_gestures(pid);
                notify(PrivateEvent::AppState {
                    pid,
                    generation,
                    open: false,
                    floating: false,
                });
            }
        }
        QueueEvent::Motion {
            pid,
            epoch,
            mut event,
        } => {
            track_gesture(pid, epoch, &event);
            let generation = APP_LIFECYCLE
                .get_or_init(Default::default)
                .lock()
                .ok()
                .and_then(|state| state.generation(pid));
            if let Some(object) = event.as_object_mut()
                && let Some(generation) = generation
            {
                object.insert("source_generation".into(), json!(generation));
            }
            notify(PrivateEvent::Motion { pid, epoch, event });
        }
        QueueEvent::AppStateClose { done, .. } => {
            let _ = done.send(true);
        }
    }
}

pub fn subscribe(callback: Callback) -> Subscription {
    let id = NEXT_LISTENER.fetch_add(1, Ordering::Relaxed);
    LISTENERS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, callback);
    Subscription(id)
}
pub struct Subscription(u64);
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(listeners) = LISTENERS.get() {
            listeners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.0);
        }
    }
}

pub fn app_state(open: bool, floating: bool) {
    OUTBOUND
        .get_or_init(|| Arc::new(Queue::new(true)))
        .enqueue(QueueEvent::AppState {
            pid: std::process::id(),
            open,
            floating,
        });
}
pub fn app_state_close(timeout: Duration) -> bool {
    OUTBOUND
        .get_or_init(|| Arc::new(Queue::new(true)))
        .close_and_wait(std::process::id(), timeout)
}
fn gesture_epoch(event: &Json) -> u64 {
    let key = gesture_key(event);
    let phase = event["phase"].as_str().unwrap_or("");
    let mut gesture = GESTURE.lock().unwrap_or_else(|e| e.into_inner());
    if phase == "move" && (gesture.0 == 0 || gesture.2 != key) {
        gesture.1 = gesture.1.wrapping_add(1).max(1);
        gesture.0 = gesture.1;
        gesture.2 = key;
    }
    let epoch = if gesture.0 == 0 {
        gesture.1.wrapping_add(1).max(1)
    } else {
        gesture.0
    };
    if phase == "drop" || phase == "cancel" {
        gesture.0 = 0;
        gesture.2.clear();
    }
    epoch
}
pub fn app_drag(event: Json) -> u64 {
    let pid = std::process::id();
    let epoch = gesture_epoch(&event);
    OUTBOUND
        .get_or_init(|| Arc::new(Queue::new(true)))
        .enqueue(QueueEvent::Motion { pid, epoch, event });
    epoch
}
pub fn terminal_feedback(argument: &Value) -> Option<(u32, u64, u64)> {
    let Value::List(records) = argument else {
        return None;
    };
    let event = records.first()?;
    if event["phase"].as_str() != Some("drop")
        || event["private_remote"] != true
        || !source_is_app(event)
    {
        return None;
    }
    Some((
        u32::try_from(event["source_pid"].as_u64()?).ok()?,
        event["source_generation"].as_u64()?,
        event["gesture_epoch"].as_u64()?,
    ))
}
pub fn send_app_feedback(
    pid: u32,
    generation: u64,
    epoch: u64,
    success: bool,
    error: &str,
) -> bool {
    let current = APP_LIFECYCLE
        .get_or_init(Default::default)
        .lock()
        .ok()
        .and_then(|state| state.generation(pid));
    if current != Some(generation) {
        return false;
    }
    crate::app::widget_feedback(pid, generation, epoch, success, error).is_ok()
}
pub fn active_app_identity() -> Option<(u32, u64)> {
    APP_LIFECYCLE
        .get_or_init(Default::default)
        .lock()
        .ok()
        .and_then(|state| state.active())
}
pub fn active_app_policy() -> Option<(u32, u64, bool)> {
    APP_LIFECYCLE
        .get_or_init(Default::default)
        .lock()
        .ok()
        .and_then(|state| state.active_policy())
}
pub fn claim_active_app_floating(pid: u32, generation: u64) -> bool {
    let active = active_app_identity();
    WIDGET_FLOATING_CLAIM
        .lock()
        .is_ok_and(|mut claim| claim.claim(active, (pid, generation)))
}
pub fn release_app_floating(pid: u32, generation: u64) {
    if let Ok(mut claim) = WIDGET_FLOATING_CLAIM.lock() {
        claim.release((pid, generation));
    }
}
pub fn send_app_history_feedback(
    pid: u32,
    generation: u64,
    epoch: Option<u64>,
    previous_document: Json,
    previous_revision: u64,
    document: Json,
    revision: u64,
) -> bool {
    if active_app_identity() != Some((pid, generation)) {
        return false;
    }
    crate::app::widget_history_feedback(
        pid,
        generation,
        epoch,
        previous_document,
        previous_revision,
        document,
        revision,
    )
    .is_ok()
}

/// Private child-to-parent command. It is intentionally outside Nacre's published daemon API.
pub fn private_call(call: Call) -> Reply {
    let invalid = || Reply::error(call.id, "invalid-route", "invalid Widgets private event");
    if call.version != 1 || call.args.len() != 1 {
        return invalid();
    }
    let Some(payload) = call.args.first() else {
        return invalid();
    };
    if payload["app"].as_str() != Some("nacre-widgets") || payload["pid"].as_u64().is_none() {
        return invalid();
    }
    let Some(pid) = payload["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
    else {
        return invalid();
    };
    match call.method.as_str() {
        "state" => {
            match payload["open"].as_bool() {
                Some(open) => INBOUND.get_or_init(|| Arc::new(Queue::new(false))).enqueue(
                    QueueEvent::AppState {
                        pid,
                        open,
                        floating: payload["floating"].as_bool().unwrap_or(false),
                    },
                ),
                None => return invalid(),
            }
        }
        "motion" => {
            let Some(epoch) = payload["epoch"].as_u64() else {
                return invalid();
            };
            let Some(event) = payload
                .get("event")
                .filter(|event| event.is_object())
                .cloned()
            else {
                return invalid();
            };
            if !matches!(
                event["phase"].as_str(),
                Some("move" | "drop" | "cancel" | "entry-seed")
            ) || source_record(&event)
                .and_then(|source| source["widget"].as_str())
                .is_none()
            {
                return invalid();
            }
            INBOUND
                .get_or_init(|| Arc::new(Queue::new(false)))
                .enqueue(QueueEvent::Motion { pid, epoch, event });
        }
        _ => return Reply::error(call.id, "unknown_method", "unknown Widgets private method"),
    }
    Reply::ok(call.id, Json::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_coordinates_are_global_then_target_output_local() {
        let stores=bead::core::Stores::default().reconcile(&BTreeMap::from([
            ("bar.height".into(),Value::Number(50.)),
            ("widgets.outputs".into(),Value::list(vec![json!({"name":"right","x":800.,"y":50.,"width":1200.,"height":800.,"scale":1.})])),
        ])).unwrap();
        let event = Value::list(vec![
            json!({"phase":"move","x":20.,"y":40.,"source_origin_x":100.,"source_origin_y":20.,
            "source_origin_valid":true,"source":[{"widget":"clock","group":"clock-main"}]}),
        ]);
        let result = prepare_event(&event, &stores, false, "right");
        let Value::List(records) = result else {
            panic!("expected one transfer event")
        };
        let event = &records[0];
        assert_eq!(event["global_x"], json!(920.));
        assert_eq!(event["destination_x"], json!(120.));
        assert_eq!(event["destination_y"], json!(60.));
        assert_eq!(event["destination"], json!("desktop"));
        assert_eq!(source_record(event).unwrap()["group"], json!("clock-main"));
    }

    #[test]
    fn a_confined_desktop_widget_drag_reaches_the_bar_from_the_work_area_top() {
        let stores=bead::core::Stores::default().reconcile(&BTreeMap::from([
            ("bar.height".into(),Value::Number(50.)),
            ("widgets.outputs".into(),Value::list(vec![json!({"name":"right","x":0.,"y":0.,"width":1920.,"height":1200.,"scale":1.})])),
        ])).unwrap();
        let destination = |host: &str, y: f64| {
            let event = Value::list(vec![json!({"phase":"drop","x":1879.,"y":y,
                "source_origin_x":0.,"source_origin_y":0.,"source_origin_valid":true,
                "source":[{"widget":"clock","group":"clock","host":host}]})]);
            let Value::List(records) = prepare_event(&event, &stores, false, "right") else {
                panic!("expected one transfer event")
            };
            records[0]["destination"].clone()
        };
        // The nested compositor stops the canvas pointer at y=67.3 under a 50 px bar.
        assert_eq!(destination("canvas", 67.3), json!("bar"));
        assert_eq!(destination("canvas", 80.), json!("desktop"));
        assert_eq!(destination("top_bar", 67.3), json!("desktop"));
        assert_eq!(destination("top_bar", 25.), json!("bar"));
    }

    #[test]
    fn app_drop_fills_missing_output_name_from_the_active_parent_output() {
        let stores = bead::core::Stores::default()
            .reconcile(&BTreeMap::from([
                ("bar.height".into(), Value::Number(50.)),
                ("widgets.screen_width".into(), Value::Number(1920.)),
                ("widgets.screen_height".into(), Value::Number(1080.)),
                ("widgets.drag.app_origin_x".into(), Value::Number(410.)),
                ("widgets.drag.app_origin_y".into(), Value::Number(220.)),
                ("widgets.drag.app_origin_valid".into(), Value::Bool(true)),
                (
                    "widgets.drag.app_output_id".into(),
                    Value::Text("winit".into()),
                ),
                (
                    "widgets.outputs".into(),
                    Value::list(vec![
                        json!({"name":"","x":0.,"y":0.,"width":1920.,"height":1080.,"scale":1.}),
                    ]),
                ),
            ]))
            .unwrap();
        let request = Value::list(vec![json!({"phase":"drop","x":1484.,"y":838.,
            "source_origin_valid":true,"source":[{"app":true,"widget":"clock","form":"desktop"}]})]);
        let Value::List(records) = prepare_event(&request, &stores, true, "") else {
            panic!("expected one app drag record")
        };
        assert_eq!(records[0]["global_x"], json!(1894.));
        assert_eq!(records[0]["global_y"], json!(1058.));
        assert_eq!(records[0]["destination"], json!("desktop"));
        assert_eq!(records[0]["target_output_id"], json!("winit"));
    }

    #[test]
    fn terminal_feedback_extracts_remote_app_drop_pid_generation_and_epoch() {
        let make = |phase: &str| {
            Value::list(vec![
                json!({"phase":phase,"private_remote":true,"source_pid":4,
            "source_generation":9,"gesture_epoch":12,"source":[{"app":true,"widget":"clock"}]}),
            ])
        };
        assert_eq!(terminal_feedback(&make("drop")), Some((4, 9, 12)));
        assert_eq!(terminal_feedback(&make("move")), None);
        let mut wrong = make("drop");
        let Value::List(records) = &mut wrong else {
            unreachable!()
        };
        std::sync::Arc::make_mut(records)[0]
            .as_object_mut()
            .unwrap()
            .remove("source_generation");
        assert_eq!(terminal_feedback(&wrong), None);
    }

    #[test]
    fn private_drag_accepts_the_payload_list_shape_from_markup() {
        let event = json!({"phase":"drop","source":[{"widget":"clock","group":"clock-main"}]});
        assert_eq!(source_record(&event).unwrap()["widget"], json!("clock"));
        assert_eq!(gesture_key(&event), "clock:clock-main:");
    }

    #[test]
    fn relay_only_replaces_adjacent_moves_in_the_same_gesture() {
        let moved = QueueEvent::Motion {
            pid: 4,
            epoch: 9,
            event: json!({"phase":"move"}),
        };
        let latest = QueueEvent::Motion {
            pid: 4,
            epoch: 9,
            event: json!({"phase":"move","x":90}),
        };
        assert!(coalesces(&moved, &latest));
        assert!(!coalesces(
            &moved,
            &QueueEvent::Motion {
                pid: 4,
                epoch: 10,
                event: json!({"phase":"move"})
            }
        ));
        assert!(!coalesces(
            &moved,
            &QueueEvent::Motion {
                pid: 4,
                epoch: 9,
                event: json!({"phase":"drop"})
            }
        ));
        assert!(!coalesces(
            &QueueEvent::AppState {
                pid: 4,
                open: true,
                floating: true
            },
            &latest
        ));
    }

    #[test]
    fn close_barrier_discards_queued_motion_but_keeps_ordered_drop_and_cancel() {
        let (done, _) = mpsc::sync_channel(1);
        let mut queue = VecDeque::from([
            QueueEvent::Motion {
                pid: 8,
                epoch: 2,
                event: json!({"phase":"move","x":1}),
            },
            QueueEvent::Motion {
                pid: 8,
                epoch: 2,
                event: json!({"phase":"drop"}),
            },
            QueueEvent::Motion {
                pid: 8,
                epoch: 3,
                event: json!({"phase":"move","x":2}),
            },
            QueueEvent::Motion {
                pid: 8,
                epoch: 3,
                event: json!({"phase":"cancel"}),
            },
            QueueEvent::Motion {
                pid: 9,
                epoch: 5,
                event: json!({"phase":"move","x":3}),
            },
        ]);
        retain_close_barriers(&mut queue, 8, done);
        assert_eq!(queue.len(), 4);
        assert!(matches!(&queue[0],QueueEvent::Motion{pid:8,event,..} if event["phase"]=="drop"));
        assert!(matches!(&queue[1],QueueEvent::Motion{pid:8,event,..} if event["phase"]=="cancel"));
        assert!(matches!(&queue[2],QueueEvent::Motion{pid:9,event,..} if event["phase"]=="move"));
        assert!(matches!(
            &queue[3],
            QueueEvent::AppStateClose { pid: 8, .. }
        ));
    }

    #[test]
    fn app_close_cancels_only_remaining_moves_for_that_pid() {
        let mut gestures = BTreeMap::from([
            ((8, 2), json!({"phase":"move","target_output_id":"right"})),
            ((9, 3), json!({"phase":"move","target_output_id":"left"})),
        ]);
        let cancelled = take_cancelled_gestures(&mut gestures, 8);
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].2["phase"], "cancel");
        assert_eq!(cancelled[0].2["target_output_id"], "right");
        assert_eq!(gestures.len(), 1);
        // A previously dispatched drop removes its epoch and is therefore never canceled later.
        gestures.remove(&(9, 3));
        assert!(take_cancelled_gestures(&mut gestures, 9).is_empty());
    }

    #[test]
    fn app_lifecycle_ignores_a_stale_exit_after_a_new_app_opens() {
        let mut lifecycle = AppLifecycle::default();
        let (old, started, previous) = lifecycle.open(1001, true);
        assert!(started);
        assert_eq!(previous, None);
        let (new, started, previous) = lifecycle.open(1002, true);
        assert!(started);
        assert_eq!(previous, Some((1001, old)));
        assert_ne!(old, new);
        assert_eq!(lifecycle.close(1001, Some(old)), None);
        assert_eq!(lifecycle.active, Some((1002, new)));
        assert_eq!(lifecycle.close(1002, Some(new)), Some(new));
        let (reused, started, previous) = lifecycle.open(1002, false);
        assert!(started);
        assert_eq!(previous, None);
        assert_ne!(reused, new);
        assert_eq!(lifecycle.close(1002, Some(new)), None);
        assert_eq!(lifecycle.active, Some((1002, reused)));
    }
    #[test]
    fn floating_claim_is_once_per_active_app_generation() {
        let mut claim = WidgetFloatingClaim::default();
        assert!(claim.claim(Some((42, 8)), (42, 8)));
        assert!(
            !claim.claim(Some((42, 8)), (42, 8)),
            "a second output cannot act on the same app"
        );
        assert!(
            !claim.claim(Some((42, 9)), (42, 8)),
            "a stale generation cannot claim a new app"
        );
        claim.release((42, 7));
        assert!(
            !claim.claim(Some((42, 8)), (42, 8)),
            "stale close cannot release the current claim"
        );
        claim.release((42, 8));
        assert!(claim.claim(Some((42, 9)), (42, 9)));
    }
    #[test]
    fn pidfd_wait_observes_an_owned_short_lived_child_exit() {
        let mut child = std::process::Command::new("sleep")
            .arg("0.05")
            .spawn()
            .unwrap();
        assert!(wait_for_pid_exit(open_pidfd(child.id()).unwrap()));
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn output_dimensions_prefer_logical_geometry_and_rotate_raw_quarter_turns() {
        assert_eq!(
            logical_dimensions(
                &json!({"logicalWidth":800,"logicalHeight":1200,"width":3840,"height":2160,"scale":2,"transform":90})
            ),
            Some((800., 1200.))
        );
        assert_eq!(
            logical_dimensions(&json!({"width":3840,"height":2160,"scale":2,"transform":1})),
            Some((1080., 1920.))
        );
        assert_eq!(
            logical_dimensions(
                &json!({"width":3840,"height":2160,"scale":2,"transform":"Rotated270"})
            ),
            Some((1080., 1920.))
        );
    }
}
