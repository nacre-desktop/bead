// SPDX-License-Identifier: GPL-3.0-only
use bead::backends::Backend;
use bead::render::Scene;
use bead::wayland::{SceneSource, UiEvent};
use std::collections::BTreeMap;
use std::error::Error;

fn consume_pending_owner_dismiss(
    pending: &mut Option<(String, std::time::Instant)>,
    id: &str,
    now: std::time::Instant,
) -> bool {
    let Some((owner, dismissed_at)) = pending.take() else {
        return false;
    };
    owner == id
        && now.saturating_duration_since(dismissed_at) <= std::time::Duration::from_millis(250)
}

#[path = "reload/bar_daemon.rs"]
mod bar_daemon;
fn scope_recovery(session: &mut bead::ui::Session, output: Option<&str>) {
    if let Ok(path) = crate::state::File::path()
        && let Some(parent) = path.parent()
    {
        let mut key = session.recovery_key();
        if let Some(output) = output {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            output.hash(&mut hash);
            key = format!("{key}-{:016x}", hash.finish());
        }
        session.enable_recovery(parent.join("ui").join(key));
    }
}
fn panel_visibility_changes(open: bool, launcher: bool) -> BTreeMap<String, bead::core::Value> {
    let mut changes = BTreeMap::from([("panel.open".into(), bead::core::Value::Bool(open))]);
    if open && launcher {
        // Reopening starts at the results, never inside a form left open (B0199).
        changes.extend(crate::launcher_form::close());
    }
    changes
}
fn widget_drag_feedback_matches(
    app_pid: u32,
    pending_epoch: Option<u64>,
    pid: u32,
    epoch: u64,
) -> bool {
    pid == app_pid && pending_epoch == Some(epoch)
}
fn widget_history_feedback_matches(
    current_revision: u64,
    last_history_revision: u64,
    previous_revision: u64,
    revision: u64,
    current_document: Option<&serde_json::Value>,
    previous_document: &serde_json::Value,
    document: &serde_json::Value,
) -> bool {
    if revision <= last_history_revision
        || revision <= previous_revision
        || current_revision > revision
    {
        return false;
    }
    if current_revision < previous_revision {
        // Canonical model publication can lag a trusted native write's feedback.
        // The caller has already excluded a local draft or pending native edit.
        true
    } else if current_revision == previous_revision {
        current_document == Some(previous_document)
    } else {
        current_revision == revision && current_document == Some(document)
    }
}
#[derive(Clone)]
struct WidgetWriteFeedback {
    pid: u32,
    generation: u64,
    epoch: Option<u64>,
    previous_document: serde_json::Value,
    previous_revision: u64,
    document: serde_json::Value,
}
enum WidgetWriteResult {
    Pending(crate::nacre_source::CallTicket),
    Saved(u64),
}
fn widget_revision(values: &BTreeMap<String, bead::core::Value>) -> u64 {
    match values.get("widgets.rev") {
        Some(bead::core::Value::Number(value)) if value.is_finite() && *value >= 0. => {
            *value as u64
        }
        _ => 0,
    }
}
fn widget_document(
    values: &BTreeMap<String, bead::core::Value>,
    key: &str,
) -> Option<serde_json::Value> {
    match values.get(key) {
        Some(bead::core::Value::List(records)) => records.first().cloned(),
        _ => None,
    }
}
fn widget_seed_document(seed: &serde_json::Value) -> Option<serde_json::Value> {
    ["document", "initial"].into_iter().find_map(|key| {
        seed.get(key)
            .filter(|document| document.is_object())
            .cloned()
    })
}
fn widget_previous_document(
    values: &BTreeMap<String, bead::core::Value>,
    seed: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    widget_document(values, "widgets.layout")
        .or_else(|| widget_document(values, "widgets.drag.original"))
        .or_else(|| {
            (widget_revision(values) == 0)
                .then(|| widget_document(values, "widgets.initial"))
                .flatten()
        })
        .or_else(|| {
            (widget_revision(values) == 0)
                .then(|| seed.and_then(widget_seed_document))
                .flatten()
        })
}
fn widget_history_current_document(
    layout: Option<serde_json::Value>,
    initial: Option<serde_json::Value>,
    seed: Option<&serde_json::Value>,
    current_revision: u64,
    previous_revision: u64,
) -> Option<serde_json::Value> {
    layout.or_else(|| {
        if current_revision != 0 || current_revision != previous_revision {
            return None;
        }
        initial.or_else(|| seed.and_then(widget_seed_document))
    })
}
fn send_widget_write_feedback(
    feedback: WidgetWriteFeedback,
    success: bool,
    error: &str,
    revision: u64,
) {
    if success && revision > feedback.previous_revision {
        let _ = crate::widgets::widget_drag::send_app_history_feedback(
            feedback.pid,
            feedback.generation,
            feedback.epoch,
            feedback.previous_document,
            feedback.previous_revision,
            feedback.document,
            revision,
        );
    } else if let Some(epoch) = feedback.epoch {
        let message = if error.is_empty() {
            "A newer widget layout superseded this change."
        } else {
            error
        };
        let _ = crate::widgets::widget_drag::send_app_feedback(
            feedback.pid,
            feedback.generation,
            epoch,
            false,
            message,
        );
    }
}
#[derive(Clone, Copy)]
struct WidgetFloatingRequest {
    pid: u32,
    generation: u64,
    enabled: bool,
    attempted: bool,
}
#[derive(Clone, PartialEq)]
struct WidgetHoverPreviewKey {
    payload: bead::core::Value,
    source_rect: [u32; 4],
    initial_bounds: bead::render::Rect,
    output: String,
    revision: u64,
}

fn widget_drag_source_bounds(
    source_left: f64,
    source_top: f64,
    source_width: f64,
    source_height: f64,
    output_width: f64,
    output_height: f64,
) -> Option<bead::render::Rect> {
    if ![
        source_left,
        source_top,
        source_width,
        source_height,
        output_width,
        output_height,
    ]
    .into_iter()
    .all(f64::is_finite)
        || source_width <= 0.
        || source_height <= 0.
        || output_width <= 0.
        || output_height <= 0.
    {
        return None;
    }
    let clipped_left = source_left.max(0.);
    let clipped_top = source_top.max(0.);
    let clipped_right = (source_left + source_width).min(output_width);
    let clipped_bottom = (source_top + source_height).min(output_height);
    if clipped_right <= clipped_left || clipped_bottom <= clipped_top {
        return None;
    }
    let left = clipped_left.floor();
    let top = clipped_top.floor();
    let right = (left + (clipped_right - clipped_left).ceil() + 1.).min(output_width);
    let bottom = (top + (clipped_bottom - clipped_top).ceil() + 1.).min(output_height);
    (right > left && bottom > top).then_some(bead::render::Rect::new(
        left as f32,
        top as f32,
        (right - left) as f32,
        (bottom - top) as f32,
    ))
}

struct WidgetHoverPreview {
    key: WidgetHoverPreviewKey,
    prepared_at: std::time::Instant,
}
fn suppress_widget_drag_hover(
    suppressed: &mut bool,
    inside_drop_target: bool,
    pointer_event: bool,
) -> bool {
    if !*suppressed {
        return false;
    }
    if inside_drop_target || !pointer_event {
        true
    } else {
        *suppressed = false;
        false
    }
}
const WIDGET_HOVER_PREVIEW_RENEW: std::time::Duration = std::time::Duration::from_millis(1500);
const WIDGET_HOVER_PREVIEW_DEDUPE: std::time::Duration = std::time::Duration::from_millis(1400);
fn widget_floating_action(
    request: &mut Option<WidgetFloatingRequest>,
    active: Option<(u32, u64)>,
    backend: &bead::backends::Snapshot,
    compositor: &bead::core::source::Snapshot,
) -> Option<bead::backends::Action> {
    let policy = request.as_mut()?;
    if !policy.enabled
        || policy.attempted
        || active != Some((policy.pid, policy.generation))
        || !backend.connected
    {
        return None;
    }
    if !backend.capabilities.supports("floatingPlacement") {
        policy.attempted = true;
        return None;
    }
    if !compositor.connected || compositor.data["available"] == false {
        return None;
    }
    let windows = compositor.data["windows"].as_object()?;
    let mut owned = windows.iter().filter(|(_, window)| {
        window["pid"].as_u64() == Some(policy.pid as u64)
            && (window["appId"].as_str() == Some("nacre-widgets")
                || window["app_id"].as_str() == Some("nacre-widgets"))
    });
    let (native_key, raw_window) = owned.next()?;
    if owned.next().is_some() {
        return None;
    }
    let raw_id = raw_window["id"]
        .as_u64()
        .map(|id| id.to_string())
        .or_else(|| raw_window["address"].as_str().map(str::to_owned))
        .unwrap_or_else(|| native_key.to_owned());
    let mut candidates = backend
        .windows
        .iter()
        .filter(|window| window.app_id == "nacre-widgets");
    let window = candidates.next()?;
    if candidates.next().is_some() || window.id.0.rsplit(':').next() != Some(raw_id.as_str()) {
        return None;
    }
    policy.attempted = true;
    if window.floating {
        return None;
    }
    Some(bead::backends::Action::SetFloating {
        window: window.id.clone(),
        floating: true,
    })
}
#[cfg(test)]
fn apply_widget_floating(
    request: &mut Option<WidgetFloatingRequest>,
    active: Option<(u32, u64)>,
    backend: &bead::backends::Snapshot,
    compositor: &bead::core::source::Snapshot,
    mut action: impl FnMut(bead::backends::Action) -> Result<(), bead::backends::Error>,
) -> bool {
    let Some(action_to_apply) = widget_floating_action(request, active, backend, compositor) else {
        return false;
    };
    action(action_to_apply).is_ok()
}
fn widget_drag_targets_output(argument: &bead::core::Value, output: Option<&str>) -> bool {
    let bead::core::Value::List(records) = argument else {
        return false;
    };
    let Some(event) = records.first() else {
        return false;
    };
    if event["private_remote"] != true {
        return true;
    }
    let Some(target) = event["target_output_id"]
        .as_str()
        .filter(|name| !name.is_empty())
    else {
        return false;
    };
    output == Some(target)
}
const CLOCK_SECONDS: &str = "bead.desktop.clock.seconds";
/// B0193: whether a desktop canvas widget shows seconds. A Luau entry rebuilds on every changed
/// store, so the 1 Hz seconds key (alive whenever the bar shows seconds) would rebuild the
/// canvas once a second with no pixel change; the host sends it only to a canvas that uses it.
fn canvas_needs_seconds(layout: Option<&bead::core::Value>) -> bool {
    let Some(bead::core::Value::List(documents)) = layout else {
        return false;
    };
    documents
        .first()
        .and_then(|doc| doc["hosts"]["canvas"]["instances"].as_array())
        .is_some_and(|instances| {
            instances.iter().any(|item| {
                item["widget"] == "clock"
                    && matches!(item["style"].as_str(), Some("seconds" | "analog-seconds"))
            })
        })
}
/// Verbose-log aid: the keys that make a canvas projection differ from the last one sent.
/// Host store ids the bar publishes for widget surfaces; other views may not declare them.
fn is_bar_widget_context(key: &str) -> bool {
    key.starts_with("bar.")
        || key.starts_with("bead.desktop.")
        || matches!(
            key,
            "bead.workspaces"
                | "bead.windows"
                | "bead.focused_title"
                | "bead.viewport.width"
                | "bead.viewport.height"
        )
}
fn projection_diff(
    old: Option<&BTreeMap<String, bead::core::Value>>,
    new: &BTreeMap<String, bead::core::Value>,
) -> String {
    new.iter()
        .filter(|(key, value)| old.and_then(|old| old.get(*key)) != Some(*value))
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
/// Canvas store set by the host while the output is covered (B0193 `pause_on`).
/// Model key a luminance change sends; the handler swaps it for this output's luma store.
const WALLPAPER_LUMA_POKE: &str = "bead.desktop.wallpaper.rev";
const WIDGETS_COVERED: &str = "bead.desktop.covered";
fn widget_screen_values(
    snapshot: &bead::core::source::Snapshot,
) -> BTreeMap<String, bead::core::Value> {
    if !snapshot.connected
        || !snapshot.data["health"]["available"]
            .as_bool()
            .unwrap_or(true)
    {
        return BTreeMap::new();
    }
    let outputs = snapshot.data["outputs"].as_array();
    let output = outputs.and_then(|outputs| {
        outputs
            .iter()
            .find(|output| output["primary"] == true)
            .or_else(|| outputs.first())
    });
    let Some(output) = output else {
        return BTreeMap::new();
    };
    let Some((width, height)) =
        crate::widgets::widget_drag::logical_dimensions(output).filter(|(width, height)| {
            (1.0..=20_000.0).contains(width) && (1.0..=20_000.0).contains(height)
        })
    else {
        return BTreeMap::new();
    };
    let records=outputs.into_iter().flatten().filter_map(|row| {
        let (logical_width, logical_height)=crate::widgets::widget_drag::logical_dimensions(row)?;
        let raw_width=row["width"].as_f64().unwrap_or(logical_width);
        let raw_height=row["height"].as_f64().unwrap_or(logical_height);
        let mut record=serde_json::json!({"name":row["name"].as_str()? ,"x":row["x"].as_f64().unwrap_or(0.),
            "y":row["y"].as_f64().unwrap_or(0.),"width":raw_width,
            "height":raw_height,"scale":row["scale"].as_f64().unwrap_or(1.),
            "logical_width":logical_width,"logical_height":logical_height});
        if let Some(transform)=row.get("transform") {record["transform"]=transform.clone();}
        if let Some(logical)=row.get("logical") {record["logical"]=logical.clone();}
        Some(record)
    }).collect::<Vec<_>>();
    BTreeMap::from([
        (
            "widgets.screen_width".into(),
            bead::core::Value::Number(width),
        ),
        (
            "widgets.screen_height".into(),
            bead::core::Value::Number(height),
        ),
        (
            "widgets.screen_origin_x".into(),
            bead::core::Value::Number(output["x"].as_f64().unwrap_or(0.)),
        ),
        (
            "widgets.screen_origin_y".into(),
            bead::core::Value::Number(output["y"].as_f64().unwrap_or(0.)),
        ),
        (
            "widgets.screen_id".into(),
            bead::core::Value::Text(output["name"].as_str().unwrap_or_default().into()),
        ),
        ("widgets.outputs".into(), bead::core::Value::list(records)),
    ])
}
fn widget_app_origin_values(
    snapshot: &bead::core::source::Snapshot,
) -> BTreeMap<String, bead::core::Value> {
    let invalid = || {
        BTreeMap::from([
            (
                "widgets.drag.app_origin_valid".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "widgets.drag.app_origin_x".into(),
                bead::core::Value::Number(0.),
            ),
            (
                "widgets.drag.app_origin_y".into(),
                bead::core::Value::Number(0.),
            ),
            (
                "widgets.drag.app_width".into(),
                bead::core::Value::Number(0.),
            ),
            (
                "widgets.drag.app_height".into(),
                bead::core::Value::Number(0.),
            ),
            (
                "widgets.drag.app_output_id".into(),
                bead::core::Value::Text(String::new()),
            ),
        ])
    };
    if !snapshot.connected || snapshot.data["available"] == false {
        return invalid();
    }
    let windows = snapshot.data["windows"].as_object();
    let Some(window) = windows.and_then(|windows| {
        windows.values().find(|window| {
            window["pid"].as_u64() == Some(std::process::id() as u64)
                && (window["appId"].as_str() == Some("nacre-widgets")
                    || window["app_id"].as_str() == Some("nacre-widgets"))
        })
    }) else {
        return invalid();
    };
    let backend = snapshot.data["backend"]
        .as_str()
        .or_else(|| window["backend"].as_str())
        .unwrap_or("");
    let geometry_ready = match backend {
        "niri" => window["geometryKnown"] == true,
        "hyprland" => true,
        _ => window["geometryKnown"] == true,
    };
    if !geometry_ready {
        return invalid();
    }
    let geometry = &window["geometry"];
    let Some(x) = geometry["x"]
        .as_f64()
        .filter(|x| x.is_finite() && x.abs() <= 20_000.)
    else {
        return invalid();
    };
    let Some(y) = geometry["y"]
        .as_f64()
        .filter(|y| y.is_finite() && y.abs() <= 20_000.)
    else {
        return invalid();
    };
    let Some(width) = geometry["width"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0. && *v <= 20_000.)
    else {
        return invalid();
    };
    let Some(height) = geometry["height"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0. && *v <= 20_000.)
    else {
        return invalid();
    };
    BTreeMap::from([
        (
            "widgets.drag.app_origin_valid".into(),
            bead::core::Value::Bool(true),
        ),
        (
            "widgets.drag.app_origin_x".into(),
            bead::core::Value::Number(x),
        ),
        (
            "widgets.drag.app_origin_y".into(),
            bead::core::Value::Number(y),
        ),
        (
            "widgets.drag.app_width".into(),
            bead::core::Value::Number(width),
        ),
        (
            "widgets.drag.app_height".into(),
            bead::core::Value::Number(height),
        ),
        (
            "widgets.drag.app_output_id".into(),
            bead::core::Value::Text(
                window["outputId"]
                    .as_str()
                    .or_else(|| window["output"].as_str())
                    .unwrap_or_default()
                    .into(),
            ),
        ),
    ])
}
// Resolve a write against the latest canonical snapshot, so a delayed acknowledgement
// cannot replace a newer edit from another process.
fn widget_write_resolution(
    document: Option<bead::core::Value>,
    previous: Option<(bead::core::Value, bead::core::Value, u64)>,
    accepted_revision: Option<u64>,
    error: Option<String>,
    canonical: BTreeMap<String, bead::core::Value>,
    app_mode: bool,
) -> BTreeMap<String, bead::core::Value> {
    use bead::core::Value;
    let base_revision = previous.as_ref().map_or(0, |(_, _, revision)| *revision);
    let canonical_revision = match canonical.get("widgets.rev") {
        Some(Value::Number(revision)) => *revision as u64,
        _ => 0,
    };
    let mut changes = BTreeMap::from([
        ("widgets.draft".into(), Value::list(Vec::new())),
        (
            "widgets.error".into(),
            Value::Text(error.clone().unwrap_or_default()),
        ),
        (
            "widgets.message".into(),
            Value::Text(if error.is_none() {
                "Changes saved".into()
            } else {
                String::new()
            }),
        ),
    ]);
    if app_mode {
        changes.insert("widgets.busy".into(), Value::Bool(false));
    }
    let revision = accepted_revision.unwrap_or(base_revision.saturating_add(1));
    let newer = canonical.contains_key("widgets.layout")
        && canonical_revision
            > if error.is_some() {
                base_revision
            } else {
                revision
            };
    if newer {
        changes.extend(canonical);
        changes.insert("widgets.undo".into(), Value::list(Vec::new()));
    } else if error.is_none() {
        if let Some(document) = document {
            changes.insert("widgets.layout".into(), document);
            changes.insert("widgets.rev".into(), Value::Number(revision as f64));
        }
    } else if let Some((layout, undo, revision)) = previous {
        changes.insert("widgets.layout".into(), layout);
        changes.insert("widgets.undo".into(), undo);
        changes.insert("widgets.rev".into(), Value::Number(revision as f64));
    }
    changes
}
pub struct Runtime {
    pub session: bead::ui::Session,
    output: Option<String>,
    pending: Option<Scene>,
    chrome: Option<crate::placement::Chrome>,
    /// B0048: the pearl chrome while docked, the docked chrome while detached.
    spare_chrome: Option<crate::placement::Chrome>,
    detached: bool,
    launcher: Option<std::rc::Rc<crate::launcher::Menu>>,
    launcher_initial_projection: std::rc::Rc<std::cell::Cell<bool>>,
    launcher_subscription: Option<bead::core::source::Subscription>,
    launcher_quick: Option<std::rc::Rc<crate::launcher_quick::Quick>>,
    launcher_quick_subscription: Vec<bead::core::source::Subscription>,
    calendar: Option<std::rc::Rc<crate::calendar::Calendar>>,
    calendar_subscription: Option<bead::core::source::Subscription>,
    i18n_subscription: Option<bead::ui::i18n::Subscription>,
    status: Option<std::rc::Rc<crate::status_views::Status>>,
    dock_status: BTreeMap<String, std::rc::Rc<crate::status_views::Status>>,
    dock_subscriptions: Vec<bead::core::source::Subscription>,
    status_subscription: Option<bead::core::source::Subscription>,
    status_control_subscription: Option<bead::core::source::Subscription>,
    extras: Option<std::rc::Rc<crate::extras::Extras>>,
    prompt: Option<std::rc::Rc<crate::prompt::Prompt>>,
    extras_subscription: Vec<bead::core::source::Subscription>,
    extras_wake: Option<std::sync::Arc<std::sync::Mutex<Option<crate::nacre_source::CallWake>>>>,
    session_menu: Option<std::rc::Rc<crate::session_menu::SessionMenu>>,
    session_menu_subscription: Option<bead::core::source::Subscription>,
    chat: Option<std::rc::Rc<crate::chat::Chat>>,
    chat_subscription: Vec<bead::core::source::Subscription>,
    settings: Option<std::rc::Rc<crate::settings::Settings>>,
    colors: Option<std::rc::Rc<crate::colors::Colors>>,
    welcome: Option<std::rc::Rc<crate::welcome::Welcome>>,
    control: Option<std::rc::Rc<crate::control_center::ControlCenter>>,
    control_subscription: Vec<bead::core::source::Subscription>,
    dashboard: Option<std::rc::Rc<std::cell::RefCell<crate::dashboard::Dashboard>>>,
    dashboard_factory: Option<crate::dashboard::Factory>,
    pending_dashboard: Option<PendingDashboard>,
    dashboard_pearl_animation: Option<DashboardPearlAnimation>,
    dashboard_pearl_deadline: Option<std::time::Instant>,
    /// Scalar view animations wait for the panel's independent compositor slide.
    panel_animation_hold_until: Option<std::time::Instant>,
    pending_panel_animation_duration: Option<std::time::Duration>,
    pending_panel_navigation: Option<String>,
    dashboard_subscription: Vec<bead::core::source::Subscription>,
    slider_sent: Option<std::time::Instant>,
    slider_pending: Option<UiEvent>,
    slider_release_serial: u64,
    slider_local_active: bool,
    launcher_memory: Option<crate::launcher::Memory>,
    icon_warmer: Option<crate::icon_warm::IconWarmer>,
    wallpaper_warmer: Option<std::sync::Arc<crate::wallpaper_warm::Warmer>>,
    connection_status: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    connection_subscription: Option<bead::core::source::Subscription>,
    prefs: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    workspaces: crate::workspace_rows::Rows,
    prefs_subscription: Option<bead::core::source::Subscription>,
    launcher_trigger: bool,
    last_launch: Option<crate::key_routes::Launch>,
    brightness: Option<crate::key_routes::Brightness>,
    routes: Option<std::rc::Rc<crate::bar_routes::Routes>>,
    routes_subscription: Option<bead::core::source::Subscription>,
    tray_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    tray_topic_subscription: Option<bead::core::source::Subscription>,
    daemon_tray: std::sync::Arc<std::sync::Mutex<Vec<bead::services::TrayItem>>>,
    /// N0040: the niri `compositor` topic, read only for workspace origins (visitor groups).
    updates_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    updates_subscription: Option<bead::core::source::Subscription>,
    /// B0199: the daemon's `controls` topic (recorder state) and the client that stops a recording.
    recording_topic: Option<(
        std::sync::Arc<dyn bead::core::source::Source>,
        crate::nacre_source::Client,
    )>,
    recording_subscription: Option<bead::core::source::Subscription>,
    bar_daemon: Option<std::sync::Arc<bar_daemon::BarDaemon>>,
    bar_daemon_subscriptions: Vec<bead::core::source::Subscription>,
    widgets_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    widgets_subscription: Option<bead::core::source::Subscription>,
    widgets_file: Option<std::path::PathBuf>,
    widgets_watch: Option<notify::RecommendedWatcher>,
    widgets_canvas_layout: Option<bead::core::Value>,
    widgets_canvas_editing: bool,
    widgets_canvas_wanted: bool,
    widget_canvas_projection: Option<BTreeMap<String, bead::core::Value>>,
    /// B0193 `pause_on`: the canvas of this output was told it is covered (fullscreen/lock).
    widgets_covered: bool,
    wallpaper_luma_watch: Option<crate::wallpaper_luma::Watch>,
    widgets_client: Option<crate::nacre_source::Client>,
    widget_editor: bool,
    widgets_close_on_commit: bool,
    widget_canvas: bool,
    /// The passive drag overlay paints in output coordinates like the widget canvas (B0198).
    widget_drag_overlay: bool,
    widgets_projection: Option<BTreeMap<String, bead::core::Value>>,
    widgets_write: Option<crate::nacre_source::CallTicket>,
    widgets_write_document: Option<bead::core::Value>,
    widget_app: bool,
    widgets_write_previous: Option<(bead::core::Value, bead::core::Value, u64)>,
    widget_write_feedback: Option<WidgetWriteFeedback>,
    widget_drag_pending_epoch: Option<u64>,
    widget_drag_latest_epoch: Option<u64>,
    widgets_drag_write: bool,
    widget_drag_controller: Option<crate::widgets::widget_drag::Controller>,
    widget_drag_controller_primed: bool,
    widget_drag_subscription: Option<crate::widgets::widget_drag::Subscription>,
    widget_drag_app_announced: bool,
    widget_active_app: Option<(u32, u64)>,
    widget_floating_request: Option<WidgetFloatingRequest>,
    widget_drag_model: Option<BTreeMap<String, bead::core::Value>>,
    widget_drag_wanted: bool,
    widget_drag_output: Option<String>,
    widget_drag_projection: Option<BTreeMap<String, bead::core::Value>>,
    widget_hover_preview: Option<WidgetHoverPreview>,
    widget_drag_suppress_hover_until_leave: bool,
    widget_drag_suppress_hover_bounds: Option<(f32, f32, f32, f32)>,
    widget_drag_epoch: u64,
    widget_drag_retire_cleanup: bool,
    widget_drag_action_registered: bool,
    widget_displays_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    widget_displays_subscription: Option<bead::core::source::Subscription>,
    widget_compositor_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    widget_compositor_subscription: Option<bead::core::source::Subscription>,
    widget_compositor_policy_only: bool,
    pointer_at: (f32, f32),
    origins_topic: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    origins_subscription: Option<bead::core::source::Subscription>,
    backend: Option<std::sync::Arc<crate::backends::Native>>,
    subscription: Option<bead::backends::Subscription>,
    last_focused_window: Option<bead::backends::Id>,
    desktop: Option<std::sync::Arc<bead::services::Desktop>>,
    /// Held only while the committed view actually renders a seconds-resolution clock form.
    clock_seconds_lease: Option<bead::services::ClockSecondsLease>,
    desktop_subscription: Option<bead::services::Subscription>,
    desktop_revision: u64,
    notification_hover: Option<String>,
    notification_primary: bool,
    fonts: crate::font_sources::Selector,
    trace: bool,
    notifications: Option<std::sync::Arc<dyn crate::notifications::Notices>>,
    notification_subscription: Option<crate::notifications::Handle>,
    /// Bar sessions of the main shell: drive the transient `notices`/`osd` views.
    notice_host: Option<std::sync::Arc<dyn crate::notifications::Notices>>,
    notice_drive: Option<crate::notifications::Handle>,
    menu_item: String,
    tray_anchor: Option<f32>,
    tray_close: bool,
    tray_requested_close: bool,
    tray_enabled: bool,
    tray_hover: String,
    tray_selection: Option<crate::tray::Selection>,
    status_pill: bead::ui::status_pill::StatusPill,
    status_pill_epoch: u64,
    status_pill_scheduled: Option<std::time::Instant>,
    status_pill_route: Option<String>,
    status_owner_dismissed: Option<(String, std::time::Instant)>,
    status_route_center: Option<f32>,
    status_route_anchor: Option<bead::wayland::PanelAnchor>,
    status_route_id: Option<String>,
    viewport_size: (u32, u32),
    popup_anchor: Option<bead::wayland::PanelAnchor>,
    popup_anchor_changed: bool,
    host_release_pending: bool,
    tray_menu: Option<crate::tray::Menu>,
    pending_focus: Option<String>,
    bar: bool,
    state_file: Option<crate::state::File>,
    actions: bead::core::capabilities::Registry,
    wakeup: Option<std::sync::Arc<dyn Fn(UiEvent) + Send + Sync>>,
    smooth_open_started: Option<std::time::Instant>,
    smooth_key_starts: std::collections::VecDeque<(std::time::Instant, &'static str)>,
    smooth_scroll_started: Option<std::time::Instant>,
    smooth_scroll_last: Option<std::time::Instant>,
    smooth_scroll_dropped: u64,
    smooth_scroll_reported: Option<std::time::Instant>,
    smooth_animation_dropped: Option<u64>,
    smooth_last_presented: Option<std::time::Instant>,
    smooth_frame_interval: std::time::Duration,
    smooth_started_at: std::time::Instant,
    smooth_user_interacted: bool,
    smooth_bar_startup_animation: bool,
    #[cfg(test)]
    reload_events: usize,
}
#[derive(Clone)]
struct DashboardPearlAnimation {
    from: f64,
    to: f64,
    started: std::time::Instant,
    duration: std::time::Duration,
    easing: String,
}
struct PendingDashboard {
    view: crate::dashboard::Dashboard,
    subscriptions: Vec<bead::core::source::Subscription>,
    animation: Option<DashboardPearlAnimation>,
    deadline: Option<std::time::Instant>,
}

fn animation_wakeup_after_panel_hold(
    now: std::time::Instant,
    hold_until: Option<std::time::Instant>,
    animation: Option<std::time::Instant>,
) -> Option<std::time::Instant> {
    hold_until.filter(|until| *until > now).or(animation)
}

#[cfg(test)]
mod panel_animation_wakeup_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn forty_slide_frames_do_not_wake_the_script_stage_for_scalar_animation() {
        let start = std::time::Instant::now();
        let hold_until = start + Duration::from_millis(336);
        let scalar_deadline = start + Duration::from_millis(8);
        let mut stage_wakeups = 0;
        for frame in 0..40 {
            let now = start + Duration::from_millis(frame * 8);
            let next =
                animation_wakeup_after_panel_hold(now, Some(hold_until), Some(scalar_deadline));
            assert_eq!(next, Some(hold_until));
            stage_wakeups += usize::from(next.is_some_and(|deadline| deadline <= now));
        }
        assert_eq!(stage_wakeups, 0);
        assert_eq!(
            animation_wakeup_after_panel_hold(hold_until, Some(hold_until), Some(scalar_deadline),),
            Some(scalar_deadline),
            "one catch-up update may run after the independent slide completes"
        );
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.widget_app && self.widget_drag_app_announced {
            let delivered =
                crate::widgets::widget_drag::app_state_close(std::time::Duration::from_millis(145));
            if !delivered {
                eprintln!("WIDGET_DRAG_CLOSE_TIMEOUT");
            }
        }
        // Keep only the bounded scalar search/navigation state after idle close;
        // rows and engine tickets remain owned by the dropped Menu projection.
        if self.launcher.is_some()
            && let Some(memory) = &self.launcher_memory
            && let Ok(mut saved) = memory.lock()
        {
            *saved = crate::launcher::remember(&self.session.stores);
            if self.trace {
                println!(
                    "PANEL_VIEW_STATE_RELEASED view=launcher entries={}",
                    saved.len()
                );
            }
        }
    }
}
impl Runtime {
    fn smooth_surface(&self) -> String {
        if self
            .extras
            .as_ref()
            .is_some_and(|extras| extras.kind == crate::extras::Kind::Wallpaper)
        {
            "picker".into()
        } else if self.launcher.is_some() {
            "launcher".into()
        } else if self.settings.is_some() {
            "settings".into()
        } else if self.dashboard.is_some() || self.pending_dashboard.is_some() {
            "dashboard".into()
        } else if self.control.is_some() {
            "control".into()
        } else if self.chat.is_some() {
            "ai-chat".into()
        } else if self.bar {
            "bar".into()
        } else {
            panel_route_name(&self.session.stores)
        }
    }

    /// Compile and decorate the first app frame before its window is mapped.
    pub(crate) fn warm_frame(&mut self) -> Result<Scene, Box<dyn Error>> {
        let scene = self.session.reload()?;
        let Some(scene) = self.finish_scene(Some(scene))? else {
            return Err("Widgets app did not produce an initial scene".into());
        };
        self.pending = None;
        self.session.commit(scene.clone());
        Ok(scene)
    }
    fn announce_widget_app(&mut self) {
        if self.widget_app && !self.widget_drag_app_announced {
            let floating =
                self.session.stores.get("window.floating") == Some(&bead::core::Value::Bool(true));
            crate::widgets::widget_drag::app_state(true, floating);
            self.widget_drag_app_announced = true;
        }
    }
    fn widget_seed(&self) -> Option<serde_json::Value> {
        let nodes = self.session.view_nodes();
        let mut seed = nodes.iter().find_map(|node| match &node.metadata {
            Some(bead::core::Value::List(records)) => records
                .first()
                .filter(|record| record["widget_layout"] == true)
                .cloned(),
            _ => None,
        })?;
        let geometry = nodes
            .iter()
            .filter_map(|node| {
                let Some(bead::core::Value::List(records)) = &node.metadata else {
                    return None;
                };
                let marker = records
                    .iter()
                    .find(|record| record["widget_instance"] == true)?;
                let iid = marker["iid"].as_str()?;
                let rect = bead::render::tree::rect(&node.draw);
                Some(serde_json::json!({"iid":iid,"x":rect.x,"y":rect.y,"w":rect.w,"h":rect.h}))
            })
            .collect::<Vec<_>>();
        if !geometry.is_empty() {
            seed["geometry"] = serde_json::json!(geometry);
        }
        Some(seed)
    }
    fn widget_argument(&self, name: &str, argument: bead::core::Value) -> bead::core::Value {
        let argument = if name == "widgets.drag" {
            crate::widgets::widget_drag::prepare_event(
                &argument,
                &self.session.stores,
                self.widget_app,
                self.output.as_deref().unwrap_or(""),
            )
        } else {
            argument
        };
        let bead::core::Value::List(records) = &argument else {
            return argument;
        };
        let mut records = (**records).clone();
        if let Some(seed) = self
            .widget_seed()
            .and_then(|seed| seed.as_object().cloned())
        {
            if name == "widgets.drag" {
                if let Some(event) = records
                    .first_mut()
                    .and_then(serde_json::Value::as_object_mut)
                {
                    let authoritative = event
                        .get("private_remote")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true);
                    let target_output = event
                        .get("target_output_id")
                        .and_then(serde_json::Value::as_str)
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                        .or_else(|| self.output.clone())
                        .unwrap_or_default();
                    let context = event
                        .entry("context")
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(context) = context.as_object_mut() {
                        for (key, value) in &seed {
                            if authoritative {
                                context.insert(key.clone(), value.clone());
                            } else {
                                context.entry(key.clone()).or_insert_with(|| value.clone());
                            }
                        }
                        // A desktop widget dropped on the bar is placed among the bar's rects.
                        if self.widget_canvas {
                            let rects = crate::widgets::widget_drag::bar_geometry(&target_output);
                            if let Some(list) = context
                                .entry("geometry")
                                .or_insert_with(|| serde_json::json!([]))
                                .as_array_mut()
                            {
                                list.extend(rects);
                            }
                        }
                    }
                    if let Some(target) = event
                        .get_mut("target")
                        .and_then(serde_json::Value::as_array_mut)
                        .and_then(|targets| targets.first_mut())
                        .and_then(serde_json::Value::as_object_mut)
                    {
                        for (key, value) in &seed {
                            target.entry(key.clone()).or_insert_with(|| value.clone());
                        }
                    }
                }
            } else if let Some(target) = records
                .first_mut()
                .and_then(serde_json::Value::as_object_mut)
            {
                for (key, value) in &seed {
                    target.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        bead::core::Value::list(records)
    }
    fn widget_app_state(&mut self, pid: u32, generation: u64, open: bool, floating: bool) -> bool {
        if open {
            if self
                .widget_active_app
                .is_some_and(|(active_pid, active_generation)| {
                    generation < active_generation
                        || (generation == active_generation && pid != active_pid)
                })
            {
                return false;
            }
            let same = self.widget_floating_request.is_some_and(|request| {
                request.pid == pid && request.generation == generation && request.enabled
            });
            self.widget_active_app = Some((pid, generation));
            if floating {
                if !same {
                    self.widget_floating_request = Some(WidgetFloatingRequest {
                        pid,
                        generation,
                        enabled: true,
                        attempted: false,
                    });
                }
            } else {
                self.widget_floating_request = None;
                crate::widgets::widget_drag::release_app_floating(pid, generation);
                self.release_widget_compositor_policy_topic();
            }
        } else if self.widget_active_app == Some((pid, generation)) {
            self.widget_active_app = None;
            self.widget_floating_request = None;
            crate::widgets::widget_drag::release_app_floating(pid, generation);
            self.release_widget_compositor_policy_topic();
        } else {
            return false;
        }
        if open
            && floating
            && self.widget_app
            && crate::widgets::widget_drag::active_app_identity() == Some((pid, generation))
            && self.backend.is_none()
            && let Ok(backend) = crate::backends::Native::from_env()
            && let Err(error) = self.attach_backend(backend)
        {
            eprintln!("WIDGET_FLOAT_BACKEND_ERROR {error}");
        }
        if open && floating && self.widget_floating_request.is_some() && self.backend.is_some() {
            self.ensure_widget_compositor_subscription();
        }
        true
    }
    fn release_widget_compositor_policy_topic(&mut self) {
        self.widget_compositor_subscription = None;
        if self.widget_compositor_policy_only {
            self.widget_compositor_topic = None;
            self.widget_compositor_policy_only = false;
        }
    }
    fn ensure_widget_compositor_subscription(&mut self) {
        if self.widget_compositor_topic.is_none()
            && let Some(client) = &self.widgets_client
            && let Ok(topic) = client.view_topic("compositor")
        {
            self.widget_compositor_topic = Some(topic);
            self.widget_compositor_policy_only = true;
        }
        if self.widget_compositor_subscription.is_none()
            && let (Some(source), Some(wakeup)) =
                (&self.widget_compositor_topic, self.wakeup.clone())
        {
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                wakeup(UiEvent::Model(widget_app_origin_values(&snapshot)));
            })) {
                Ok(subscription) => self.widget_compositor_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=widgets_policy_compositor_subscription"),
            }
        }
    }
    fn try_float_widgets_window(&mut self, snapshot: &bead::backends::Snapshot) {
        if self.widget_floating_request.is_some() {
            self.ensure_widget_compositor_subscription();
        }
        let (Some(backend), Some(compositor)) =
            (self.backend.clone(), self.widget_compositor_topic.as_ref())
        else {
            return;
        };
        let compositor = compositor.snapshot();
        let active = crate::widgets::widget_drag::active_app_identity();
        let Some(action) = widget_floating_action(
            &mut self.widget_floating_request,
            active,
            snapshot,
            &compositor,
        ) else {
            return;
        };
        let Some(request) = self.widget_floating_request else {
            return;
        };
        if !crate::widgets::widget_drag::claim_active_app_floating(request.pid, request.generation)
        {
            return;
        }
        let _ = backend.action(action);
    }
    fn widget_host_values(&self) -> BTreeMap<String, bead::core::Value> {
        self.session
            .stores
            .values()
            .iter()
            .filter(|(key, _)| key.starts_with("widgets.") || is_bar_widget_context(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
    /// Common display data shared by the desktop canvas and the passive drag preview.
    fn widget_surface_values(&self, include_status: bool) -> BTreeMap<String, bead::core::Value> {
        let public_bar_keys = [
            "bar.clock.text",
            "bar.clock.twelve",
            "bar.dashboard.enabled",
            "bar.height",
            "bar.launcher",
            "bar.notifications.count",
            "bar.notifications.text",
            "bar.popouts",
            "bar.reconnecting",
            "bar.title",
            "bar.title.cue.body",
            "bar.title.cue.border",
            "bar.title.cue.fade",
            "bar.title.cue.halo",
            "bar.title.cue.visible",
            "bar.tray.enabled",
            "bar.tray.visible",
            "bar.updates.count",
            "bar.updates.enabled",
            "bar.updates.text",
            "bar.window.focus.enabled",
            "bar.workspace.count",
            "bar.workspace.enabled",
            "bar.workspace.icons",
            "bar.workspace.hints",
            "bar.workspace.origins",
        ];
        let public_context_keys = ["bead.edge.dwell", "bead.edge.fade"];
        let public_desktop_keys = [
            "bead.desktop.audio.percent",
            "bead.desktop.audio.device",
            "bead.desktop.audio.label",
            "bead.desktop.titlevisible",
            "bead.desktop.clock",
            "bead.desktop.clock.seconds",
            "bead.desktop.clock.date",
            "bead.desktop.audio",
            "bead.desktop.audio.available",
            "bead.desktop.battery",
            "bead.desktop.battery.value",
            "bead.desktop.battery.value.width",
            "bead.desktop.network",
            "bead.desktop.bluetooth",
            "bead.desktop.tray",
            "bead.desktop.menu",
            "bead.desktop.tray.tooltip",
            "bead.desktop.tray.tooltip.visible",
            crate::wallpaper_luma::STORE,
        ];
        self.session
            .stores
            .values()
            .iter()
            .filter(|(key, _)| {
                public_bar_keys.contains(&key.as_str())
                    || public_desktop_keys.contains(&key.as_str())
                    || public_context_keys.contains(&key.as_str())
                    || (include_status
                        && matches!(
                            key.as_str(),
                            "status.hovered"
                                | "status.peek.visible"
                                | "status.peek.kind"
                                | "status.panel.kind"
                        ))
                    || matches!(
                        key.as_str(),
                        "bead.workspaces"
                            | "bead.windows"
                            | "bead.focused_title"
                            | "bead.viewport.width"
                            | "bead.viewport.height"
                    )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
    /// Values that the desktop canvas is allowed to receive from its host.
    /// Keep this projection aligned with the canvas behavior stores; controller
    /// bookkeeping never crosses the view boundary.
    fn widget_canvas_values(&self) -> BTreeMap<String, bead::core::Value> {
        let public_widget_keys = [
            "widgets.layout",
            "widgets.rev",
            "widgets.draft",
            "widgets.undo",
            "widgets.error",
            "widgets.message",
            "widgets.preview",
            "widgets.geometry",
            "widgets.editing",
            "widgets.selected",
            "widgets.canvas.width",
            "widgets.canvas.height",
            "widgets.drag.active",
            "widgets.drag.source",
            "widgets.drag.phase",
            "widgets.drag.layout",
            "widgets.drag.destination",
            "widgets.drag.x",
            "widgets.drag.y",
            "widgets.drag.width",
            "widgets.drag.height",
            "widgets.drag.grab_x",
            "widgets.drag.grab_y",
            "widgets.drag.snap_x",
            "widgets.drag.snap_y",
            "widgets.drag.source_origin_valid",
            "widgets.drag.app_origin_valid",
            "widgets.drag.target_output_id",
            "widgets.app.open",
            "widgets.screen_width",
            "widgets.screen_height",
            "widgets.screen_origin_x",
            "widgets.screen_origin_y",
            "widgets.screen_id",
            "widgets.outputs",
            // Desktop widgets consume the same published service rows and action
            // availability as the bar; keep these at the adapter edge.
            "launcher.apps",
            "bluetooth.devices",
            "bluetooth.power_checked",
            "network.rows",
            "network.wifi_enabled",
            "notifications.rows",
            "status.updates_checked",
            "status.updates_can_open",
            "status.updates_can_refresh",
            "session.enabled",
            "session.busy",
            "session.confirm",
            "session.confirm_action",
            "session.message",
        ];
        let mut values = self.widget_surface_values(false);
        values.extend(
            self.session
                .stores
                .values()
                .iter()
                .filter(|(key, _)| public_widget_keys.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        values
    }
    /// The drag ghost has its own narrower behavior contract than the canvas.
    fn widget_drag_preview_values(&self) -> BTreeMap<String, bead::core::Value> {
        let public_widget_keys = [
            "widgets.layout",
            "widgets.draft",
            "widgets.preview",
            "widgets.editing",
            "widgets.drag.layout",
            "widgets.drag.base",
            "widgets.drag.original",
            "widgets.drag.active",
            "widgets.drag.source",
            "widgets.drag.destination",
            "widgets.drag.phase",
            "widgets.drag.epoch",
            "widgets.drag.snap_x",
            "widgets.drag.snap_y",
            "widgets.drag.x",
            "widgets.drag.y",
            "widgets.drag.start_x",
            "widgets.drag.start_y",
            "widgets.drag.width",
            "widgets.drag.height",
            "widgets.drag.grab_x",
            "widgets.drag.grab_y",
            "widgets.drag.source_origin_x",
            "widgets.drag.source_origin_y",
            "widgets.drag.source_origin_valid",
            "widgets.drag.target_origin_x",
            "widgets.drag.target_origin_y",
            "widgets.drag.app_origin_x",
            "widgets.drag.app_origin_y",
            "widgets.drag.app_origin_valid",
            "widgets.drag.app_width",
            "widgets.drag.app_height",
            "widgets.drag.target_output_id",
        ];
        let mut values = self.widget_surface_values(false);
        values.extend(
            self.session
                .stores
                .values()
                .iter()
                .filter(|(key, _)| public_widget_keys.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some(model) = &self.widget_drag_model {
            values.extend(
                model
                    .iter()
                    .filter(|(key, _)| {
                        key.starts_with("widgets.drag.")
                            && public_widget_keys.contains(&key.as_str())
                    })
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        values
    }
    fn widget_drag_projection(
        &self,
        model: &BTreeMap<String, bead::core::Value>,
    ) -> BTreeMap<String, bead::core::Value> {
        let mut projection = self.widget_drag_preview_values();
        for (key, value) in model {
            if key.starts_with("widgets.drag.") && projection.contains_key(key.as_str()) {
                projection.insert(key.clone(), value.clone());
            }
        }
        projection
    }
    fn publish_widget_drag_projection(&mut self, drag_values: BTreeMap<String, bead::core::Value>) {
        let active = drag_values.get("widgets.drag.active") == Some(&bead::core::Value::Bool(true));
        if active {
            self.widget_hover_preview = None;
        }
        let output = drag_values
            .get("widgets.drag.target_output_id")
            .and_then(|value| {
                if let bead::core::Value::Text(name) = value {
                    (!name.is_empty()).then(|| name.clone())
                } else {
                    None
                }
            })
            .or_else(|| self.output.clone());
        if let Some(wakeup) = &self.wakeup {
            if self.widget_drag_wanted && (!active || self.widget_drag_output != output) {
                wakeup(UiEvent::CanvasOffer {
                    view: "widgets-drag".into(),
                    output: self.widget_drag_output.clone(),
                    open: false,
                    values: drag_values.clone(),
                });
                self.widget_drag_wanted = false;
                self.widget_drag_output = None;
                self.widget_drag_projection = None;
            }
            if active && (!self.widget_drag_wanted || self.widget_drag_output != output) {
                wakeup(UiEvent::CanvasOffer {
                    view: "widgets-drag".into(),
                    output: output.clone(),
                    open: true,
                    values: drag_values.clone(),
                });
                self.widget_drag_wanted = true;
                self.widget_drag_output = output.clone();
                self.widget_drag_projection = Some(drag_values.clone());
            }
            if self.widget_drag_wanted && self.widget_drag_projection.as_ref() != Some(&drag_values)
            {
                self.widget_drag_projection = Some(drag_values.clone());
                wakeup(UiEvent::CanvasModel {
                    view: "widgets-drag".into(),
                    output: self.widget_drag_output.clone(),
                    values: drag_values,
                });
            }
        }
    }
    fn widget_drag_preview(
        &self,
        x: f32,
        y: f32,
        candidate: bead::ui::TransferCandidate,
    ) -> Option<(WidgetHoverPreviewKey, BTreeMap<String, bead::core::Value>)> {
        if self.widget_app || !(self.bar || self.widget_canvas) {
            return None;
        }
        if ![
            candidate.origin.0,
            candidate.origin.1,
            candidate.source_rect.0,
            candidate.source_rect.1,
            candidate.source_rect.2,
            candidate.source_rect.3,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return None;
        }
        let bead::core::Value::List(items) = &candidate.payload else {
            return None;
        };
        let item = items.first()?;
        let host = item["host"].as_str().unwrap_or_default().to_owned();
        let source_form = item["form"].as_str().unwrap_or_default().to_owned();
        let bar_source = self.bar && matches!(host.as_str(), "top_bar" | "status-pill");
        let canvas_source = self.widget_canvas && host == "canvas";
        if item["widget"].as_str().is_none()
            || (!bar_source && !canvas_source)
            || !matches!(source_form.as_str(), "bar" | "desktop")
        {
            return None;
        };
        let number = |key: &str| match self.session.stores.get(key) {
            Some(bead::core::Value::Number(value)) if value.is_finite() => *value,
            _ => 0.,
        };
        let (surface_x, surface_y) = if canvas_source {
            (
                number("widgets.screen_origin_x"),
                number("widgets.screen_origin_y"),
            )
        } else {
            (number("bead.surface.x"), number("bead.surface.y"))
        };
        let pointer_x = f64::from(candidate.origin.0) + surface_x;
        let pointer_y = f64::from(candidate.origin.1) + surface_y;
        let source_left = f64::from(candidate.source_rect.0) + surface_x;
        let source_top = f64::from(candidate.source_rect.1) + surface_y;
        let output =
            self.output
                .clone()
                .or_else(|| match self.session.stores.get("bead.output.name") {
                    Some(bead::core::Value::Text(name)) if !name.is_empty() => Some(name.clone()),
                    _ => None,
                })?;
        let target_origin_x = number("widgets.screen_origin_x");
        let target_origin_y = number("widgets.screen_origin_y");
        let number_or = |key: &str, fallback: f64| match self.session.stores.get(key) {
            Some(bead::core::Value::Number(value)) if value.is_finite() && *value > 0. => *value,
            _ => fallback,
        };
        let output_width = number_or("widgets.screen_width", f64::from(self.viewport_size.0));
        let output_height = number_or("widgets.screen_height", f64::from(self.viewport_size.1));
        let initial_bounds = widget_drag_source_bounds(
            source_left - target_origin_x,
            source_top - target_origin_y,
            f64::from(candidate.source_rect.2.max(1.)),
            f64::from(candidate.source_rect.3.max(1.)),
            output_width,
            output_height,
        )?;
        let key = WidgetHoverPreviewKey {
            payload: candidate.payload.clone(),
            source_rect: [
                candidate.source_rect.0.to_bits(),
                candidate.source_rect.1.to_bits(),
                candidate.source_rect.2.to_bits(),
                candidate.source_rect.3.to_bits(),
            ],
            initial_bounds,
            output: output.clone(),
            revision: match self.session.stores.get("widgets.rev") {
                Some(bead::core::Value::Number(revision))
                    if revision.is_finite() && *revision >= 0. =>
                {
                    *revision as u64
                }
                _ => 0,
            },
        };
        let mut values = self.widget_drag_preview_values();
        let mut set = |key: &str, value| values.insert(key.into(), value);
        set("bead.output.name", bead::core::Value::Text(output.clone()));
        set("widgets.drag.active", bead::core::Value::Bool(true));
        set("widgets.drag.source", candidate.payload);
        set(
            "widgets.drag.phase",
            bead::core::Value::Text("prepare".into()),
        );
        set(
            "widgets.drag.destination",
            bead::core::Value::Text(
                if source_form == "desktop" {
                    "bar"
                } else {
                    "desktop"
                }
                .into(),
            ),
        );
        set("widgets.drag.x", bead::core::Value::Number(pointer_x));
        set("widgets.drag.y", bead::core::Value::Number(pointer_y));
        set("widgets.drag.start_x", bead::core::Value::Number(pointer_x));
        set("widgets.drag.start_y", bead::core::Value::Number(pointer_y));
        set(
            "widgets.drag.width",
            bead::core::Value::Number(f64::from(candidate.source_rect.2.max(1.))),
        );
        set(
            "widgets.drag.height",
            bead::core::Value::Number(f64::from(candidate.source_rect.3.max(1.))),
        );
        set(
            "widgets.drag.grab_x",
            bead::core::Value::Number(f64::from(x - candidate.source_rect.0)),
        );
        set(
            "widgets.drag.grab_y",
            bead::core::Value::Number(f64::from(y - candidate.source_rect.1)),
        );
        set(
            "widgets.drag.snap_x",
            bead::core::Value::Number(source_left),
        );
        set("widgets.drag.snap_y", bead::core::Value::Number(source_top));
        set(
            "widgets.drag.source_origin_x",
            bead::core::Value::Number(surface_x),
        );
        set(
            "widgets.drag.source_origin_y",
            bead::core::Value::Number(surface_y),
        );
        set(
            "widgets.drag.source_origin_valid",
            bead::core::Value::Bool(true),
        );
        set(
            "widgets.drag.target_origin_x",
            bead::core::Value::Number(number("widgets.screen_origin_x")),
        );
        set(
            "widgets.drag.target_origin_y",
            bead::core::Value::Number(number("widgets.screen_origin_y")),
        );
        set(
            "widgets.drag.target_output_id",
            bead::core::Value::Text(output),
        );
        Some((key, values))
    }
    fn send_widget_drag_prepare(
        &self,
        key: &WidgetHoverPreviewKey,
        values: BTreeMap<String, bead::core::Value>,
    ) -> bool {
        let Some(wakeup) = &self.wakeup else {
            return false;
        };
        wakeup(UiEvent::CanvasPrepare {
            view: "widgets-drag".into(),
            output: Some(key.output.clone()),
            values,
            initial_bounds: Some(key.initial_bounds),
        });
        true
    }
    fn widget_drag_is_active(&self) -> bool {
        self.widget_drag_wanted
            || self.widget_drag_model.as_ref().is_some_and(|model| {
                model.get("widgets.drag.active") == Some(&bead::core::Value::Bool(true))
            })
            || self.session.stores.get("widgets.drag.active")
                == Some(&bead::core::Value::Bool(true))
            || self.session.transfer_candidate().is_some()
            || self.session.transfer_source().is_some()
    }
    fn retire_widget_hover_preview(&mut self) {
        if self.widget_drag_is_active() {
            return;
        }
        if self.widgets_write.is_none() && self.widget_drag_controller_primed {
            self.widget_drag_controller = None;
            self.widget_drag_controller_primed = false;
        }
        let Some(previous) = self.widget_hover_preview.take() else {
            return;
        };
        if let Some(wakeup) = &self.wakeup {
            wakeup(UiEvent::CanvasOffer {
                view: "widgets-drag".into(),
                output: Some(previous.key.output),
                open: false,
                values: self.widget_drag_preview_values(),
            });
        }
    }
    fn prepare_widget_drag_preview(&mut self, x: f32, y: f32) {
        let Some(candidate) = self.session.transfer_candidate() else {
            return;
        };
        let Some((key, values)) = self.widget_drag_preview(x, y, candidate) else {
            return;
        };
        if self
            .widget_hover_preview
            .as_ref()
            .is_some_and(|previous| previous.key.output != key.output)
        {
            self.retire_widget_hover_preview();
        }
        if self.send_widget_drag_prepare(&key, values) {
            self.widget_hover_preview = Some(WidgetHoverPreview {
                key,
                prepared_at: std::time::Instant::now(),
            });
        }
    }
    fn prewarm_widget_drag_hover(&mut self, x: f32, y: f32, pointer_event: bool) {
        if self.widget_drag_is_active() {
            return;
        }
        let candidate = self.session.transfer_candidate_at(x, y);
        let inside_drop_target =
            self.widget_drag_suppress_hover_bounds
                .is_some_and(|(left, top, width, height)| {
                    x >= left && x < left + width && y >= top && y < top + height
                });
        if suppress_widget_drag_hover(
            &mut self.widget_drag_suppress_hover_until_leave,
            inside_drop_target,
            pointer_event,
        ) {
            return;
        }
        self.widget_drag_suppress_hover_bounds = None;
        let preview = candidate.and_then(|candidate| self.widget_drag_preview(x, y, candidate));
        let Some((key, values)) = preview else {
            self.retire_widget_hover_preview();
            return;
        };
        let now = std::time::Instant::now();
        if self.widget_hover_preview.as_ref().is_some_and(|previous| {
            previous.key == key
                && now.saturating_duration_since(previous.prepared_at) < WIDGET_HOVER_PREVIEW_DEDUPE
        }) {
            return;
        }
        if self
            .widget_hover_preview
            .as_ref()
            .is_some_and(|previous| previous.key.output != key.output)
        {
            self.retire_widget_hover_preview();
        }
        if self.send_widget_drag_prepare(&key, values) {
            self.widget_hover_preview = Some(WidgetHoverPreview {
                key,
                prepared_at: now,
            });
            if self.widget_drag_controller.is_none() && self.widgets_write.is_none() {
                match crate::widgets::widget_drag::Controller::new() {
                    Ok(controller) => {
                        self.widget_drag_controller = Some(controller);
                        self.widget_drag_controller_primed = true;
                    }
                    Err(error) => eprintln!("WIDGET_DRAG_PREWARM {error}"),
                }
            }
        }
    }
    fn stage_widget_drag_values(
        &self,
        values: &BTreeMap<String, bead::core::Value>,
    ) -> BTreeMap<String, bead::core::Value> {
        values
            .iter()
            .filter(|(name, _)| {
                (name.starts_with("widgets.drag.")
                    || matches!(
                        name.as_str(),
                        "widgets.draft"
                            | "widgets.undo"
                            | "widgets.error"
                            | "widgets.message"
                            | "widgets.editing"
                            | "widgets.app.open"
                    ))
                    && self.session.stores.get(name).is_some()
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
    fn stage_widget_drag_state(
        &self,
        values: &BTreeMap<String, bead::core::Value>,
    ) -> BTreeMap<String, bead::core::Value> {
        values
            .iter()
            .filter(|(name, _)| {
                name.starts_with("widgets.drag.") && self.session.stores.get(name).is_some()
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
    fn retire_widget_drag_on_other_output(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        let Some(controller) = self.widget_drag_controller.as_mut() else {
            return self.finish_scene(None);
        };
        let epoch = controller.drag_epoch();
        let values = controller.drag(bead::core::Value::list(vec![
            serde_json::json!({"phase":"retire","epoch":epoch}),
        ]))?;
        self.widget_drag_model = Some(values.clone());
        self.widget_drag_retire_cleanup = true;
        let projection = self.stage_widget_drag_state(&values);
        let scene = self
            .session
            .update(projection)
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;
        let scene = self.finish_scene(scene)?;
        if scene.is_none() {
            self.widget_drag_controller = None;
            self.widget_drag_controller_primed = false;
            self.widget_drag_model = None;
            self.widget_drag_retire_cleanup = false;
            self.widget_drag_wanted = false;
            self.retire_widget_hover_preview();
        }
        Ok(scene)
    }
    fn register_widget_drag_action(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.widget_drag_action_registered {
            let defaults = BTreeMap::from([
                ("widgets.editing".into(), bead::core::Value::Bool(false)),
                ("widgets.app.open".into(), bead::core::Value::Bool(false)),
                ("widgets.drag.active".into(), bead::core::Value::Bool(false)),
                (
                    "widgets.drag.source".into(),
                    bead::core::Value::list(Vec::new()),
                ),
                (
                    "widgets.drag.destination".into(),
                    bead::core::Value::Text(String::new()),
                ),
                (
                    "widgets.drag.phase".into(),
                    bead::core::Value::Text(String::new()),
                ),
                ("widgets.drag.epoch".into(), bead::core::Value::Number(0.)),
                (
                    "widgets.drag.base".into(),
                    bead::core::Value::list(Vec::new()),
                ),
                (
                    "widgets.drag.original".into(),
                    bead::core::Value::list(Vec::new()),
                ),
                (
                    "widgets.drag.layout".into(),
                    bead::core::Value::list(Vec::new()),
                ),
                ("widgets.drag.x".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.y".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.start_x".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.start_y".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.width".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.height".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.grab_x".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.grab_y".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.snap_x".into(), bead::core::Value::Number(0.)),
                ("widgets.drag.snap_y".into(), bead::core::Value::Number(0.)),
                (
                    "widgets.drag.target_origin_x".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.target_origin_y".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.target_output_id".into(),
                    bead::core::Value::Text(String::new()),
                ),
                (
                    "widgets.drag.source_origin_x".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.source_origin_y".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.source_origin_valid".into(),
                    bead::core::Value::Bool(false),
                ),
                (
                    "widgets.drag.app_origin_x".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.app_origin_y".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.app_origin_valid".into(),
                    bead::core::Value::Bool(false),
                ),
                (
                    "widgets.drag.app_width".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.app_height".into(),
                    bead::core::Value::Number(0.),
                ),
                (
                    "widgets.drag.app_output_id".into(),
                    bead::core::Value::Text(String::new()),
                ),
            ]);
            self.session.stores = self.session.stores.reconcile(&defaults)?;
            self.actions.register("widgets.drag", "ui.widgets")?;
            let _ = self.actions.register("widgets.edit", "ui.widgets");
            self.actions.grant("ui.widgets");
            self.widget_drag_action_registered = true;
        }
        Ok(())
    }
    fn widget_drag_motion(argument: &bead::core::Value) -> bool {
        matches!(argument, bead::core::Value::List(records)
            if records.first().and_then(|record|record["phase"].as_str())==Some("move"))
    }
    fn widget_drag_origin_argument(
        argument: &bead::core::Value,
        origin_x: f64,
        origin_y: f64,
    ) -> bead::core::Value {
        let bead::core::Value::List(records) = argument else {
            return argument.clone();
        };
        let mut records = (**records).clone();
        if let Some(event) = records
            .first_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            event
                .entry("source_origin_x")
                .or_insert(serde_json::json!(origin_x));
            event
                .entry("source_origin_y")
                .or_insert(serde_json::json!(origin_y));
        }
        bead::core::Value::list(records)
    }
    fn begin_widget_drag_write(
        &mut self,
        document: serde_json::Value,
        model: &BTreeMap<String, bead::core::Value>,
        previous_undo: Option<bead::core::Value>,
    ) -> Result<(), Box<dyn Error>> {
        if self.widgets_write.is_some() {
            return Ok(());
        }
        let old_layout = model
            .get("widgets.layout")
            .cloned()
            .unwrap_or_else(|| bead::core::Value::list(Vec::new()));
        let old_undo = previous_undo.unwrap_or_else(|| bead::core::Value::list(Vec::new()));
        let old_revision = match model.get("widgets.rev") {
            Some(bead::core::Value::Number(rev)) if rev.is_finite() && *rev >= 0. => *rev as u64,
            _ => 0,
        };
        let document_value = bead::core::Value::list(vec![document.clone()]);
        self.widgets_write_previous = Some((old_layout.clone(), old_undo.clone(), old_revision));
        self.widgets_write_document = Some(document_value.clone());
        self.widgets_drag_write = true;
        let result: Result<WidgetWriteResult, String> = if let Some(client) = &self.widgets_client {
            let status = client.status();
            (|| -> Result<_, String> {
                let method = status
                    .methods
                    .get("settings.set")
                    .filter(|method| method.v == 1)
                    .ok_or("Widget settings are unavailable")?;
                let boot = status
                    .boot
                    .clone()
                    .ok_or("Widget settings are disconnected")?;
                let wakeup = self
                    .wakeup
                    .clone()
                    .ok_or("Widget event channel unavailable")?;
                client.begin_call("settings.set".into(),serde_json::json!({"store":"widgets","key":["layout"],"value":document,"rev":old_revision}),
                    std::time::Duration::from_millis(u64::from(method.timeout).clamp(1,30_000)),
                    crate::nacre_source::CallGuard{boot,version:1},Some(std::sync::Arc::new(move||wakeup(UiEvent::Model(BTreeMap::new())))))
                    .map(WidgetWriteResult::Pending).map_err(|error|error.message)
            })()
        } else {
            self.widgets_file
                .as_deref()
                .ok_or("Widget layout path unavailable".to_owned())
                .and_then(|path| {
                    crate::widgets::save_file_revisioned(path, &document, old_revision)
                        .map_err(|error| error.to_string())
                })
                .map(WidgetWriteResult::Saved)
        };
        match result {
            Ok(WidgetWriteResult::Pending(ticket)) => {
                self.widgets_write = Some(ticket);
            }
            Ok(WidgetWriteResult::Saved(rev)) => {
                self.widgets_drag_write = false;
                self.widgets_write_previous = None;
                self.widgets_write_document = None;
                let mut values = model.clone();
                values.insert("widgets.layout".into(), document_value);
                values.insert("widgets.draft".into(), bead::core::Value::list(Vec::new()));
                values.insert("widgets.rev".into(), bead::core::Value::Number(rev as f64));
                values.insert("widgets.write".into(), bead::core::Value::list(Vec::new()));
                values.insert(
                    "widgets.error".into(),
                    bead::core::Value::Text(String::new()),
                );
                values.insert(
                    "widgets.message".into(),
                    bead::core::Value::Text("Changes saved".into()),
                );
                self.widget_drag_model = Some(values);
            }
            Err(error) => {
                let was_drag_write = matches!(
                    model.get("widgets.drag.active"),
                    Some(bead::core::Value::Bool(true))
                );
                self.widgets_drag_write = false;
                self.widgets_write_document = None;
                let previous = self.widgets_write_previous.take();
                let mut values = model.clone();
                if let Some((layout, undo, revision)) = previous {
                    values.insert("widgets.layout".into(), layout.clone());
                    values.insert("widgets.undo".into(), undo.clone());
                    values.insert(
                        "widgets.rev".into(),
                        bead::core::Value::Number(revision as f64),
                    );
                } else {
                    values.insert("widgets.layout".into(), old_layout.clone());
                    values.insert("widgets.undo".into(), old_undo.clone());
                    values.insert(
                        "widgets.rev".into(),
                        bead::core::Value::Number(old_revision as f64),
                    );
                }
                values.insert("widgets.draft".into(), bead::core::Value::list(Vec::new()));
                values.insert("widgets.write".into(), bead::core::Value::list(Vec::new()));
                values.insert("widgets.error".into(), bead::core::Value::Text(error));
                if was_drag_write && let Some(controller) = self.widget_drag_controller.as_mut() {
                    let epoch = controller.drag_epoch();
                    if let Ok(returning) = controller.drag(bead::core::Value::list(vec![
                        serde_json::json!({"phase":"cancel","epoch":epoch}),
                    ])) {
                        for (key, value) in returning {
                            if key.starts_with("widgets.drag.") {
                                values.insert(key, value);
                            }
                        }
                    }
                }
                self.widget_drag_model = Some(values);
            }
        }
        Ok(())
    }
    fn refresh_widget_write_scene(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        let Some(values) = self.widget_drag_model.as_ref().cloned() else {
            return self.finish_scene(None);
        };
        let mut changes = self.stage_widget_drag_values(&values);
        for key in ["widgets.layout", "widgets.rev"] {
            if let Some(value) = values.get(key) {
                changes.insert(key.to_owned(), value.clone());
            }
        }
        let scene = self
            .session
            .update(changes)
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;
        self.finish_scene(scene)
    }
    /// The host marked this canvas covered (fullscreen window or lock on its output).
    fn widgets_paused(&self) -> bool {
        self.session.stores.get(WIDGETS_COVERED) == Some(&bead::core::Value::Bool(true))
    }
    /// B0193 `pause_on = {fullscreen, lock}` for the desktop layer. Editing and dragging
    /// never pause: the owner is looking at the widgets.
    fn widgets_covered_now(&self) -> bool {
        let on = |key: &str| self.session.stores.get(key) == Some(&bead::core::Value::Bool(true));
        if !self.bar || on("widgets.editing") || on("widgets.drag.active") {
            return false;
        }
        let (Some(output), Some(backend), Some(desktop)) =
            (self.output.as_deref(), &self.backend, &self.desktop)
        else {
            return false;
        };
        crate::wallpaper_motion::output_hidden(&backend.snapshot(), &desktop.snapshot(), output)
    }
    /// Tell the canvas when its output is covered or visible again. While covered the host
    /// sends no further projections, so the canvas rebuilds nothing and drops its seconds
    /// lease; on uncover the next projection is a full, current one.
    fn sync_widgets_covered(&mut self) -> bool {
        let covered = self.widgets_covered_now();
        if covered != self.widgets_covered {
            self.widgets_covered = covered;
            if !covered {
                self.widget_canvas_projection = None;
                self.widgets_projection = None;
            }
            if self.widgets_canvas_wanted
                && let Some(wakeup) = &self.wakeup
            {
                wakeup(UiEvent::CanvasModel {
                    view: "widgets-canvas".into(),
                    output: self.output.clone(),
                    values: BTreeMap::from([(
                        WIDGETS_COVERED.into(),
                        bead::core::Value::Bool(covered),
                    )]),
                });
            }
        }
        covered
    }
    fn publish_widget_models(&mut self) {
        let covered = self.sync_widgets_covered();
        if self.widget_editor && !self.widget_app {
            let keys = [
                "widgets.layout",
                "widgets.draft",
                "widgets.editing",
                "widgets.selected",
                "widgets.geometry",
                "widgets.canvas.width",
                "widgets.canvas.height",
                "widgets.drag.active",
                "widgets.drag.source",
                "widgets.drag.x",
                "widgets.drag.y",
            ];
            let projection: BTreeMap<_, _> = keys
                .into_iter()
                .filter_map(|key| {
                    self.session
                        .stores
                        .get(key)
                        .map(|value| (key.to_owned(), value.clone()))
                })
                .collect();
            if self.widgets_projection.as_ref() != Some(&projection) {
                self.widgets_projection = Some(projection.clone());
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::HostModel(projection));
                }
            }
        }
        if !(self.bar || self.widget_canvas) {
            return;
        }
        let mut values = self.widget_host_values();
        if let Some(model) = &self.widget_drag_model {
            for (key, value) in model {
                if key.starts_with("widgets.drag.") {
                    values.insert(key.clone(), value.clone());
                }
            }
        }
        let mut canvas_values = self.widget_canvas_values();
        for (key, value) in &values {
            if key.starts_with("widgets.drag.") && canvas_values.contains_key(key.as_str()) {
                canvas_values.insert(key.clone(), value.clone());
            }
        }
        let drag_values = self.widget_drag_projection(&values);
        if self.bar || self.widget_canvas {
            let active = values.get("widgets.drag.active") == Some(&bead::core::Value::Bool(true));
            let previewing = active
                && matches!(values.get("widgets.drag.phase"),
                Some(bead::core::Value::Text(phase)) if phase=="dragging"||phase=="settle");
            let transient_first = previewing
                && matches!(values.get("widgets.drag.layout"),Some(bead::core::Value::List(documents)) if !documents.is_empty());
            let draft_first = values.get("widgets.editing") == Some(&bead::core::Value::Bool(true))
                && matches!(values.get("widgets.draft"),Some(bead::core::Value::List(documents)) if !documents.is_empty());
            let layout_key = if transient_first {
                "widgets.drag.layout"
            } else if draft_first {
                "widgets.draft"
            } else {
                "widgets.layout"
            };
            let layout = values
                .get(layout_key)
                .cloned()
                .unwrap_or_else(|| bead::core::Value::list(Vec::new()));
            let has_canvas = match &layout {
                bead::core::Value::List(documents) => documents
                    .first()
                    .and_then(|doc| doc["hosts"]["canvas"]["instances"].as_array())
                    .is_some_and(|instances| !instances.is_empty()),
                _ => false,
            };
            let wanted = has_canvas;
            let canvas_output = if active {
                values
                    .get("widgets.drag.target_output_id")
                    .and_then(|value| {
                        if let bead::core::Value::Text(name) = value {
                            (!name.is_empty()).then(|| name.clone())
                        } else {
                            None
                        }
                    })
                    .or_else(|| self.output.clone())
            } else {
                self.output.clone()
            };
            if self.bar && wanted != self.widgets_canvas_wanted {
                self.widgets_canvas_wanted = wanted;
                self.widgets_canvas_layout = Some(layout.clone());
                self.widget_canvas_projection = None;
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::CanvasOffer {
                        view: "widgets-canvas".into(),
                        output: canvas_output.clone(),
                        open: wanted,
                        values: canvas_values.clone(),
                    });
                }
            }
            if self.bar && wanted && covered {
                // Paused: keep the last projection so nothing is rebuilt while covered.
            } else if self.bar && wanted {
                let mut projection = canvas_values.clone();
                if !canvas_needs_seconds(Some(&layout)) {
                    projection.remove(CLOCK_SECONDS);
                }
                if self.widget_canvas_projection.as_ref() != Some(&projection) {
                    bead::core::vprintln!(
                        "CANVAS_PROJECTION path=models changed={}",
                        projection_diff(self.widget_canvas_projection.as_ref(), &projection)
                    );
                    self.widget_canvas_projection = Some(projection.clone());
                    if let Some(wakeup) = &self.wakeup {
                        wakeup(UiEvent::CanvasModel {
                            view: "widgets-canvas".into(),
                            output: canvas_output,
                            values: projection,
                        });
                    }
                }
            } else {
                self.widget_canvas_projection = None;
            }
        }
        self.publish_widget_drag_projection(drag_values);
    }
    fn retire_widget_drag_after_feedback(&mut self) {
        // Keep the dropped target from reopening the just-retired ghost. Once the
        // pointer leaves its bounds, hover can prepare the next destination early.
        self.widget_drag_suppress_hover_until_leave = true;
        self.widget_drag_suppress_hover_bounds = self
            .session
            .transfer_candidate_at(self.pointer_at.0, self.pointer_at.1)
            .map(|candidate| candidate.source_rect);
        self.widget_drag_epoch = self.widget_drag_epoch.wrapping_add(1).max(1u64 << 63);
        let token = self.widget_drag_epoch;
        if let Some(wakeup) = self.wakeup.clone() {
            let _ = std::thread::Builder::new()
                .name("widgets-drag-retire".into())
                .stack_size(64 * 1024)
                .spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(205));
                    wakeup(UiEvent::Timer { token });
                });
        }
    }
    /// A freshly opened prompt or form takes keyboard focus on its first field, so the
    /// user can type (and press Enter) at once.
    fn focus_network_form(
        &mut self,
        scene: Option<Scene>,
        mut changes: BTreeMap<String, bead::core::Value>,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        let field = match self.session.stores.get("network.view") {
            Some(bead::core::Value::Text(view)) if view == "prompt" => "status-popout.secret.field",
            Some(bead::core::Value::Text(view)) if view == "hidden" => "status-popout.ssid",
            _ => return Ok(scene),
        };
        let field = if self.dock_status.is_empty() {
            field.to_owned()
        } else {
            format!(
                "{}{field}",
                match self.session.stores.get("cc.detail_focus_prefix") {
                    Some(bead::core::Value::Text(prefix)) => prefix.as_str(),
                    _ => "",
                }
            )
        };
        if self.session.stores.get("bead.focused") == Some(&bead::core::Value::Text(field.clone()))
        {
            return Ok(scene);
        }
        // `update` builds on the committed stores, so the pending changes ride along.
        changes.insert("bead.focused".into(), bead::core::Value::Text(field));
        Ok(self.session.update(changes)?.or(scene))
    }
    fn dropdown_kind(&self, key: &str) -> String {
        self.session
            .view_nodes()
            .iter()
            .find(|node| node.id == key)
            .and_then(|node| node.action_arg.as_ref())
            .and_then(crate::widgets::dropdown_view)
            .unwrap_or(key)
            .to_owned()
    }
    fn publish_status_pill(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        // Pointer-region and timer events also reach companion surfaces. Only
        // the bar owns the pill state declared by bar/behavior.luau.
        if !self.bar {
            return Ok(None);
        }
        let values = BTreeMap::from([
            (
                "status.hovered".into(),
                bead::core::Value::Text(
                    self.dropdown_kind(self.status_pill.hovered().unwrap_or("")),
                ),
            ),
            (
                "status.peek.visible".into(),
                bead::core::Value::Bool(self.status_pill.peek().is_some()),
            ),
            (
                "status.peek.kind".into(),
                bead::core::Value::Text(self.dropdown_kind(self.status_pill.peek().unwrap_or(""))),
            ),
            (
                "status.panel.kind".into(),
                bead::core::Value::Text(self.dropdown_kind(self.status_pill.panel().unwrap_or(""))),
            ),
        ]);
        self.session.update(values).map_err(Into::into)
    }

    fn schedule_status_pill(&mut self) {
        let deadline = self.status_pill.next_deadline();
        if deadline == self.status_pill_scheduled {
            return;
        }
        self.status_pill_epoch = self.status_pill_epoch.wrapping_add(1);
        self.status_pill_scheduled = deadline;
        let Some(deadline) = deadline else { return };
        let delay = deadline.saturating_duration_since(std::time::Instant::now());
        let epoch = self.status_pill_epoch;
        let Some(wakeup) = self.wakeup.clone() else {
            return;
        };
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            wakeup(UiEvent::Timer { token: epoch });
        });
    }

    fn sync_status_peek_route(&mut self) -> Result<(), Box<dyn Error>> {
        let desired = self
            .status_pill
            .panel()
            .map(|key| {
                let kind = self.dropdown_kind(key);
                if kind == "audio" {
                    "sound".to_owned()
                } else {
                    kind.to_owned()
                }
            })
            .or_else(|| {
                self.status_pill.peek().map(|key| {
                    let kind = self.dropdown_kind(key);
                    match kind.as_str() {
                        "calendar" | "session" => kind,
                        "sound" => "peek-audio".to_owned(),
                        _ => format!("peek-{kind}"),
                    }
                })
            });
        let center = desired.as_ref().and_then(|view| {
            let item = view.strip_prefix("peek-").unwrap_or(view);
            self.status_icon_center(if item == "sound" { "audio" } else { item })
        });
        let anchor = self
            .widget_canvas
            .then(|| {
                let id = self.status_pill.panel().or(self.status_pill.peek())?;
                let node = self
                    .session
                    .view_nodes()
                    .iter()
                    .find(|node| node.id == id)?;
                let rect = bead::ui::rect(&node.draw);
                Some(bead::wayland::PanelAnchor {
                    rect,
                    edge: if rect.y + rect.h / 2. > self.viewport_size.1 as f32 / 2. {
                        bead::core::Edge::Bottom
                    } else {
                        bead::core::Edge::Top
                    },
                })
            })
            .flatten();
        if desired == self.status_pill_route
            && center == self.status_route_center
            && anchor == self.status_route_anchor
            && self.status_pill.panel().or(self.status_pill.peek())
                == self.status_route_id.as_deref()
        {
            return Ok(());
        }
        if let Some(view) = desired {
            self.wakeup
                .as_ref()
                .ok_or("status panel event channel unavailable")?(
                if let Some(anchor) = anchor {
                    UiEvent::CompanionSelectAt {
                        view: view.clone(),
                        output: self.output.clone(),
                        anchor,
                    }
                } else {
                    UiEvent::CompanionSelect {
                        view: view.clone(),
                        output: self.output.clone(),
                        toggle: false,
                        center,
                    }
                },
            );
            self.status_pill_route = Some(view);
            self.status_route_center = center;
            self.status_route_anchor = anchor;
            self.status_route_id = self
                .status_pill
                .panel()
                .or(self.status_pill.peek())
                .map(String::from);
        } else if self.status_pill_route.take().is_some()
            && let Some(wakeup) = &self.wakeup
        {
            wakeup(UiEvent::CompanionVisibility(false));
        }
        Ok(())
    }

    fn status_icon_center(&self, item: &str) -> Option<f32> {
        if let Some(anchor) = self
            .status_pill
            .panel()
            .or(self.status_pill.peek())
            .or(self.status_pill.hovered())
            && let Some(node) = self.session.view_nodes().iter().find(|node| {
                node.id == anchor
                    && node
                        .action_arg
                        .as_ref()
                        .and_then(crate::widgets::dropdown_view)
                        == Some(item)
            })
        {
            let rect = bead::ui::rect(&node.draw);
            return Some(rect.x + rect.w / 2.);
        }
        let arg = if item == "audio" { "audio" } else { item };
        self.session
            .view_nodes()
            .iter()
            .filter(|node| {
                (node.action.as_deref() == Some("status.pill.open")
                    && node
                        .action_arg
                        .as_ref()
                        .and_then(crate::widgets::dropdown_view)
                        == Some(arg))
                    || (item == "calendar"
                        && node.id.rsplit('.').next() == Some("clock")
                        && node.action.as_deref() == Some("view.open"))
            })
            .map(|node| bead::ui::rect(&node.draw))
            .reduce(|bounds, rect| bounds.union(rect))
            .map(|rect| rect.x + rect.w / 2.)
    }

    fn status_keep_open_zone(&self) -> Option<bead::ui::status_pill::KeepOpenZone> {
        use bead::ui::status_pill::{KeepOpenZone, PointerRect};
        let owner_id = self.status_pill.panel()?;
        let view = self.dropdown_kind(owner_id);
        let width = crate::popouts::status_panel_width(&view);
        let center = self
            .status_route_center
            .or_else(|| self.status_icon_center(&view))?;
        let bar_height = match self.session.stores.get("bar.height") {
            Some(bead::core::Value::Number(height)) if height.is_finite() && *height > 0. => {
                *height as f32
            }
            _ => 50.,
        };
        let owner_slot = self
            .session
            .view_nodes()
            .iter()
            .find(|node| node.id == owner_id)
            .map(|node| {
                let rect = bead::ui::rect(&node.draw);
                PointerRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.w,
                    height: rect.h,
                }
            })
            .unwrap_or(PointerRect {
                x: center - 24.,
                y: 0.,
                width: 48.,
                height: bar_height,
            });
        Some(KeepOpenZone {
            dropdown: PointerRect {
                x: center - width / 2.,
                y: bar_height,
                width,
                height: 1.,
            },
            bar_band: PointerRect {
                x: center - width / 2.,
                y: 0.,
                width,
                height: bar_height,
            },
            owner_slot,
            tolerance: 4.,
        })
    }

    fn consume_dismissed_owner_click(&mut self, id: &str) -> bool {
        consume_pending_owner_dismiss(
            &mut self.status_owner_dismissed,
            id,
            std::time::Instant::now(),
        )
    }

    #[cfg(test)]
    pub(crate) fn isolate_state(&mut self, path: &std::path::Path) {
        self.state_file = None;
        self.session.restore_on_start(BTreeMap::new());
        self.session.enable_recovery(path.into());
    }
    pub fn new(session: bead::ui::Session) -> Self {
        Self::for_output(session, None)
    }
    pub fn for_output(mut session: bead::ui::Session, output: Option<String>) -> Self {
        crate::i18n::init();
        session.set_font_picker_if_missing(std::sync::Arc::new(|scene| {
            crate::font_sources::select(scene).unwrap_or_default()
        }));
        if let Some(name) = &output {
            session.stores = session
                .stores
                .reconcile(&BTreeMap::from([(
                    "bead.output.name".into(),
                    bead::core::Value::Text(name.clone()),
                )]))
                .expect("valid output name store");
        }
        scope_recovery(&mut session, output.as_deref());
        let state_file = match crate::state::File::path()
            .map_err(|e| -> Box<dyn Error> { e.into() })
            .and_then(crate::state::File::open)
        {
            Ok((file, saved)) => {
                session.restore_on_start(saved);
                Some(file)
            }
            Err(error) => {
                eprintln!("STATE_ERROR restore disabled: {error}");
                None
            }
        };
        Self {
            session,
            output,
            pending: None,
            chrome: None,
            spare_chrome: None,
            detached: false,
            launcher: None,
            launcher_initial_projection: std::rc::Rc::new(std::cell::Cell::new(true)),
            launcher_subscription: None,
            launcher_quick: None,
            launcher_quick_subscription: Vec::new(),
            calendar: None,
            calendar_subscription: None,
            i18n_subscription: None,
            status: None,
            dock_status: BTreeMap::new(),
            dock_subscriptions: Vec::new(),
            status_subscription: None,
            status_control_subscription: None,
            extras: None,
            prompt: None,
            extras_subscription: Vec::new(),
            extras_wake: None,
            session_menu: None,
            session_menu_subscription: None,
            chat: None,
            chat_subscription: Vec::new(),
            settings: None,
            colors: None,
            welcome: None,
            control: None,
            control_subscription: Vec::new(),
            dashboard: None,
            dashboard_factory: None,
            pending_dashboard: None,
            dashboard_pearl_animation: None,
            dashboard_pearl_deadline: None,
            panel_animation_hold_until: None,
            pending_panel_animation_duration: None,
            pending_panel_navigation: None,
            dashboard_subscription: Vec::new(),
            slider_sent: None,
            slider_pending: None,
            slider_release_serial: 0,
            slider_local_active: false,
            launcher_memory: None,
            icon_warmer: None,
            wallpaper_warmer: None,
            connection_status: None,
            connection_subscription: None,
            prefs: None,
            workspaces: Default::default(),
            prefs_subscription: None,
            launcher_trigger: false,
            last_launch: None,
            brightness: None,
            routes: None,
            routes_subscription: None,
            tray_topic: None,
            tray_topic_subscription: None,
            daemon_tray: Default::default(),
            widgets_file: None,
            widgets_watch: None,
            widgets_canvas_layout: None,
            widgets_canvas_editing: false,
            widgets_canvas_wanted: false,
            widget_canvas_projection: None,
            widgets_covered: false,
            wallpaper_luma_watch: None,
            widgets_client: None,
            widget_editor: false,
            widgets_close_on_commit: false,
            widget_canvas: false,
            widget_drag_overlay: false,
            widgets_projection: None,
            widgets_write: None,
            widgets_write_document: None,
            widget_app: false,
            widgets_write_previous: None,
            widget_write_feedback: None,
            widget_drag_pending_epoch: None,
            widget_drag_latest_epoch: None,
            widgets_drag_write: false,
            widget_drag_controller: None,
            widget_drag_controller_primed: false,
            widget_drag_subscription: None,
            widget_drag_app_announced: false,
            widget_active_app: None,
            widget_floating_request: None,
            widget_drag_model: None,
            widget_drag_wanted: false,
            widget_drag_output: None,
            widget_drag_projection: None,
            widget_hover_preview: None,
            widget_drag_suppress_hover_until_leave: false,
            widget_drag_suppress_hover_bounds: None,
            widget_drag_epoch: 1u64 << 63,
            widget_drag_retire_cleanup: false,
            widget_drag_action_registered: false,
            widget_displays_topic: None,
            widget_displays_subscription: None,
            widget_compositor_topic: None,
            widget_compositor_subscription: None,
            widget_compositor_policy_only: false,
            pointer_at: (0., 0.),
            widgets_topic: None,
            widgets_subscription: None,
            updates_topic: None,
            updates_subscription: None,
            recording_topic: None,
            recording_subscription: None,
            bar_daemon: None,
            bar_daemon_subscriptions: Vec::new(),
            origins_topic: None,
            origins_subscription: None,
            backend: None,
            subscription: None,
            last_focused_window: None,
            desktop: None,
            clock_seconds_lease: None,
            desktop_subscription: None,
            desktop_revision: 0,
            notification_hover: None,
            notification_primary: false,
            fonts: Default::default(),
            trace: false,
            notifications: None,
            notification_subscription: None,
            notice_host: None,
            notice_drive: None,
            menu_item: String::new(),
            tray_anchor: None,
            tray_close: false,
            tray_requested_close: false,
            tray_enabled: true,
            tray_hover: String::new(),
            tray_selection: None,
            status_pill: bead::ui::status_pill::StatusPill::new(bead::ui::status_pill::Timings {
                peek_dwell: std::time::Duration::from_millis(350),
                peek_exit: std::time::Duration::from_millis(120),
            }),
            status_pill_epoch: 0,
            status_pill_scheduled: None,
            status_pill_route: None,
            status_owner_dismissed: None,
            status_route_center: None,
            status_route_anchor: None,
            status_route_id: None,
            viewport_size: (1200, 800),
            popup_anchor: None,
            popup_anchor_changed: false,
            host_release_pending: false,
            tray_menu: None,
            pending_focus: None,
            bar: false,
            state_file,
            actions: Default::default(),
            wakeup: None,
            smooth_open_started: None,
            smooth_key_starts: std::collections::VecDeque::new(),
            smooth_scroll_started: None,
            smooth_scroll_last: None,
            smooth_scroll_dropped: 0,
            smooth_scroll_reported: None,
            smooth_animation_dropped: None,
            smooth_last_presented: None,
            smooth_frame_interval: std::time::Duration::from_millis(16),
            smooth_started_at: std::time::Instant::now(),
            smooth_user_interacted: false,
            smooth_bar_startup_animation: false,
            #[cfg(test)]
            reload_events: 0,
        }
    }
    pub fn with_connection_status(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        // Subscriptions deliver immediately, before the first UI generation.
        // Declare this host-owned view state before any callback can update it.
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.reconnecting".into(),
                bead::core::Value::Bool(source.snapshot().data["reconnecting"] == true),
            )]))?;
        self.connection_status = Some(source);
        Ok(self)
    }
    /// B0036: the bar's panel buttons follow the owner's panel enable flags.
    pub fn with_prefs(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        let prefs = crate::prefs::Prefs::from_source(&*source);
        let stores = self.prefs_stores(&prefs);
        self.session.stores = self.session.stores.reconcile(&stores)?;
        // The bar's workspace rows depend on the owner's count, icons and hints.
        if self.workspaces.view.follow(&stores) {
            self.refresh_workspaces()?;
        }
        self.prefs = Some(source);
        Ok(self)
    }
    /// Re-project `bead.workspaces` from the backend snapshot and the current row view.
    fn refresh_workspaces(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(backend) = &self.backend {
            let rows = self
                .workspaces
                .rows(&backend.snapshot(), self.output.as_deref());
            self.session
                .stores
                .commit(std::collections::BTreeMap::from([(
                    "bead.workspaces".into(),
                    bead::core::Value::list(rows),
                )]))?;
        }
        Ok(())
    }
    pub fn with_widget_file(mut self) -> Self {
        self.widgets_file = crate::widgets::file_path();
        // File watchers can publish before the first Luau generation exists.
        self.session.stores = self
            .session
            .stores
            .reconcile(&BTreeMap::from([
                ("widgets.layout".into(), bead::core::Value::list(Vec::new())),
                ("widgets.rev".into(), bead::core::Value::Number(0.)),
            ]))
            .expect("widget transport schema is stable");
        self
    }
    pub fn with_widget_editor(
        mut self,
        client: Option<&crate::nacre_source::Client>,
        canvas: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.widget_editor = !canvas;
        self.widget_canvas = canvas;
        self.actions.register("widgets.edit", "ui.widgets")?;
        self.register_widget_drag_action()?;
        self.actions.grant("ui.widgets");
        if canvas {
            self.actions.register("view.open", "ui.widgets")?;
            self.actions.register("status.pill.open", "ui.widgets")?;
            self.actions.register("launcher.open", "ui.widgets")?;
            self.launcher_trigger = true;
        }
        let mut values = crate::desktop::values(&Default::default(), "");
        values.insert("widgets.editing".into(), bead::core::Value::Bool(!canvas));
        if canvas {
            values.insert("bar.popouts".into(), bead::core::Value::Bool(true));
            values.insert("bar.launcher".into(), bead::core::Value::Bool(true));
            values.extend(BTreeMap::from([
                (
                    "session.confirm".into(),
                    bead::core::Value::Text(String::new()),
                ),
                (
                    "session.message".into(),
                    bead::core::Value::Text(String::new()),
                ),
                (
                    "status.hovered".into(),
                    bead::core::Value::Text(String::new()),
                ),
                ("status.peek.visible".into(), bead::core::Value::Bool(false)),
                (
                    "status.peek.kind".into(),
                    bead::core::Value::Text(String::new()),
                ),
                (
                    "status.panel.kind".into(),
                    bead::core::Value::Text(String::new()),
                ),
            ]));
        }
        values.insert(
            "bead.output.name".into(),
            bead::core::Value::Text(String::new()),
        );
        values.extend(BTreeMap::from([
            ("bead.windows".into(), bead::core::Value::list(Vec::new())),
            (
                "bead.workspaces".into(),
                bead::core::Value::list(Vec::new()),
            ),
            (
                "bead.focused_title".into(),
                bead::core::Value::Text(String::new()),
            ),
        ]));
        self.session.stores = self.session.stores.reconcile(&values)?;
        if let Some(client) = client {
            self.widgets_client = Some(client.clone());
            self = self.with_bar_daemon(client)?;
            if let Ok(topic) = client.view_topic("displays") {
                let topic: std::sync::Arc<dyn bead::core::source::Source> = topic;
                let values = widget_screen_values(&topic.snapshot());
                if !values.is_empty() {
                    self.session.stores = self.session.stores.reconcile(&values)?;
                }
                self.widget_displays_topic = Some(topic);
            }
            let routes = crate::bar_routes::Routes::new(client.clone());
            self.session.stores = self.session.stores.reconcile(&routes.stores())?;
            self.actions.register("bar.title", "ui.widgets")?;
            self.routes = Some(std::rc::Rc::new(routes));
            self.with_widgets(client)
        } else {
            Ok(self.with_widget_file())
        }
    }
    /// Marks this scene as a passive drag preview, not a host.
    /// The bar/canvas supplies its models; the ghost has no direct subscriptions,
    /// panel state, or host-event publisher of its own.
    pub fn with_widget_drag_preview(mut self) -> Self {
        self.widget_canvas = false;
        self.widget_drag_overlay = true;
        self.widget_editor = false;
        self.launcher_trigger = false;
        self.state_file = None;
        self.session.restore_on_start(BTreeMap::new());
        self.session.stores = self
            .session
            .stores
            .reconcile(&BTreeMap::from([(
                "bead.output.name".into(),
                bead::core::Value::Text(String::new()),
            )]))
            .expect("the passive drag behavior declares its output name");
        self
    }
    /// The standalone Widgets app shares the editor's preview services and persistence topic,
    /// but commits each model change without closing its window.
    pub fn with_widget_app(
        mut self,
        client: Option<&crate::nacre_source::Client>,
        root: &std::path::Path,
    ) -> Result<Self, Box<dyn Error>> {
        self = self.with_widget_editor(client, false)?;
        if let Some(path) = crate::widgets::presentation_wallpaper_path() {
            self.session.stores = self.session.stores.reconcile(&BTreeMap::from([(
                "widgets.wallpaper.path".into(),
                bead::core::Value::Text(path),
            )]))?;
        }
        self.widget_app = true;
        self.state_file = None;
        self.session.restore_on_start(BTreeMap::new());
        self.actions.register("widgets.navigate", "ui.widgets")?;
        self.actions.register("plugins.do", "ui.widgets")?;
        for action in ["view.open", "status.pill.open", "launcher.open"] {
            self.actions.register(action, "ui.widgets")?;
        }
        if self.routes.is_none() {
            self.actions.register("bar.title", "ui.widgets")?;
        }
        self.actions.grant("ui.widgets");
        if let Some(desktop) = &self.desktop {
            let values = crate::desktop::values(&desktop.snapshot(), "");
            self.session.stores = self.session.stores.reconcile(&values)?;
        }
        if let Some(client) = client {
            self = self.with_settings(Some(client.clone()), root)?;
            self.session.stores = self.session.stores.reconcile(&BTreeMap::from([
                (
                    "settings.page".into(),
                    bead::core::Value::Text("wallpaper".into()),
                ),
                (
                    "settings.search".into(),
                    bead::core::Value::Text(String::new()),
                ),
            ]))?;
            if self.widget_displays_topic.is_none()
                && let Ok(topic) = client.view_topic("displays")
            {
                let topic: std::sync::Arc<dyn bead::core::source::Source> = topic;
                let values = widget_screen_values(&topic.snapshot());
                if !values.is_empty() {
                    self.session.stores = self.session.stores.reconcile(&values)?;
                }
                self.widget_displays_topic = Some(topic);
            }
            if let Ok(topic) = client.topic("compositor") {
                let topic: std::sync::Arc<dyn bead::core::source::Source> =
                    std::sync::Arc::new(topic);
                let values = widget_app_origin_values(&topic.snapshot());
                self.session.stores = self.session.stores.reconcile(&values)?;
                self.widget_compositor_topic = Some(topic);
            }
        }
        Ok(self)
    }
    /// The daemon stores an opaque JSON document; Luau validates it in a staged update.
    pub fn with_widgets(
        mut self,
        client: &crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        self.register_widget_drag_action()?;
        if self.widget_displays_topic.is_none()
            && let Ok(topic) = client.view_topic("displays")
        {
            let topic: std::sync::Arc<dyn bead::core::source::Source> = topic;
            let values = widget_screen_values(&topic.snapshot());
            if !values.is_empty() {
                self.session.stores = self.session.stores.reconcile(&values)?;
            }
            self.widget_displays_topic = Some(topic);
        }
        // An explicit standalone layout override remains independent of service transport.
        if std::env::var_os("BEAD_WIDGET_LAYOUT").is_some() {
            self.widgets_client = None;
            return Ok(self.with_widget_file());
        }
        self.widgets_client = Some(client.clone());
        let topic = match client.view_topic(crate::widgets::TOPIC) {
            Ok(topic) => topic,
            Err(_) => {
                eprintln!("SOURCE_ERROR code=widgets_lease");
                return Ok(self);
            }
        };
        let values = crate::widgets::values(&bead::core::source::Source::snapshot(&*topic));
        self.session.stores = self.session.stores.reconcile(&values)?;
        self.widgets_topic = Some(topic);
        Ok(self)
    }
    /// Optional bar item; one daemon lease, no local polling or provider.
    pub fn with_bar_updates(
        mut self,
        client: &crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        if self.session.stores.get("bar.updates.enabled") == Some(&bead::core::Value::Bool(false)) {
            return Ok(self);
        }
        match client.lease_topic("dashboard.updates") {
            Ok(topic) => {
                self.session.stores = self.session.stores.reconcile(
                    &crate::bar_updates::values(&bead::core::source::Source::snapshot(&topic)),
                )?;
                self.updates_topic = Some(std::sync::Arc::new(topic));
            }
            Err(_) => eprintln!("SOURCE_ERROR code=bar_updates_lease"),
        }
        Ok(self)
    }
    /// B0199: the recording indicator follows `controls.recorder.recording` (one more lease while
    /// a bar exists; nothing ticks unless a recording runs).
    pub fn with_bar_recording(
        mut self,
        client: &crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        match client.lease_topic("controls") {
            Ok(topic) => {
                let on =
                    crate::bar_recording::recording(&bead::core::source::Source::snapshot(&topic));
                self.session.stores = self
                    .session
                    .stores
                    .reconcile(&crate::bar_recording::values(on, 0))?;
                self.recording_topic = Some((std::sync::Arc::new(topic), client.clone()));
                self.actions
                    .register("bar.recording.stop", "bar.recording")?;
                self.actions.grant("bar.recording");
            }
            Err(_) => eprintln!("SOURCE_ERROR code=bar_recording_lease"),
        }
        Ok(self)
    }
    /// N0040 visitor groups: on niri the daemon `compositor` topic carries each workspace's home
    /// output and number. Share notifications' subscription when present, otherwise lease
    /// for the bar's lifetime; other backends need nothing.
    pub fn with_workspace_origins(
        self,
        client: &crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        if !self
            .backend
            .as_ref()
            .is_some_and(|backend| backend.snapshot().backend == "niri")
        {
            return Ok(self);
        }
        match client.view_topic("compositor") {
            Ok(topic) => self.with_origins_topic(topic),
            Err(_) => {
                eprintln!("SOURCE_ERROR code=compositor_topic_lease");
                Ok(self)
            }
        }
    }
    /// Bar status icons use the same reconnecting daemon topics as the popouts.
    pub fn with_bar_daemon(
        mut self,
        client: &crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        let source = std::sync::Arc::new(bar_daemon::BarDaemon::new(client)?);
        self.session.stores = self.session.stores.reconcile(&source.values())?;
        self.bar_daemon = Some(source);
        Ok(self)
    }
    fn with_origins_topic(
        mut self,
        topic: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        let stores = BTreeMap::from([(
            "bar.workspace.origins".into(),
            bead::core::Value::list(crate::workspace_rows::origin_rows(&topic.snapshot().data)),
        )]);
        self.session.stores = self.session.stores.reconcile(&stores)?;
        if self.workspaces.view.follow(&stores) {
            self.refresh_workspaces()?;
        }
        if self.widget_compositor_topic.is_none() {
            self.widget_compositor_topic = Some(topic.clone());
        }
        self.origins_topic = Some(topic);
        Ok(self)
    }
    /// Host stores the preferences drive: the bar's, or the launcher rail's.
    fn prefs_stores(
        &self,
        prefs: &crate::prefs::Prefs,
    ) -> std::collections::BTreeMap<String, bead::core::Value> {
        if self.launcher.is_some() {
            prefs.launcher_stores()
        } else {
            prefs.bar_stores()
        }
    }
    pub fn panel_session(session: bead::ui::Session, open: bool) -> Self {
        let mut runtime = Self::new(session);
        if let Ok((_, mut saved)) = crate::state::File::path()
            .map_err(|e| -> Box<dyn Error> { e.into() })
            .and_then(crate::state::File::open)
        {
            saved.insert("panel.open".into(), bead::core::Value::Bool(open));
            runtime.session.restore_on_start(saved);
        }
        runtime
    }
    pub fn with_launcher(
        mut self,
        source: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let menu = std::rc::Rc::new(crate::launcher::Menu::new(source)?);
        let projector = menu.clone();
        let initial = self.launcher_initial_projection.clone();
        self.session.set_projector(Box::new(move |stores| {
            if initial.replace(false) {
                projector.project_initial(stores, std::time::Duration::from_millis(20))
            } else {
                projector.project(stores)
            }
        }));
        self.session.set_projection_coalescing(true);
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text(String::new()),
            )]))?;
        self.session
            .stores
            .commit(std::collections::BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text("launcher.search".into()),
            )]))?;
        self.actions.register("select_app", "launcher")?;
        self.actions.register("toggle_favorite", "launcher")?;
        self.actions.register("launcher.form", "launcher")?;
        self.actions.grant("launcher");
        self.launcher = Some(menu);
        self.fonts = crate::font_sources::Selector::default().with_ascii();
        Ok(self)
    }
    /// Quick control leases, projections and pending calls die with the launcher runtime.
    pub fn with_launcher_quick(
        mut self,
        quick: crate::launcher_quick::Quick,
    ) -> Result<Self, Box<dyn Error>> {
        let menu = self
            .launcher
            .clone()
            .ok_or("quick controls need a launcher")?;
        let quick = std::rc::Rc::new(quick);
        let projector = quick.clone();
        let initial = self.launcher_initial_projection.clone();
        let root = self.session.root().to_path_buf();
        self.session.set_projector(Box::new(move |stores| {
            let mut changes = if initial.replace(false) {
                menu.project_initial(stores, std::time::Duration::from_millis(20))?
            } else {
                menu.project(stores)?
            };
            changes.extend(projector.project(stores));
            if let Some(bead::core::Value::Text(query)) = stores.get("launcher.search.text") {
                changes.extend(crate::plugins::launcher_values(&root, query));
            }
            Ok(changes)
        }));
        self.actions.register("launcher.quick", "launcher")?;
        self.actions.register("plugins.do", "launcher")?;
        self.launcher_quick = Some(quick);
        Ok(self)
    }
    pub fn with_calendar(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let calendar = std::rc::Rc::new(crate::calendar::Calendar::new(source));
        let projector = calendar.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.calendar = Some(calendar);
        Ok(self)
    }
    pub fn with_status(
        self,
        kind: crate::status_views::Kind,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::new(kind, source))
    }
    pub fn with_core_network_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_network(source))
    }
    pub fn with_core_network_control(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        let mut runtime = self.with_status_projector(
            crate::status_views::Status::core_network_control(source, client),
        )?;
        runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([
            (
                "network.wifi_checked".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "network.wifi_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            ("network.wifi_busy".into(), bead::core::Value::Bool(false)),
            (
                "network.wifi_message".into(),
                bead::core::Value::Text(String::new()),
            ),
            ("network.rows".into(), bead::core::Value::list(Vec::new())),
            // B0095: the scroll position of the nearby-network list (the popout caps it at 210 px as QML does).
            ("network.scroll".into(), bead::core::Value::Number(0.)),
            (
                "network.height".into(),
                bead::core::Value::Number(crate::status_views::NETWORK_BASE_HEIGHT),
            ),
            (
                "network.link_visible".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "network.link_message".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "network.view".into(),
                bead::core::Value::Text("list".into()),
            ),
            (
                "network.prompt_label".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "network.form_error".into(),
                bead::core::Value::Text(String::new()),
            ),
            ("network.form_busy".into(), bead::core::Value::Bool(false)),
            ("network.can_hidden".into(), bead::core::Value::Bool(false)),
            ("network.can_forget".into(), bead::core::Value::Bool(false)),
            ("network.saved".into(), bead::core::Value::list(Vec::new())),
            (
                "network.scan_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            ("network.scanning".into(), bead::core::Value::Bool(false)),
            (
                "network.scan_label".into(),
                bead::core::Value::Text(String::new()),
            ),
        ]))?;
        for action in [
            "network.open",
            "network.cancel",
            "network.join",
            "network.forget_ask",
            "network.forget",
            "network.scan",
        ] {
            runtime.actions.register(action, "network.control")?;
        }
        runtime
            .actions
            .register("network.toggle_wifi", "network.control")?;
        runtime
            .actions
            .register("network.connect", "network.control")?;
        runtime
            .actions
            .register("network.disconnect", "network.control")?;
        runtime.actions.grant("network.control");
        Ok(runtime)
    }
    pub fn with_core_bluetooth_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_bluetooth(source))
    }
    pub fn with_core_bluetooth_control(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        let mut runtime = self.with_status_projector(
            crate::status_views::Status::core_bluetooth_control(source, client),
        )?;
        runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([
            (
                "bluetooth.power_visible".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_checked".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_busy".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_message".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.devices".into(),
                bead::core::Value::list(Vec::new()),
            ),
            (
                "bluetooth.list_message".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.scan_label".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.scan_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            ("bluetooth.scanning".into(), bead::core::Value::Bool(false)),
            (
                "bluetooth.empty_text".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.view".into(),
                bead::core::Value::Text("list".into()),
            ),
            (
                "bluetooth.pair_title".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.pair_kind".into(),
                bead::core::Value::Text("waiting".into()),
            ),
            (
                "bluetooth.pair_text".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.pair_countdown".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.pair_error".into(),
                bead::core::Value::Text(String::new()),
            ),
            ("bluetooth.pair_busy".into(), bead::core::Value::Bool(false)),
            (
                "bluetooth.pair_retry".into(),
                bead::core::Value::Bool(false),
            ),
        ]))?;
        for action in [
            "bluetooth.toggle_power",
            "bluetooth.pair",
            "bluetooth.scan",
            "bluetooth.link",
            "bluetooth.pair_respond",
            "bluetooth.pair_cancel",
            "bluetooth.pair_retry",
        ] {
            runtime.actions.register(action, "bluetooth.control")?;
        }
        runtime.actions.grant("bluetooth.control");
        Ok(runtime)
    }
    pub fn with_extras(mut self, extras: crate::extras::Extras) -> Result<Self, Box<dyn Error>> {
        let extras = std::rc::Rc::new(extras);
        let projector = extras.clone();
        let wake =
            std::sync::Arc::new(std::sync::Mutex::new(None::<crate::nacre_source::CallWake>));
        let callback = wake.clone();
        self.extras_wake = Some(wake);
        self.session.set_projector(Box::new(move |stores| {
            Ok(projector.project(stores, callback.lock().ok().and_then(|w| w.clone())))
        }));
        self.actions.register("extras.do", "extras.control")?;
        self.actions.grant("extras.control");
        let focus = if extras.kind == crate::extras::Kind::Wallpaper {
            ""
        } else {
            "extras.search"
        };
        self.extras = Some(extras);
        self.session.stores = self.session.stores.reconcile(&BTreeMap::from([(
            "bead.focused".into(),
            bead::core::Value::Text(focus.into()),
        )]))?;
        Ok(self)
    }
    /// B0093 selector prompt: the request as stores, the answer through the responder.
    pub fn with_prompt(mut self, prompt: crate::prompt::Prompt) -> Result<Self, Box<dyn Error>> {
        let prompt = std::rc::Rc::new(prompt);
        let projector = prompt.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("prompt.do", "prompt.control")?;
        self.actions.grant("prompt.control");
        let focus = if prompt.stores().get("prompt.kind")
            == Some(&bead::core::Value::Text("input".into()))
        {
            "prompt.value"
        } else {
            "prompt.search"
        };
        let mut initial = prompt.stores();
        initial.insert("bead.focused".into(), bead::core::Value::Text(focus.into()));
        self.session.stores = self.session.stores.reconcile(&initial)?;
        self.prompt = Some(prompt);
        Ok(self)
    }
    /// Session menu panel (B0023). Controls stay disabled until the daemon
    /// advertises `session.request` v1.
    pub fn with_session_menu(
        mut self,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
            || self.session_menu.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let menu = std::rc::Rc::new(crate::session_menu::SessionMenu::new(client));
        let projector = menu.clone();
        self.session
            .set_projector(Box::new(move |_| Ok(projector.project())));
        for name in ["session.select", "session.confirm", "session.cancel"] {
            self.actions.register(name, "session.control")?;
        }
        self.actions.grant("session.control");
        self.session_menu = Some(menu);
        Ok(self)
    }
    /// AI sidebar (B0043): the `ai.chat` topic of the edge channel and its published calls.
    pub fn with_chat(mut self, chat: crate::chat::Chat) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
            || self.session_menu.is_some()
            || self.chat.is_some()
            || self.control.is_some()
        {
            return Err("one data projector per panel".into());
        }
        self.session.stores = self.session.stores.reconcile(&chat.remembered())?;
        self.session.stores = self.session.stores.reconcile(&BTreeMap::from([(
            "bead.focused".into(),
            bead::core::Value::Text(String::new()),
        )]))?;
        if self.session.stores.get("panel.hover_opened") != Some(&bead::core::Value::Bool(true)) {
            self.session.stores.commit(BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text("chat.composer".into()),
            )]))?;
        }
        let chat = std::rc::Rc::new(chat);
        let projector = chat.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("chat.do", "chat.control")?;
        self.actions.grant("chat.control");
        if let Some(chrome) = &mut self.chrome {
            chrome.set_engaged(true);
        }
        self.chat = Some(chat);
        Ok(self)
    }
    /// B0045 Settings window: registry rows over the published store contract.
    pub fn with_settings(
        mut self,
        client: Option<crate::nacre_source::Client>,
        root: &std::path::Path,
    ) -> Result<Self, Box<dyn Error>> {
        if self.settings.is_some() {
            return Err("one data projector per panel".into());
        }
        let settings = std::rc::Rc::new(crate::settings::Settings::new(client, root)?);
        let projector = settings.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("settings.do", "settings.control")?;
        self.actions.grant("settings.control");
        self.settings = Some(settings);
        Ok(self)
    }
    /// B0060 Colors window: the explorer over the colours methods and the two topics it follows.
    pub fn with_colors(
        mut self,
        client: Option<crate::nacre_source::Client>,
    ) -> Result<Self, Box<dyn Error>> {
        if self.colors.is_some() {
            return Err("one data projector per panel".into());
        }
        let colors = std::rc::Rc::new(crate::colors::Colors::new(client));
        let projector = colors.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("colors.do", "colors.control")?;
        self.actions.grant("colors.control");
        self.colors = Some(colors);
        Ok(self)
    }
    /// B0059 Welcome window: app availability, shortcut rows and the window size over the tour view.
    pub fn with_welcome(
        mut self,
        welcome: crate::welcome::Welcome,
    ) -> Result<Self, Box<dyn Error>> {
        if self.welcome.is_some() {
            return Err("one data projector per panel".into());
        }
        let welcome = std::rc::Rc::new(welcome);
        let projector = welcome.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("welcome.do", "welcome.control")?;
        self.actions.grant("welcome.control");
        self.welcome = Some(welcome);
        Ok(self)
    }
    /// Control center (B0049): tile data sources, the stored tile order and its published write.
    pub fn with_control_center(
        mut self,
        control: crate::control_center::ControlCenter,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
            || self.session_menu.is_some()
            || self.chat.is_some()
            || self.control.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let control = std::rc::Rc::new(control);
        let projector = control.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.actions.register("view.open", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        self.actions.register("cc.do", "cc.control")?;
        self.actions.grant("cc.control");
        if let Some(chrome) = &mut self.chrome {
            chrome.set_engaged(true);
        }
        self.control = Some(control);
        Ok(self)
    }
    /// Compose the existing service adapters for the dock; no extra surface or runtime.
    /// Leases and adapter state die with the open control view.
    pub fn with_docked_details(
        mut self,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        self = self.with_core_network_control(
            std::sync::Arc::new(client.lease_topic("network")?),
            client.clone(),
        )?;
        self.dock_status.insert(
            "network".into(),
            self.status.take().ok_or("network adapter missing")?,
        );
        self = self.with_core_bluetooth_control(
            std::sync::Arc::new(client.lease_topic("bluetooth")?),
            client.clone(),
        )?;
        self.dock_status.insert(
            "bluetooth".into(),
            self.status.take().ok_or("Bluetooth adapter missing")?,
        );
        self =
            self.with_core_battery_status(std::sync::Arc::new(client.lease_topic("battery")?))?;
        self.dock_status.insert(
            "battery".into(),
            self.status.take().ok_or("battery adapter missing")?,
        );
        self = self.with_core_audio_status(
            std::sync::Arc::new(client.lease_topic("audio")?),
            Some(client.clone()),
        )?;
        self.dock_status.insert(
            "audio".into(),
            self.status.take().ok_or("audio adapter missing")?,
        );
        let control = self.control.clone().ok_or("control adapter required")?;
        let pages = self.dock_status.clone();
        self.session.set_projector(Box::new(move |stores| {
            let mut values = control.project(stores);
            if let Some(bead::core::Value::Text(page)) = stores.get("cc.detail")
                && stores.get("cc.page") == Some(&bead::core::Value::Text("detail".into()))
                && let Some(status) = pages.get(page)
            {
                values.extend(status.project());
                let focused = match stores.get("bead.focused") {
                    Some(bead::core::Value::Text(id)) => id.as_str(),
                    _ => "",
                };
                if let (Some(bead::core::Value::Text(prefix)), Some(bead::core::Value::Text(back))) =
                    (stores.get("cc.detail_focus_prefix"), stores.get("cc.back_focus"))
                    && !focused.starts_with(prefix)
                {
                    values.insert("bead.focused".into(), bead::core::Value::Text(back.clone()));
                }

                values.insert(
                    "cc.audio_devices".into(),
                    bead::core::Value::list(control.audio_devices()),
                );
                values.insert(
                    "cc.streams".into(),
                    bead::core::Value::list(control.audio_streams()),
                );
            } else {
                // Only the audio page needs the microphone meter (and its capture).
                for status in pages.values() {
                    status.park();
                }
            }
            Ok(values)
        }));
        Ok(self)
    }
    fn leave_docked_flow(&self) {
        for status in self.dock_status.values() {
            status.network_cancel();
            status.bluetooth_pair_leave();
        }
    }
    fn cleared_docked_secrets() -> BTreeMap<String, bead::core::Value> {
        BTreeMap::from([
            (
                "network.secret".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "network.hidden_ssid".into(),
                bead::core::Value::Text(String::new()),
            ),
            (
                "bluetooth.secret".into(),
                bead::core::Value::Text(String::new()),
            ),
            ("network.reveal".into(), bead::core::Value::Bool(false)),
            ("bluetooth.reveal".into(), bead::core::Value::Bool(false)),
        ])
    }
    fn active_status(&self) -> Option<std::rc::Rc<crate::status_views::Status>> {
        self.status.clone().or_else(|| {
            if self.session.stores.get("cc.page") != Some(&bead::core::Value::Text("detail".into()))
            {
                return None;
            }
            let Some(bead::core::Value::Text(page)) = self.session.stores.get("cc.detail") else {
                return None;
            };
            self.dock_status.get(page).cloned()
        })
    }
    /// Reuse the control centre's guarded calls with a status view and only its topic leases.
    pub fn with_status_controls(
        mut self,
        client: Option<crate::nacre_source::Client>,
        topics: Vec<(&'static str, std::sync::Arc<dyn bead::core::source::Source>)>,
    ) -> Result<Self, Box<dyn Error>> {
        let status = self.status.clone().ok_or("status projector required")?;
        let control = std::rc::Rc::new(crate::control_center::ControlCenter::new(
            client,
            topics,
            None,
            Vec::new(),
        ));
        let projector = control.clone();
        self.session.set_projector(Box::new(move |stores| {
            let mut values = status.project();
            values.extend(projector.project(stores).into_iter().filter(|(key, _)| {
                matches!(key.as_str(), "cc.src" | "cc.audio_devices" | "cc.message")
            }));
            values.insert(
                "cc.audio_devices".into(),
                bead::core::Value::list(projector.audio_devices()),
            );
            values.insert(
                "cc.streams".into(),
                bead::core::Value::list(projector.audio_streams()),
            );
            Ok(values)
        }));
        self.actions.register("cc.do", "cc.control")?;
        self.actions.grant("cc.control");
        self.control = Some(control);
        Ok(self)
    }
    pub fn with_core_battery_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_battery(source))
    }
    /// `peak` is the daemon client that leases the microphone meter while the view is shown.
    pub fn with_core_audio_status(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        peak: Option<crate::nacre_source::Client>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut status = crate::status_views::Status::core_audio(source);
        if let Some(client) = peak {
            status = status.with_audio_peak(client);
        }
        self.session.stores = self.session.stores.reconcile(&status.project())?;
        self.with_status_projector(status)
    }
    pub fn with_core_updates_status(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        self = self.with_status_projector(crate::status_views::Status::core_updates(
            source,
            Some(client),
        ))?;
        for action in ["updates.refresh", "updates.open"] {
            self.actions.register(action, "updates.control")?;
        }
        self.actions.grant("updates.control");
        Ok(self)
    }
    fn with_status_projector(
        mut self,
        status: crate::status_views::Status,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let status = std::rc::Rc::new(status);
        let projector = status.clone();
        self.session
            .set_projector(Box::new(move |_| Ok(projector.project())));
        self.status = Some(status);
        Ok(self)
    }
    #[cfg(test)]
    pub fn with_dashboard(
        self,
        page: crate::dashboard::Page,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        updates: Option<std::sync::Arc<dyn bead::core::source::Source>>,
        clock: Option<std::sync::Arc<dyn bead::core::source::Source>>,
        preview: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_dashboard_view(
            crate::dashboard::Dashboard::new(page, source, updates, preview).with_clock(clock),
        )
    }
    pub fn with_dashboard_view(
        mut self,
        dashboard: crate::dashboard::Dashboard,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let dashboard = std::rc::Rc::new(std::cell::RefCell::new(dashboard));
        let projector = dashboard.clone();
        let projected_revision = std::rc::Rc::new(std::cell::Cell::new(None));
        let cached_revision = projected_revision.clone();
        self.session.set_projector(Box::new(move |stores| {
            let revision = projector.borrow().projection_revision(stores);
            if cached_revision.get() == Some(revision) {
                return Ok(BTreeMap::new());
            }
            let changes = projector.borrow().project_with_stores(stores);
            cached_revision.set(Some(
                projector
                    .borrow()
                    .projection_revision_after(stores, revision, &changes),
            ));
            Ok(changes)
        }));
        self.actions.register("view.open", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        self.actions.register("dashboard.select", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        self.actions.register("dashboard.do", "dashboard.control")?;
        self.actions.grant("dashboard.control");
        if let Some(chrome) = &mut self.chrome {
            chrome.set_engaged(true);
        }
        self.dashboard = Some(dashboard);
        Ok(self)
    }
    #[cfg(test)]
    pub(crate) fn test_reload_events(&self) -> usize {
        self.reload_events
    }
    pub fn with_dashboard_factory(
        mut self,
        dashboard: crate::dashboard::Dashboard,
        factory: crate::dashboard::Factory,
    ) -> Result<Self, Box<dyn Error>> {
        self = self.with_dashboard_view(dashboard)?;
        self.dashboard_factory = Some(factory);
        Ok(self)
    }
    pub fn with_launcher_memory(
        mut self,
        memory: crate::launcher::Memory,
    ) -> Result<Self, Box<dyn Error>> {
        let saved = memory
            .lock()
            .map_err(|_| "launcher view state unavailable")?
            .clone();
        self.session.stores = self.session.stores.reconcile(&saved)?;
        self.launcher_memory = Some(memory);
        Ok(self)
    }
    pub fn with_icon_warmer(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        palette: std::path::PathBuf,
    ) -> Self {
        self.icon_warmer = Some(crate::icon_warm::IconWarmer::new(source, palette));
        self
    }
    pub fn with_wallpaper_warmer(
        mut self,
        warmer: std::sync::Arc<crate::wallpaper_warm::Warmer>,
    ) -> Self {
        self.wallpaper_warmer = Some(warmer);
        self
    }
    pub fn with_launcher_trigger(
        mut self,
        memory: crate::launcher::Memory,
    ) -> Result<Self, Box<dyn Error>> {
        self.launcher_trigger = true;
        self.launcher_memory = Some(memory);
        self.actions.register("launcher.open", "launcher.view")?;
        self.actions.grant("launcher.view");
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.launcher".into(),
                bead::core::Value::Bool(true),
            )]))?;
        Ok(self)
    }
    /// B0022 daemon bar routes (N0031). The bar's workspace, title, pill and tray
    /// actions become output-scoped daemon calls, gated on the welcome registry,
    /// and the tray list comes from the daemon `tray` topic. Bar-only; the
    /// local compositor/tray action paths stay for daemon-less preview runs.
    pub fn with_bar_routes(
        mut self,
        client: crate::nacre_source::Client,
        tray: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self = self.with_bar_daemon(&client)?;
        let routes = crate::bar_routes::Routes::new(client);
        let mut defaults = routes.stores();
        defaults.insert(
            "bead.desktop.tray".into(),
            bead::core::Value::list(crate::bar_routes::tray_rows(
                &crate::bar_routes::tray_items(&tray.snapshot().data),
            )),
        );
        *self.daemon_tray.lock().unwrap_or_else(|e| e.into_inner()) =
            crate::bar_routes::tray_items(&tray.snapshot().data);
        self.session.stores = self.session.stores.reconcile(&defaults)?;
        self.actions.register("bar.title", "bar.routes")?;
        self.actions.grant("bar.routes");
        self.routes = Some(std::rc::Rc::new(routes));
        self.tray_topic = Some(tray);
        Ok(self)
    }
    /// The desktop snapshot with the daemon's tray rows when the bar uses them.
    fn with_daemon_tray(
        &self,
        snapshot: std::sync::Arc<bead::services::Snapshot>,
    ) -> std::sync::Arc<bead::services::Snapshot> {
        if self.tray_topic.is_none() {
            return snapshot;
        }
        let mut merged = (*snapshot).clone();
        merged.tray = self
            .daemon_tray
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        std::sync::Arc::new(merged)
    }
    /// Which daemon route, if any, an event on a bar control means. The route is
    /// chosen from the event and control alone; availability is the daemon's
    /// advertised registry, checked when the call is made.
    fn bar_route(
        &self,
        event: &UiEvent,
        id: &str,
        name: &str,
        arg: &bead::core::Value,
    ) -> Option<crate::bar_routes::Route> {
        use crate::bar_routes::Route;
        let text = match arg {
            bead::core::Value::Text(text) => text.as_str(),
            _ => "",
        };
        let press = matches!(event, UiEvent::Click { .. } | UiEvent::Release)
            || matches!(event, UiEvent::Command { command, .. } if command == "enter");
        let wheel_up = |delta: f64| delta > 0.;
        let tray = |method| {
            let x = self.popout_center(id, "tray").unwrap_or(0.);
            Route::Tray {
                method,
                item: text.into(),
                x: x.round().clamp(0., 32767.) as i16,
                y: 25,
            }
        };
        match (name, event) {
            ("backend.workspace.switch", _) if press => {
                // This output's own row model (B0025), never another output's.
                let Some(bead::core::Value::List(rows)) =
                    self.session.stores.get("bead.workspaces")
                else {
                    return None;
                };
                let index = rows.iter().find(|w| w["id"] == text)?["index"].as_i64()?;
                Some(Route::WorkspaceActivate {
                    slot: u16::try_from(index)
                        .ok()
                        .filter(|i| (1..=255).contains(i))?,
                })
            }
            ("backend.window.focus", _) if press => {
                // Glyphs carry the local id; the daemon knows windows by its own.
                crate::workspace_rows::daemon_window_id(text)
                    .map(|window| Route::WindowFocus { window })
            }
            ("backend.workspace.switch", UiEvent::Scroll { delta, .. }) => {
                Some(Route::WorkspaceScroll {
                    forward: !wheel_up(*delta),
                })
            }
            ("bar.title" | "services.audio", UiEvent::Scroll { delta, .. }) => {
                Some(Route::Volume {
                    up: wheel_up(*delta),
                })
            }
            ("status.pill.open", UiEvent::Scroll { delta, .. }) if text == "battery" => {
                Some(Route::Brightness {
                    up: wheel_up(*delta),
                })
            }
            ("status.pill.open" | "services.audio", UiEvent::SecondaryClick { .. })
                if text.is_empty() || text == "audio" =>
            {
                Some(Route::Mute)
            }
            ("services.tray", _) if press => {
                let menu = self
                    .daemon_tray
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .any(|item| item.id == text && item.item_is_menu);
                Some(tray(if menu {
                    "tray.context_menu"
                } else {
                    "tray.activate"
                }))
            }
            ("services.tray", UiEvent::ContextMenu { .. }) => Some(tray("tray.context_menu")),
            ("services.tray.secondary", UiEvent::SecondaryClick { .. }) => {
                Some(tray("tray.secondary_activate"))
            }
            ("services.tray", UiEvent::Scroll { delta, .. }) => Some(Route::TrayScroll {
                item: text.into(),
                horizontal: false,
                up: wheel_up(*delta),
            }),
            ("services.tray", UiEvent::HorizontalScroll { delta, .. }) => Some(Route::TrayScroll {
                item: text.into(),
                horizontal: true,
                up: wheel_up(*delta),
            }),
            _ => None,
        }
    }
    /// The focused output, else the first one (the QML `NacreBrightness.focused`).
    fn focused_output(&self) -> Option<String> {
        let snapshot = self.backend.as_ref()?.snapshot();
        snapshot
            .outputs
            .iter()
            .find(|output| output.focused)
            .or(snapshot.outputs.first())
            .map(|output| output.name.clone())
    }
    /// `brightness step|set` and `keyboardLight step` keys; disabled until the daemon advertises `brightness.adjust` v1.
    pub fn with_brightness_keys(
        mut self,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        self.actions.register("key.brightness", "key.control")?;
        self.actions.grant("key.control");
        self.brightness = Some(crate::key_routes::Brightness::new(client));
        Ok(self)
    }
    pub fn with_popout_trigger(mut self) -> Result<Self, Box<dyn Error>> {
        self.actions.register("view.open", "ui.popouts")?;
        self.actions.register("status.pill.open", "ui.popouts")?;
        self.actions.register("widgets.edit", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.popouts".into(),
                bead::core::Value::Bool(true),
            )]))?;
        Ok(self)
    }
    fn popout_center(&self, id: &str, view: &str) -> Option<f32> {
        // Top-bar popouts center on their invoking control, including keyboard activation.
        ((self.bar
            && matches!(
                view,
                "calendar" | "network" | "bluetooth" | "battery" | "sound" | "tray"
            ))
            || (self.bar && view.starts_with("peek-")))
        .then(|| {
            self.session
                .view_nodes()
                .iter()
                .find(|n| n.id == id)
                .map(|node| {
                    let rect = bead::ui::rect(&node.draw);
                    rect.x + rect.w / 2.
                })
        })
        .flatten()
    }
    pub fn with_trace(mut self, trace: bool) -> Self {
        self.trace = trace;
        self
    }
    pub fn with_notifications(
        self,
        socket: std::path::PathBuf,
        history: std::path::PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_notification_service(std::sync::Arc::new(crate::notifications::Owner::start(
            socket, history,
        )?))
    }
    /// The primary notice session over a private owner or the daemon consumer.
    pub fn with_notification_service(
        mut self,
        service: std::sync::Arc<dyn crate::notifications::Notices>,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::notice_service::seed())?;
        for name in crate::notice_service::ACTIONS {
            self.actions.register(name, "notifications")?;
        }
        self.actions.grant("notifications");
        self.notification_primary = true;
        self.state_file = None;
        self.notifications = Some(service);
        Ok(self)
    }
    /// A bar session of the main shell: offers the daemon notice/OSD panel views and
    /// answers `bead.osd`/`notifs` commands. It shows no notice content itself.
    pub fn with_notice_host(
        mut self,
        service: std::sync::Arc<dyn crate::notifications::Notices>,
    ) -> Result<Self, Box<dyn Error>> {
        // The notice driver can publish before the bar's first markup load.
        self.session.stores = self.session.stores.reconcile(&BTreeMap::from([(
            "bar.notifications.count".into(),
            bead::core::Value::Number(0.),
        )]))?;
        self.actions
            .register("notifications.feedback", "notifications")?;
        self.actions.grant("notifications");
        self.notice_host = Some(service);
        Ok(self)
    }
    pub fn notification_owner(&self) -> Option<std::sync::Arc<dyn crate::notifications::Notices>> {
        self.notifications.clone()
    }
    pub fn with_notification_owner(
        mut self,
        owner: std::sync::Arc<dyn crate::notifications::Notices>,
        open: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::notice_service::seed())?;
        for name in crate::notice_service::ACTIONS {
            self.actions.register(name, "notifications")?;
        }
        self.actions.grant("notifications");
        self.state_file = None;
        self.session
            .restore_on_start(std::collections::BTreeMap::from([(
                "panel.open".into(),
                bead::core::Value::Bool(open),
            )]));
        self.notifications = Some(owner);
        Ok(self)
    }
    pub fn with_tray_bar(mut self, selection: crate::tray::Selection, enabled: bool) -> Self {
        self.tray_enabled = enabled;
        self.tray_selection = enabled.then_some(selection);
        self
    }
    pub fn with_tray_menu(
        mut self,
        selection: crate::tray::Selection,
    ) -> Result<Self, Box<dyn Error>> {
        let mut menu = crate::tray::Menu::new(selection);
        let snapshot = self
            .desktop
            .as_ref()
            .ok_or("desktop unavailable")?
            .snapshot();
        self.session.stores = self.session.stores.reconcile(&menu.values(&snapshot))?;
        self.tray_menu = Some(menu);
        Ok(self)
    }
    pub fn with_desktop(self) -> Result<Self, Box<dyn Error>> {
        self.with_shared_desktop(bead::services::Desktop::start(), true)
    }
    pub fn desktop_owner(&self) -> Option<std::sync::Arc<bead::services::Desktop>> {
        self.desktop.clone()
    }
    pub fn with_shared_desktop(
        mut self,
        desktop: std::sync::Arc<bead::services::Desktop>,
        bar: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::desktop::values(&desktop.snapshot(), ""))?;
        if bar {
            let mut defaults = BTreeMap::new();
            for (key, value) in [
                ("bead.focused_title", bead::core::Value::Text(String::new())),
                ("bead.workspaces", bead::core::Value::list(vec![])),
                ("bead.windows", bead::core::Value::list(vec![])),
            ] {
                if self.session.stores.get(key).is_none() {
                    defaults.insert(key.into(), value);
                }
            }
            self.session.stores = self.session.stores.reconcile(&defaults)?;
        }
        if bar && !self.tray_enabled {
            self.session.stores = self.session.stores.reconcile(&BTreeMap::from([(
                "bead.desktop.tray".into(),
                bead::core::Value::list(vec![]),
            )]))?;
        }
        for name in [
            "services.audio",
            "services.audio.volume",
            "services.tray",
            "services.menu",
            "services.menu.open",
            "services.menu.back",
            "services.tray.secondary",
        ] {
            self.actions.register(name, "desktop.services")?;
        }
        self.actions.grant("desktop.services");
        self.desktop = Some(desktop);
        self.bar = bar;
        if bar {
            // Durable panel pages are loaded by their own worker on first open.
            // The bar must not retain either startup copy after configuration.
            self.state_file = None;
            self.session.restore_on_start(Default::default());
        }
        Ok(self)
    }
    fn desktop_action(
        &mut self,
        name: &str,
        arg: bead::core::Value,
        delta: Option<f64>,
        context_menu: bool,
        horizontal: bool,
    ) -> Result<(), Box<dyn Error>> {
        self.actions.authorize(name)?;
        use bead::services::Action;
        let desktop = self.desktop.as_ref().ok_or("desktop unavailable")?;
        let snapshot = desktop.snapshot();
        let action = match name {
            "services.audio.volume" => {
                let bead::core::Value::Number(value) = arg else {
                    return Err("volume requires a number".into());
                };
                return desktop
                    .queue_volume((value / 100.).clamp(0., 1.))
                    .map_err(|e| e.into());
            }
            "services.audio" => {
                if let Some(delta) = delta {
                    Action::Volume(
                        (snapshot.audio.volume.ok_or("audio unavailable")? + delta.signum() * 0.05)
                            .clamp(0., 1.),
                    )
                } else {
                    Action::Mute(!snapshot.audio.muted.ok_or("audio unavailable")?)
                }
            }
            "services.tray" => {
                if !self.tray_enabled {
                    return Err("tray producer unavailable".into());
                }
                let bead::core::Value::Text(item) = arg else {
                    return Err("tray ID required".into());
                };
                let tray = snapshot
                    .tray
                    .iter()
                    .find(|t| t.id == item)
                    .ok_or("stale tray ID")?;
                if let Some(delta) = delta {
                    Action::TrayScroll {
                        item,
                        delta: if delta > 0. { -120 } else { 120 },
                        horizontal,
                    }
                } else if tray.item_is_menu || context_menu {
                    if !tray.has_menu {
                        return desktop
                            .action(Action::TrayContextMenu { item, x: 0, y: 0 })
                            .map_err(|e| e.into());
                    }
                    if let Some(selection) = &self.tray_selection {
                        desktop
                            .action(Action::TrayPrepare { item: item.clone() })
                            .map_err(|e| -> Box<dyn Error> { e.into() })?;
                        *selection.lock().unwrap_or_else(|e| e.into_inner()) = item.clone();
                        self.wakeup
                            .as_ref()
                            .ok_or("tray event channel unavailable")?(
                            UiEvent::CompanionSelect {
                                view: "tray".into(),
                                output: None,
                                toggle: false,
                                center: self.tray_anchor,
                            },
                        );
                        self.wakeup
                            .as_ref()
                            .ok_or("tray event channel unavailable")?(
                            UiEvent::CompanionModel(Default::default()),
                        );
                        return Ok(());
                    } else {
                        self.menu_item = if self.menu_item == item {
                            String::new()
                        } else {
                            item.clone()
                        };
                    }
                    Action::TrayPrepare { item }
                } else {
                    Action::TrayActivate { item, x: 0, y: 0 }
                }
            }
            "services.tray.secondary" => {
                let bead::core::Value::Text(item) = arg else {
                    return Err("tray ID required".into());
                };
                Action::TraySecondaryActivate { item, x: 0, y: 0 }
            }
            "services.menu.back" => {
                self.tray_menu
                    .as_mut()
                    .ok_or("tray menu unavailable")?
                    .back();
                return Ok(());
            }
            "services.menu.open" => {
                let bead::core::Value::Text(value) = arg else {
                    return Err("menu ID required".into());
                };
                let (item, entry) = value.rsplit_once('|').ok_or("menu ID required")?;
                desktop
                    .action(Action::TrayPrepareEntry {
                        item: item.into(),
                        entry: entry.parse()?,
                    })
                    .map_err(|e| -> Box<dyn Error> { e.into() })?;
                if !self
                    .tray_menu
                    .as_mut()
                    .ok_or("tray menu unavailable")?
                    .navigate(&desktop.snapshot(), entry.parse()?)
                {
                    return Err("stale submenu".into());
                }
                return Ok(());
            }
            "services.menu" => {
                let bead::core::Value::Text(value) = arg else {
                    return Err("menu ID required".into());
                };
                let (item, entry) = value.rsplit_once('|').ok_or("menu ID required")?;
                self.menu_item.clear();
                if self.tray_menu.is_some() {
                    self.tray_close = true;
                }
                Action::TrayMenu {
                    item: item.into(),
                    entry: entry.parse()?,
                }
            }
            _ => return Err("unknown desktop action".into()),
        };
        desktop
            .action(action)
            .map_err(|e| -> Box<dyn Error> { e.into() })
    }
    pub fn with_backend(
        mut self,
        backend: std::sync::Arc<crate::backends::Native>,
    ) -> Result<Self, Box<dyn Error>> {
        self.attach_backend(backend)?;
        Ok(self)
    }
    fn attach_backend(
        &mut self,
        backend: std::sync::Arc<crate::backends::Native>,
    ) -> Result<(), Box<dyn Error>> {
        let snapshot = backend.snapshot();
        let mut values = backend_values(&snapshot, self.output.as_deref(), &self.workspaces)?;
        let (title, focused_window) = focused_title(&snapshot, self.output.as_deref(), None);
        self.last_focused_window = focused_window;
        values.insert("bead.focused_title".into(), bead::core::Value::Text(title));
        self.session.stores = self.session.stores.reconcile(&values)?;
        for name in [
            "backend.workspace.switch",
            "backend.window.focus",
            "backend.window.close",
            "backend.windowTrash",
            "backend.scratchpad",
            "backend.windowCapture",
        ] {
            self.actions.register(name, "desktop.compositor")?;
        }
        self.actions.grant("desktop.compositor");
        self.backend = Some(backend);
        self.subscribe_backend();
        Ok(())
    }
    fn subscribe_backend(&mut self) {
        if self.subscription.is_some() {
            return;
        }
        let (Some(backend), Some(wakeup)) = (self.backend.as_ref(), self.wakeup.clone()) else {
            return;
        };
        self.subscription = Some(backend.subscribe(Box::new(move |event| {
            if let bead::backends::Event::Snapshot(snapshot) = event {
                wakeup(UiEvent::Backend(snapshot));
            }
        })));
    }
    fn backend_action(&self, name: &str, arg: bead::core::Value) -> Result<(), Box<dyn Error>> {
        self.actions.authorize(name)?;
        let backend = self.backend.as_ref().ok_or("backend unavailable")?;
        let bead::core::Value::Text(id) = arg else {
            return Err("backend action requires a stable ID".into());
        };
        let id = bead::backends::Id(id);
        let action = match name {
            "backend.workspace.switch" => bead::backends::Action::SwitchWorkspace { workspace: id },
            "backend.window.focus" => bead::backends::Action::FocusWindow { window: id },
            "backend.window.close" => bead::backends::Action::CloseWindow { window: id },
            "backend.windowTrash" => bead::backends::Action::WindowTrash { window: id },
            "backend.scratchpad" => bead::backends::Action::Scratchpad { window: Some(id) },
            "backend.windowCapture" => bead::backends::Action::WindowCapture { window: id },
            _ => {
                return Err(bead::backends::Error::Unsupported {
                    operation: name.into(),
                    backend: backend.snapshot().backend.clone(),
                }
                .into());
            }
        };
        let started = std::time::Instant::now();
        backend.action(action)?;
        println!(
            "BACKEND_ACTION name={name} request_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(())
    }
}

fn panel_route_name(stores: &bead::core::Stores) -> String {
    for key in ["panel.route", "panel.page"] {
        if let Some(bead::core::Value::Text(route)) = stores.get(key)
            && !route.is_empty()
        {
            return route.clone();
        }
    }
    "panel".into()
}
fn suppress_startup_bar_jank(bar: bool, user_interacted: bool, age: std::time::Duration) -> bool {
    bar && !user_interacted && age < std::time::Duration::from_millis(750)
}

#[cfg(test)]
mod dismissed_owner_tests {
    use super::consume_pending_owner_dismiss;
    use std::time::{Duration, Instant};

    #[test]
    fn consumes_only_a_matching_owner_release_within_its_pointer_press() {
        let now = Instant::now();
        let mut pending = Some(("battery".into(), now));
        assert!(consume_pending_owner_dismiss(
            &mut pending,
            "battery",
            now + Duration::from_millis(100)
        ));
        assert!(pending.is_none());

        let mut pending = Some(("battery".into(), now));
        assert!(!consume_pending_owner_dismiss(
            &mut pending,
            "bluetooth",
            now + Duration::from_millis(100)
        ));
        assert!(pending.is_none());

        let mut pending = Some(("battery".into(), now));
        assert!(!consume_pending_owner_dismiss(
            &mut pending,
            "battery",
            now + Duration::from_millis(251)
        ));
    }
}

#[cfg(test)]
mod panel_route_name_tests {
    use super::panel_route_name;
    use bead::core::{Stores, Value};
    use std::collections::BTreeMap;

    #[test]
    fn uses_route_then_page_and_falls_back_for_unlabelled_panels() {
        let stores = |pairs: &[(&str, Value)]| {
            Stores::default()
                .reconcile(
                    &pairs
                        .iter()
                        .map(|(key, value)| ((*key).into(), value.clone()))
                        .collect::<BTreeMap<_, _>>(),
                )
                .unwrap()
        };
        assert_eq!(
            panel_route_name(&stores(&[(
                "panel.route",
                Value::Text("dropdown:audio".into())
            )])),
            "dropdown:audio"
        );
        assert_eq!(
            panel_route_name(&stores(&[("panel.page", Value::Text("output".into()))])),
            "output"
        );
        assert_eq!(
            panel_route_name(&stores(&[
                ("panel.route", Value::Text("dropdown:audio".into())),
                ("panel.page", Value::Text("output".into()))
            ])),
            "dropdown:audio"
        );
        assert_eq!(
            panel_route_name(&stores(&[("panel.route", Value::Text("picker".into()))])),
            "picker"
        );
        assert_eq!(panel_route_name(&Stores::default()), "panel");
    }
}
#[cfg(test)]
mod startup_bar_jank_tests {
    use super::suppress_startup_bar_jank;
    use std::time::Duration;

    #[test]
    fn startup_suppression_ends_after_input_or_the_grace_period() {
        assert!(suppress_startup_bar_jank(
            true,
            false,
            Duration::from_millis(300)
        ));
        assert!(!suppress_startup_bar_jank(
            true,
            true,
            Duration::from_millis(300)
        ));
        assert!(!suppress_startup_bar_jank(
            true,
            false,
            Duration::from_millis(750)
        ));
        assert!(!suppress_startup_bar_jank(
            false,
            false,
            Duration::from_millis(300)
        ));
    }
}
impl Runtime {
    pub fn with_frame(
        mut self,
        attachment: crate::placement::Attachment,
        colors: &bead::render::Palette,
    ) -> Result<Self, Box<dyn Error>> {
        self.chrome = Some(crate::placement::Chrome::new(attachment, colors)?);
        // Only panels that can float need the second look.
        self.spare_chrome = match attachment {
            crate::placement::Attachment::Attached(_)
            | crate::placement::Attachment::SceneEdge(_)
            | crate::placement::Attachment::Popout
            | crate::placement::Attachment::ControlCenter => Some(crate::placement::Chrome::new(
                crate::placement::Attachment::Pearl,
                colors,
            )?),
            crate::placement::Attachment::Pearl => None,
        };
        Ok(self)
    }
    pub fn with_view_edge(mut self, edge: &str) -> Result<Self, String> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&BTreeMap::from([(
                "view.edge".into(),
                bead::core::Value::Text(edge.into()),
            )]))
            .map_err(|error| error.to_string())?;
        Ok(self)
    }
    pub fn with_panel_geometry(
        mut self,
        width: f64,
        height: f64,
        duration: f64,
    ) -> Result<Self, String> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&BTreeMap::from([
                ("view.width".into(), bead::core::Value::Number(width)),
                ("view.height".into(), bead::core::Value::Number(height)),
                ("view.duration".into(), bead::core::Value::Number(duration)),
            ]))
            .map_err(|error| error.to_string())?;
        Ok(self)
    }
    fn finish_scene(&mut self, mut scene: Option<Scene>) -> Result<Option<Scene>, Box<dyn Error>> {
        if std::mem::take(&mut self.host_release_pending) {
            // Native host handlers own this release. Re-stage from their
            // latest candidate so the final scene includes both the side
            // effect and cleared pressed feedback, without replaying Luau.
            scene = self.session.release_without_activation()?.or(scene);
        }
        if self.popup_anchor_changed && scene.is_none() {
            scene = self.session.relayout()?;
        }
        self.popup_anchor_changed = false;
        if let Some(scene) = &mut scene {
            if let Some(anchor) = self.popup_anchor
                && let Some(motion) = &mut scene.motion
            {
                motion.edge = anchor.edge;
            }
            // Canvas surfaces are positioned by content bounds in output coordinates, so the
            // layout's 32 px shadow margin must not shift their content (or the drag overlay's
            // zones and ghost) away from the output-local hit tests.
            if self.widget_canvas || self.widget_drag_overlay {
                scene.offset = (0., 0.);
            }
            if let Some(base) = self
                .desktop
                .as_ref()
                .map(|desktop| desktop.snapshot())
                .or_else(|| self.tray_topic.is_some().then(Default::default))
            {
                crate::desktop::icons(scene, &self.with_daemon_tray(base))?;
            }
            if let Some(dashboard) = &self.dashboard {
                dashboard.borrow().attach_art(scene);
            }
            if let Some(colors) = &self.colors {
                colors.attach_images(scene);
            }
            if let Some(settings) = &self.settings {
                settings.attach_images(scene);
            }
            if let Some(extras) = &self.extras {
                let wake = self
                    .extras_wake
                    .as_ref()
                    .and_then(|slot| slot.lock().ok().and_then(|wake| wake.clone()));
                extras.attach_images(scene, wake);
            }
            if let Some(welcome) = &self.welcome {
                welcome.attach_images(scene);
            }
            let asset_profile = bead::core::profile::Span::new("assets");
            for draw in &mut scene.primitives {
                if let bead::render::Primitive::Image { path, .. } = draw
                    && let Some(icon) = path.to_str().and_then(|s| s.strip_prefix("@icon:"))
                {
                    *path = if std::env::var_os("BEAD_MATCHED_ICONS").is_some() {
                        std::env::var_os("BEAD_ICON_PATH")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| "@bead-icon-fallback.svg".into())
                    } else {
                        resolve_icon_path(icon)
                    };
                    let fallback = || {
                        std::sync::Arc::from(include_bytes!("../../../shell/sample.svg").as_slice())
                    };
                    let data = if path.as_path() == std::path::Path::new("@bead-icon-fallback.svg")
                    {
                        fallback()
                    } else if let Ok(bytes) = std::fs::read(&*path)
                        && bead::render::validate_asset(path, &bytes).is_ok()
                    {
                        std::sync::Arc::from(bytes)
                    } else {
                        *path = "@bead-icon-fallback.svg".into();
                        fallback()
                    };
                    scene.asset_data.insert(path.clone(), data);
                }
            }
            drop(asset_profile);
            // Service text can introduce a family/codepoint after an initially empty title.
            scene.font_files = self.fonts.select(scene)?;
        }
        if let Some(scene) = &mut scene {
            if self.bar {
                scene.offset = (0., 0.);
                scene.slide = None;
                scene.bounds = bead::render::Rect::new(0., 0., scene.width, 50.);
            }
            let editor_profile = bead::core::profile::Span::new("editor_shape");
            self.session.decorate_inputs(scene)?;
            drop(editor_profile);
            let chrome = if self.popup_anchor.is_some() {
                self.spare_chrome.as_ref().or(self.chrome.as_ref())
            } else {
                self.chrome.as_ref()
            };
            if let Some(chrome) = chrome
                && self.session.resolve("view.frame").ok() != Some(bead::core::Value::Bool(false))
            {
                chrome.decorate(scene)?;
            }
        }
        self.session.check_sources()?;
        if self
            .extras
            .as_ref()
            .is_some_and(|extras| !extras.content_ready())
        {
            if let Some(scene) = scene {
                self.session.commit(scene);
            }
            return Ok(None);
        }
        if scene
            .as_ref()
            .and_then(|scene| scene.motion.as_ref())
            .is_some_and(|motion| !motion.open)
            && self.session.stores.get("panel.open") == Some(&bead::core::Value::Bool(true))
            && let Some(wakeup) = &self.wakeup
        {
            wakeup(UiEvent::CompanionVisibility(false));
        }
        self.pending = scene.clone();
        if scene.is_none() {
            // A transfer can change only model data; its ghost still needs the newest point.
            self.publish_widget_models();
        }
        Ok(scene)
    }
}
/// Icon names resolve through the theme directories (stat-heavy); a launcher grid asks for the same
/// few dozen names on every scene, so remember the answer. Bounded; only paths are kept, never bytes.
pub(crate) fn resolve_icon_path(icon: &str) -> std::path::PathBuf {
    let direct = std::path::Path::new(icon);
    if direct.is_absolute() && direct.is_file() {
        return direct.to_path_buf();
    }
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static PATHS: OnceLock<Mutex<HashMap<String, std::path::PathBuf>>> = OnceLock::new();
    let cache = PATHS.get_or_init(Default::default);
    // A package removal must not leave a dead path behind: one stat revalidates a hit.
    if let Some(path) = cache
        .lock()
        .ok()
        .and_then(|map| map.get(icon).cloned())
        .filter(|path| path.exists())
    {
        return path;
    }
    // B0187: an unresolvable name cost ~12 ms of theme scans on every scene build (every keystroke and
    // scroll step). Remember misses briefly; a newly installed icon appears within the TTL.
    static MISSES: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    let misses = MISSES.get_or_init(Default::default);
    if misses.lock().is_ok_and(|map| {
        map.get(icon)
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60))
    }) {
        return "@bead-icon-fallback.svg".into();
    }
    let direct = std::path::PathBuf::from(icon);
    static THEME: OnceLock<String> = OnceLock::new();
    let theme = THEME
        .get_or_init(|| freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "Papirus".into()));
    let path = direct
        .is_absolute()
        .then_some(direct)
        .filter(|path| path.is_file())
        .or_else(|| {
            [theme.as_str(), "Papirus", "hicolor", "Adwaita"]
                .into_iter()
                .find_map(|theme| {
                    freedesktop_icons::lookup(icon)
                        .with_theme(theme)
                        .with_size(48)
                        .find()
                })
        })
        .or_else(|| {
            if icon.is_empty() || std::path::Path::new(icon).components().count() != 1 {
                return None;
            }
            let roots = std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            std::env::split_paths(&roots).find_map(|root| {
                ["svg", "png", "xpm"]
                    .into_iter()
                    .map(|extension| root.join("icons").join(format!("{icon}.{extension}")))
                    .find(|path| path.is_file())
            })
        })
        .unwrap_or_else(|| "@bead-icon-fallback.svg".into());
    if path.as_path() != std::path::Path::new("@bead-icon-fallback.svg") {
        if let Ok(mut map) = cache.lock() {
            if map.len() >= 4096 {
                map.clear();
            }
            map.insert(icon.to_owned(), path.clone());
        }
    } else if let Ok(mut map) = misses.lock() {
        if map.len() >= 4096 {
            map.clear();
        }
        map.insert(icon.to_owned(), std::time::Instant::now());
    }
    path
}

#[cfg(test)]
mod icon_miss_tests {
    #[test]
    fn unresolvable_icon_names_are_remembered_instead_of_rescanned() {
        let name = "bead-b0187-no-such-icon-anywhere";
        assert_eq!(
            super::resolve_icon_path(name).as_path(),
            std::path::Path::new("@bead-icon-fallback.svg")
        );
        let started = std::time::Instant::now();
        for _ in 0..50 {
            super::resolve_icon_path(name);
        }
        assert!(started.elapsed() < std::time::Duration::from_millis(20));
    }
}
impl Runtime {
    fn pointer_feedback(
        &mut self,
        id: &str,
        event: &UiEvent,
        local: bool,
    ) -> Result<Option<Scene>, bead::ui::Diagnostic> {
        let feedback = |session: &mut bead::ui::Session| match event {
            UiEvent::Click { x, .. } => session.pointer_focus_id(id, true, Some(*x)),
            UiEvent::Drag { x, .. } => session.pointer_focus_id(id, false, Some(*x)),
            UiEvent::Command { command, .. } if command == "left" || command == "right" => {
                session.range_step_id(id)
            }
            _ => session.focus_id(id),
        };
        // Async services publish their model separately. Pure range feedback can
        // draw immediately; synchronous local models (e.g. colors) still project.
        if local && self.session.has_preview(id) {
            self.session.with_local_feedback(feedback)
        } else {
            feedback(&mut self.session)
        }
    }
}

impl Runtime {
    pub(crate) fn select_dashboard_page(
        &mut self,
        page: crate::dashboard::Page,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        let Some(current) = &self.dashboard else {
            return Ok(None);
        };
        if current.borrow().page() == page {
            return Ok(None);
        }
        let Some(factory) = self.dashboard_factory.clone() else {
            return Ok(None);
        };
        let mut changes = self.session.action_values(
            "dashboard.select",
            bead::core::Value::Text(page.name().into()),
        )?;
        let target = match changes.get("dashboard.pearl.target") {
            Some(bead::core::Value::Number(target)) => *target,
            _ => return Err("dashboard pearl target is unavailable".into()),
        };
        let from = match changes.get("dashboard.pearl.x") {
            Some(bead::core::Value::Number(value)) => *value,
            _ => match self.session.stores.get("dashboard.pearl.x") {
                Some(bead::core::Value::Number(value)) => *value,
                _ => target,
            },
        };
        let duration = match changes.get("dashboard.pearl.duration") {
            Some(bead::core::Value::Number(value)) => *value,
            _ => 0.,
        };
        let easing = match changes.get("dashboard.pearl.curve") {
            Some(bead::core::Value::Text(value)) => value.clone(),
            _ => String::new(),
        };
        let reduced_motion = bead::core::animation::reduced_motion();
        let next = factory(page).map_err(std::io::Error::other)?;
        next.set_raster_scale(current.borrow().raster_scale());
        let wakeup = self
            .wakeup
            .clone()
            .ok_or("dashboard event channel unavailable")?;
        let subscriptions = next.subscribe(wakeup)?;
        let previous = std::mem::replace(&mut *current.borrow_mut(), next);
        let animate = !reduced_motion && duration > 0. && from != target;
        if !animate {
            changes.insert(
                "dashboard.pearl.x".into(),
                bead::core::Value::Number(target),
            );
            changes.insert(
                "dashboard.pearl.moving".into(),
                bead::core::Value::Bool(false),
            );
        }
        let entry = format!("dashboard/{}/main.luau", page.name()).into();
        let scene = match self.session.navigate(entry, changes) {
            Ok(scene) => scene,
            Err(error) => {
                *current.borrow_mut() = previous;
                return Err(error.into());
            }
        };
        self.pending_dashboard = Some(PendingDashboard {
            view: previous,
            subscriptions: std::mem::replace(&mut self.dashboard_subscription, subscriptions),
            animation: self.dashboard_pearl_animation.take(),
            deadline: self.dashboard_pearl_deadline.take(),
        });
        if animate {
            self.dashboard_pearl_animation = Some(DashboardPearlAnimation {
                from,
                to: target,
                started: std::time::Instant::now(),
                duration: std::time::Duration::from_secs_f64(duration),
                easing,
            });
            self.dashboard_pearl_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(16));
        } else {
            self.dashboard_pearl_animation = None;
            self.dashboard_pearl_deadline = None;
        }
        self.pending_panel_navigation = Some(format!("dashboard-{}", page.name()));
        Ok(Some(scene))
    }

    fn advance_dashboard_pearl(&mut self) -> Option<(f64, bool)> {
        if !self
            .dashboard_pearl_deadline
            .is_some_and(|deadline| deadline <= std::time::Instant::now())
        {
            return None;
        }
        let (target, value, done) = {
            let animation = self.dashboard_pearl_animation.as_ref()?;
            let progress = (animation.started.elapsed().as_secs_f64()
                / animation.duration.as_secs_f64())
            .clamp(0., 1.);
            let eased = if animation.easing == "out-cubic" {
                1. - (1. - progress).powi(3)
            } else {
                progress
            };
            (
                animation.to,
                animation.from + (animation.to - animation.from) * eased,
                progress >= 1.,
            )
        };
        if done {
            self.dashboard_pearl_animation = None;
            self.dashboard_pearl_deadline = None;
            Some((target, true))
        } else {
            self.dashboard_pearl_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(16));
            Some((value, false))
        }
    }
}

impl SceneSource for Runtime {
    fn drag_visuals(&self) -> Vec<bead::wayland::DragVisual> {
        use bead::core::Value;

        self.session
            .view_nodes()
            .iter()
            .filter_map(|thumb| {
                let metadata = match thumb.metadata.as_ref()? {
                    Value::List(records) => records.first()?,
                    _ => return None,
                };
                let motion = metadata.get("pointer_motion")?;
                let key = motion.get("key")?.as_str()?;
                let travel = motion.get("travel")?.as_f64()? as f32;
                let fraction = motion.get("fraction")?.as_f64()? as f32;
                let thumb_rect = bead::ui::rect(&thumb.draw);
                let scope = thumb.id.rsplit_once('.')?.0;
                let hit_prefix = format!("{scope}.");
                let mut hits = self
                    .session
                    .view_nodes()
                    .iter()
                    .filter(|node| node.preview_key == key && node.id.starts_with(&hit_prefix));
                let first = hits.next()?;
                let mut grab = bead::ui::rect(&first.draw);
                for hit in hits {
                    grab = grab.union(bead::ui::rect(&hit.draw));
                }
                let fill = motion.get("fill").and_then(|fill| {
                    let id = format!("{scope}.{}", fill.get("id")?.as_str()?);
                    let rect = self.session.paint_bounds(&id)?;
                    Some(bead::wayland::DragFill {
                        id,
                        rect,
                        minimum: fill.get("minimum")?.as_f64()? as f32,
                        radius: fill.get("radius")?.as_f64()? as f32,
                        gradient_min: fill.get("gradient_min")?.as_f64()? as f32,
                        from: self.session.paint_color(fill.get("from")?)?,
                        to: self.session.paint_color(fill.get("to")?)?,
                    })
                });
                if motion.get("fill").is_some() && fill.is_none() {
                    return None;
                }
                let inset = first.preview_inset;
                let start = motion
                    .get("origin")
                    .and_then(|origin| origin.as_f64())
                    .and_then(|origin| {
                        self.session
                            .paint_bounds(scope)
                            .map(|rect| rect.x + origin as f32)
                    })
                    .unwrap_or(thumb_rect.x - fraction * travel);
                let end = start + travel;
                let pointer = (grab.x + inset, grab.x + grab.w - inset);
                (travel.is_finite()
                    && travel > 0.
                    && fraction.is_finite()
                    && start.is_finite()
                    && end.is_finite()
                    && pointer.0.is_finite()
                    && pointer.1.is_finite()
                    && end > start
                    && pointer.1 > pointer.0)
                    .then(|| bead::wayland::DragVisual {
                        id: thumb.id.clone(),
                        grab,
                        travel: (start, end),
                        pointer,
                        fill,
                        local_feedback: self.control.is_some()
                            && first.action.as_deref() == Some("cc.do")
                            && motion.get("argument").is_some()
                            && self
                                .session
                                .supports_local_range(&first.id, motion.get("argument")),
                        release_serial: self.slider_release_serial,
                    })
            })
            .collect()
    }

    fn wakeup_at(&self) -> Option<std::time::Instant> {
        let now = std::time::Instant::now();
        let view_animation = animation_wakeup_after_panel_hold(
            now,
            self.panel_animation_hold_until,
            self.session.preview_deadline(),
        );
        let send = self
            .slider_pending
            .as_ref()
            .and(self.slider_sent)
            .map(|sent| sent + std::time::Duration::from_millis(34));
        if self.slider_local_active {
            return send;
        }
        let hover = (!self.widget_drag_is_active())
            .then(|| {
                self.widget_hover_preview
                    .as_ref()
                    .map(|preview| preview.prepared_at + WIDGET_HOVER_PREVIEW_RENEW)
            })
            .flatten();
        [
            send,
            hover,
            view_animation,
            self.session.hover_close_deadline(),
            self.dashboard_pearl_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn set_idle(&mut self, idle: bool) {
        // A closed view frees its markup VM before the worker's idle trim.
        if idle {
            self.session.release_markup_vm();
        }
    }

    fn frame_interval(&mut self, interval: std::time::Duration) {
        self.session.set_frame_interval(interval);
        self.smooth_frame_interval = interval.max(std::time::Duration::from_millis(1));
    }

    fn initial_model(
        &mut self,
        values: BTreeMap<String, bead::core::Value>,
    ) -> Result<(), Box<dyn Error>> {
        let mut stores = self.session.stores.reconcile(&values)?;
        stores.commit(values)?;
        if self.launcher_initial_projection.get()
            && let Some(menu) = &self.launcher
        {
            menu.prime_initial(&stores)?;
        }
        self.session.stores = stores;
        Ok(())
    }
    fn panel_anchor(&mut self, anchor: Option<bead::wayland::PanelAnchor>) {
        let before = self.popup_anchor.map(|anchor| anchor.edge);
        let after = anchor.map(|anchor| anchor.edge);
        self.popup_anchor = anchor;
        self.popup_anchor_changed = before != after;
    }
    fn panel_center(&mut self, center: Option<f32>) {
        if let Some(chrome) = &mut self.chrome {
            chrome.center = center;
        }
    }
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        let wakeup: std::sync::Arc<dyn Fn(UiEvent) + Send + Sync> = std::sync::Arc::from(wakeup);
        self.wakeup = Some(wakeup.clone());
        if let Some(source) = &self.widget_displays_topic {
            let wakeup = wakeup.clone();
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                let values = widget_screen_values(&snapshot);
                if !values.is_empty() {
                    wakeup(UiEvent::Model(values));
                }
            })) {
                Ok(subscription) => self.widget_displays_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=widgets_displays_subscription"),
            }
        }
        if (self.widget_app || self.widget_floating_request.is_some())
            && let Some(source) = &self.widget_compositor_topic
        {
            let wakeup = wakeup.clone();
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                wakeup(UiEvent::Model(widget_app_origin_values(&snapshot)));
            })) {
                Ok(subscription) => self.widget_compositor_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=widgets_compositor_subscription"),
            }
        }
        if self.bar {
            let wakeup = wakeup.clone();
            self.wallpaper_luma_watch = Some(crate::wallpaper_luma::watch(std::sync::Arc::new(
                move || {
                    wakeup(UiEvent::Model(BTreeMap::from([(
                        WALLPAPER_LUMA_POKE.into(),
                        bead::core::Value::Bool(true),
                    )])))
                },
            )));
        }
        if self.bar || self.widget_app {
            let state_wakeup = wakeup.clone();
            let wakeup = wakeup.clone();
            let bar = self.bar;
            self.widget_drag_subscription = Some(crate::widgets::widget_drag::subscribe(
                std::sync::Arc::new(move |message| match message {
                    crate::widgets::widget_drag::PrivateEvent::AppState {
                        pid,
                        generation,
                        open,
                        floating,
                    } => wakeup(UiEvent::Model(BTreeMap::from([
                        ("widgets.app.open".into(), bead::core::Value::Bool(open)),
                        ("widgets.editing".into(), bead::core::Value::Bool(open)),
                        (
                            "widgets.policy.pid".into(),
                            bead::core::Value::Number(f64::from(pid)),
                        ),
                        (
                            "widgets.policy.generation".into(),
                            bead::core::Value::Number(generation as f64),
                        ),
                        ("widgets.policy.open".into(), bead::core::Value::Bool(open)),
                        (
                            "widgets.policy.floating".into(),
                            bead::core::Value::Bool(floating),
                        ),
                    ]))),
                    crate::widgets::widget_drag::PrivateEvent::Motion {
                        pid,
                        epoch,
                        mut event,
                    } => {
                        if !bar {
                            return;
                        }
                        if let Some(object) = event.as_object_mut() {
                            object.insert("private_remote".into(), serde_json::Value::Bool(true));
                            object.insert("source_pid".into(), serde_json::json!(pid));
                            object.insert("gesture_epoch".into(), serde_json::json!(epoch));
                        }
                        let is_move = event["phase"].as_str() == Some("move");
                        let name = "widgets.drag".to_owned();
                        let argument = bead::core::Value::list(vec![event]);
                        if is_move {
                            wakeup(UiEvent::MotionAction { name, argument });
                        } else {
                            wakeup(UiEvent::Action { name, argument });
                        }
                    }
                }),
            ));
            if let Some((pid, generation, floating)) =
                crate::widgets::widget_drag::active_app_policy()
            {
                state_wakeup(UiEvent::Model(BTreeMap::from([
                    ("widgets.app.open".into(), bead::core::Value::Bool(true)),
                    ("widgets.editing".into(), bead::core::Value::Bool(true)),
                    (
                        "widgets.policy.pid".into(),
                        bead::core::Value::Number(f64::from(pid)),
                    ),
                    (
                        "widgets.policy.generation".into(),
                        bead::core::Value::Number(generation as f64),
                    ),
                    ("widgets.policy.open".into(), bead::core::Value::Bool(true)),
                    (
                        "widgets.policy.floating".into(),
                        bead::core::Value::Bool(floating),
                    ),
                ])));
            }
        }
        if let Some(source) = &self.connection_status {
            let wakeup = wakeup.clone();
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                let mark = snapshot.data["reconnecting"] == true;
                wakeup(UiEvent::Model(std::collections::BTreeMap::from([(
                    "bar.reconnecting".into(),
                    bead::core::Value::Bool(mark),
                )])));
            })) {
                Ok(subscription) => self.connection_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=connection_subscription"),
            }
        }
        if let Some(source) = &self.prefs {
            let wakeup = wakeup.clone();
            let launcher = self.launcher.is_some();
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                let prefs: crate::prefs::Prefs =
                    serde_json::from_value((*snapshot.data).clone()).unwrap_or_default();
                wakeup(UiEvent::Model(if launcher {
                    prefs.launcher_stores()
                } else {
                    prefs.bar_stores()
                }));
            })) {
                Ok(subscription) => self.prefs_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=prefs_subscription"),
            }
        }
        if let Some(routes) = &self.routes {
            let wakeup = wakeup.clone();
            match routes.watch(move |stores| wakeup(UiEvent::Model(stores))) {
                Ok(subscription) => self.routes_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=bar_routes_subscription"),
            }
        }
        if let Some(tray) = &self.tray_topic {
            let wakeup = wakeup.clone();
            let rows = self.daemon_tray.clone();
            match tray.subscribe(std::sync::Arc::new(move |snapshot| {
                let items = crate::bar_routes::tray_items(&snapshot.data);
                let list = bead::core::Value::list(crate::bar_routes::tray_rows(&items));
                *rows.lock().unwrap_or_else(|e| e.into_inner()) = items;
                wakeup(UiEvent::Model(BTreeMap::from([(
                    "bead.desktop.tray".into(),
                    list,
                )])));
            })) {
                Ok(subscription) => self.tray_topic_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=tray_topic_subscription"),
            }
        }
        if let Some(path) = &self.widgets_file {
            use notify::Watcher;
            let path = path.clone();
            let watched = path.clone();
            let wake = wakeup.clone();
            let watch = || -> Result<notify::RecommendedWatcher, Box<dyn Error>> {
                let parent = path.parent().ok_or("widget file needs a parent")?;
                std::fs::create_dir_all(parent)?;
                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        if let Ok(event) = event
                            && !event.kind.is_access()
                            && (event.need_rescan() || event.paths.iter().any(|p| p == &watched))
                        {
                            let values = crate::widgets::file_values(&watched);
                            if !values.is_empty() {
                                wake(UiEvent::Model(values));
                            }
                        }
                    })?;
                watcher.watch(parent, notify::RecursiveMode::NonRecursive)?;
                Ok(watcher)
            };
            match watch() {
                Ok(watcher) => {
                    self.widgets_watch = Some(watcher);
                    let values = crate::widgets::file_values(&path);
                    if !values.is_empty() {
                        wakeup(UiEvent::Model(values));
                    }
                }
                Err(error) => eprintln!(
                    "WIDGET_LAYOUT_ERROR file={} line=1 message={error}",
                    path.display()
                ),
            }
        }
        if let Some(topic) = &self.widgets_topic {
            let wakeup = wakeup.clone();
            match topic.subscribe(std::sync::Arc::new(move |snapshot| {
                let values = crate::widgets::values(&snapshot);
                if !values.is_empty() {
                    wakeup(UiEvent::Model(values));
                }
            })) {
                Ok(subscription) => self.widgets_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=widgets_subscription"),
            }
        }
        if let Some((topic, _)) = &self.recording_topic {
            let indicator =
                std::sync::Arc::new(std::sync::Mutex::new(crate::bar_recording::Indicator::new()));
            let follow = {
                let (indicator, wakeup) = (indicator.clone(), wakeup.clone());
                move |snapshot: &bead::core::source::Snapshot| {
                    let wakeup = wakeup.clone();
                    if let Ok(mut indicator) = indicator.lock() {
                        indicator.follow(snapshot, move |values| wakeup(UiEvent::Model(values)));
                    }
                }
            };
            // A recording that began before this bar did.
            follow(&topic.snapshot());
            match topic.subscribe(std::sync::Arc::new(move |snapshot| follow(&snapshot))) {
                Ok(subscription) => self.recording_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=bar_recording_subscription"),
            }
        }
        if let Some(topic) = &self.updates_topic {
            let wakeup = wakeup.clone();
            match topic.subscribe(std::sync::Arc::new(move |snapshot| {
                wakeup(UiEvent::Model(crate::bar_updates::values(&snapshot)));
            })) {
                Ok(subscription) => self.updates_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=bar_updates_subscription"),
            }
        }
        if let Some(topic) = &self.origins_topic {
            let wakeup = wakeup.clone();
            // Titles and focus churn the topic constantly; only origin changes reach the bar.
            let last = std::sync::Mutex::new(Vec::<serde_json::Value>::new());
            match topic.subscribe(std::sync::Arc::new(move |snapshot| {
                let rows = crate::workspace_rows::origin_rows(&snapshot.data);
                let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
                if *last != rows {
                    last.clone_from(&rows);
                    wakeup(UiEvent::Model(BTreeMap::from([(
                        "bar.workspace.origins".into(),
                        bead::core::Value::list(rows),
                    )])));
                }
            })) {
                Ok(subscription) => self.origins_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=origins_subscription"),
            }
        }
        if let Some(source) = &self.bar_daemon {
            let wakeup = wakeup.clone();
            match source.subscribe(std::sync::Arc::new(move |values| {
                wakeup(UiEvent::Model(values));
            })) {
                Ok(subscriptions) => self.bar_daemon_subscriptions = subscriptions,
                Err(_) => eprintln!("SOURCE_ERROR code=bar_daemon_subscription"),
            }
        }
        // A language switch rebuilds the views from the new catalog, then lets the
        // Rust-side projectors rewrite their text (live refresh, no restart).
        {
            let wakeup = wakeup.clone();
            self.i18n_subscription = Some(bead::ui::i18n::on_change(move || {
                wakeup(UiEvent::Reload);
                wakeup(UiEvent::Model(BTreeMap::new()));
            }));
        }
        if let Some(calendar) = &self.calendar {
            match calendar.subscribe(wakeup.clone()) {
                Ok(subscription) => self.calendar_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=calendar_subscription"),
            }
        }
        if let Some(status) = &self.status {
            match status.subscribe(wakeup.clone()) {
                Ok(subscription) => self.status_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=status_subscription"),
            }
            let wakeup = wakeup.clone();
            let notify: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            });
            match status.subscribe_control_link(notify) {
                Ok(subscription) => self.status_control_subscription = Some(subscription),
                Err(error) if error.code == "unavailable" => {}
                Err(_) => eprintln!("SOURCE_ERROR code=status_control_subscription"),
            }
        }
        for status in self.dock_status.values() {
            match status.subscribe(wakeup.clone()) {
                Ok(subscription) => self.dock_subscriptions.push(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=dock_subscription"),
            }
            let wakeup = wakeup.clone();
            let notify = std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new())));
            match status.subscribe_control_link(notify) {
                Ok(subscription) => self.dock_subscriptions.push(subscription),
                Err(error) if error.code == "unavailable" => {}
                Err(_) => eprintln!("SOURCE_ERROR code=dock_control_subscription"),
            }
        }
        if let Some(chat) = &self.chat {
            match chat.subscribe(wakeup.clone()) {
                Ok(subscriptions) => self.chat_subscription = subscriptions,
                Err(_) => eprintln!("SOURCE_ERROR code=chat_subscription"),
            }
        }
        if let Some(control) = &self.control {
            match control.subscribe(wakeup.clone()) {
                Ok(subscriptions) => self.control_subscription = subscriptions,
                Err(_) => eprintln!("SOURCE_ERROR code=control_subscription"),
            }
        }
        if let Some(extras) = &self.extras {
            let wakeup = wakeup.clone();
            let wake: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(slot) = &self.extras_wake {
                *slot.lock().unwrap() = Some(wake.clone());
            }
            match extras.subscribe(wake) {
                Ok(subscriptions) => self.extras_subscription = subscriptions,
                Err(_) => eprintln!("SOURCE_ERROR code=extras_subscription"),
            }
        }
        if let Some(prompt) = &self.prompt {
            let wakeup = wakeup.clone();
            prompt.watch(std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }));
        }
        if let Some(menu) = &self.session_menu {
            let wakeup = wakeup.clone();
            match menu.subscribe(std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            })) {
                Ok(subscription) => self.session_menu_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=session_menu_subscription"),
            }
        }
        if let Some(settings) = &self.settings {
            let wakeup = wakeup.clone();
            settings.subscribe(std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }));
        }
        if let Some(colors) = &self.colors {
            let wakeup = wakeup.clone();
            colors.subscribe(std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }));
        }
        if let Some(welcome) = &self.welcome {
            let wakeup = wakeup.clone();
            welcome.subscribe(std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }));
        }
        if let Some(dashboard) = &self.dashboard {
            match dashboard.borrow().subscribe(wakeup.clone()) {
                Ok(subscription) => self.dashboard_subscription = subscription,
                Err(_) => eprintln!("SOURCE_ERROR code=dashboard_subscription"),
            }
        }
        if let Some(menu) = &self.launcher {
            let wakeup = wakeup.clone();
            match menu.subscribe(std::sync::Arc::new(move |_| {
                wakeup(UiEvent::Model(Default::default()))
            })) {
                Ok(subscription) => self.launcher_subscription = subscription,
                Err(_) => eprintln!("SOURCE_ERROR code=launcher_subscription"),
            }
        }
        if let Some(quick) = &self.launcher_quick {
            match quick.subscribe(wakeup.clone()) {
                Ok(subscriptions) => self.launcher_quick_subscription = subscriptions,
                Err(_) => eprintln!("SOURCE_ERROR code=launcher_quick_subscription"),
            }
        }
        if let Some(owner) = &self.notifications {
            self.notification_subscription =
                Some(owner.subscribe(wakeup.clone(), self.notification_primary));
        }
        if let Some(host) = &self.notice_host {
            self.notice_drive = Some(host.drive(wakeup.clone()));
        }
        if let Some(desktop) = &self.desktop {
            let wakeup = wakeup.clone();
            self.desktop_subscription =
                Some(desktop.subscribe(move |snapshot| wakeup(UiEvent::Desktop(snapshot))));
        }
        self.subscribe_backend();
    }
    fn input_regions(&self) -> Vec<bead::wayland::InputRegion> {
        self.session
            .view_nodes()
            .iter()
            .filter(|node| {
                node.input.is_some()
                    || node.action.is_some()
                    || node.drag_action.is_some()
                    || node.drop_action.is_some()
            })
            .map(|node| {
                let rect = match &node.draw {
                    bead::render::Primitive::Rounded { rect, .. }
                    | bead::render::Primitive::Text { rect, .. }
                    | bead::render::Primitive::Image { rect, .. }
                    | bead::render::Primitive::Shadow { rect, .. }
                    | bead::render::Primitive::Arc { rect, .. }
                    | bead::render::Primitive::Polyline { rect, .. } => *rect,
                };
                bead::wayland::InputRegion {
                    include_in_bounds: node.action.is_some()
                        || node.input.is_some()
                        || node.drag_action.is_some(),
                    id: node.id.clone(),
                    prepare_panel: self.bar
                        && (node.action.as_deref() == Some("view.open")
                            || node.action.as_deref() == Some("status.pill.open")
                            || node.action.as_deref()
                                == Some(if self.launcher_trigger {
                                    "launcher.open"
                                } else {
                                    "services.audio"
                                })),
                    prepare_view: if self.bar && node.action.as_deref() == Some("view.open") {
                        if let Some(bead::core::Value::Text(view)) = &node.action_arg {
                            Some(view.clone())
                        } else {
                            None
                        }
                    } else if self.bar && node.action.as_deref() == Some("status.pill.open") {
                        node.action_arg
                            .as_ref()
                            .and_then(crate::widgets::dropdown_view)
                            .map(|view| {
                                if !crate::widgets::dropdown_hover(
                                    node.action_arg.as_ref().unwrap(),
                                ) || matches!(view, "calendar" | "session")
                                {
                                    if view == "audio" {
                                        "sound".to_owned()
                                    } else {
                                        view.to_owned()
                                    }
                                } else {
                                    format!("peek-{}", if view == "sound" { "audio" } else { view })
                                }
                            })
                    } else {
                        None
                    },
                    prepare_center: if matches!(
                        node.action.as_deref(),
                        Some("view.open" | "status.pill.open")
                    ) {
                        if let Some(view) = node
                            .action_arg
                            .as_ref()
                            .and_then(crate::widgets::dropdown_view)
                        {
                            if node.action.as_deref() == Some("status.pill.open") {
                                let rect = bead::ui::rect(&node.draw);
                                Some(rect.x + rect.w / 2.)
                            } else {
                                self.popout_center(&node.id, view)
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    },
                    dismiss: node.action.as_deref() == Some("close")
                        && self.session.stores.get("panel.open").is_some(),
                    rect,
                    clip: node.clip,
                    hit_polygon: node.hit_polygon.clone(),
                    cursor: if node.input.is_some() {
                        "text"
                    } else {
                        "pointer"
                    },
                    edge_hover: node.edge_hover,
                }
            })
            .collect()
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn allows_edge_hover(&self, x: f32, y: f32) -> bool {
        self.session.allows_edge_hover(x, y)
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn sensitive_input(&self) -> bool {
        self.session.focused_masked()
    }
    fn scales(&mut self, scales: Vec<f32>) {
        let scale = scales
            .into_iter()
            .filter(|scale| scale.is_finite() && *scale > 0.)
            .max_by(f32::total_cmp);
        if let Some(scale) = scale
            && let Some(warmer) = &self.icon_warmer
        {
            warmer.set_scale(scale);
        }
        if let Some(dashboard) = &self.dashboard
            && let Some(scale) = scale
        {
            dashboard.borrow().set_raster_scale(scale);
        }
        if let Some(warmer) = &self.wallpaper_warmer
            && let Some(scale) = scale
        {
            warmer.set_scale(scale);
        }
    }
    fn tracks_origin(&self) -> bool {
        self.widget_editor
    }
    fn viewport(&mut self, width: u32, height: u32) -> bool {
        self.viewport_size = (width, height);
        if let Some(chrome) = &mut self.chrome {
            chrome.output = (width, height);
        }
        if let Some(control) = &self.control {
            control.set_viewport(width, height);
        }
        if let Some(chat) = &self.chat {
            chat.set_viewport(width, height);
        }
        if let Some(menu) = &self.session_menu {
            menu.set_viewport(height);
        }
        if let Some(colors) = &self.colors {
            colors.set_viewport(width);
        }
        if let Some(welcome) = &self.welcome {
            welcome.set_viewport(width, height);
        }
        if let Some(extras) = &self.extras {
            extras.set_viewport(width, height);
        }
        if let Some(warmer) = &self.wallpaper_warmer {
            let frame = crate::placement::Frame::current();
            let output_width = width as f32;
            let output_height = height as f32;
            warmer.set_viewport(
                output_width,
                output_height,
                (output_width - frame.left - frame.right).max(1.),
                (output_height - frame.top - frame.bottom).max(1.),
            );
        }
        if self.widget_editor {
            let values = BTreeMap::from([
                (
                    "widgets.canvas.width".into(),
                    bead::core::Value::Number(f64::from(width)),
                ),
                (
                    "widgets.canvas.height".into(),
                    bead::core::Value::Number(f64::from(height)),
                ),
            ]);
            if let Ok(stores) = self.session.stores.reconcile(&values) {
                self.session.stores = stores;
            }
            let _ = self.session.stores.commit(values);
        }
        self.session.set_viewport(width, height)
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn stage(&mut self, mut event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        let now = std::time::Instant::now();
        if let UiEvent::PanelVisibility(open) = &event
            && let Some(duration) = self.session.motion_duration(*open)
        {
            self.pending_panel_animation_duration = Some(duration);
        }
        if matches!(&event, UiEvent::Reload) {
            #[cfg(test)]
            {
                self.reload_events += 1;
            }
            crate::notice_service::refresh_icon_theme();
            if let Some(dashboard) = &self.dashboard {
                dashboard.borrow().invalidate_projection();
            }
        }
        if matches!(
            &event,
            UiEvent::Key { .. }
                | UiEvent::Click { .. }
                | UiEvent::Drag { .. }
                | UiEvent::Release
                | UiEvent::Scroll { .. }
                | UiEvent::HorizontalScroll { .. }
                | UiEvent::SecondaryClick { .. }
                | UiEvent::ContextMenu { .. }
                | UiEvent::Command { .. }
                | UiEvent::PanelVisibility(true)
                | UiEvent::Hover { .. }
        ) {
            self.smooth_user_interacted = true;
        }
        match &event {
            UiEvent::PanelVisibility(true) if self.smooth_open_started.is_none() => {
                self.smooth_open_started = Some(now);
            }
            UiEvent::Key { .. } if self.launcher.is_some() || self.chat.is_some() => {
                if self.smooth_key_starts.len() == 32 {
                    self.smooth_key_starts.pop_front();
                }
                let surface = if self.chat.is_some() {
                    "ai-chat"
                } else {
                    "launcher"
                };
                self.smooth_key_starts.push_back((now, surface));
            }
            UiEvent::Click { .. } if self.chat.is_some() => {
                if self.smooth_key_starts.len() == 32 {
                    self.smooth_key_starts.pop_front();
                }
                self.smooth_key_starts.push_back((now, "ai-chat"));
            }
            UiEvent::Scroll { .. } => {
                if self.smooth_scroll_last.is_none_or(|last| {
                    now.duration_since(last) > std::time::Duration::from_millis(120)
                }) {
                    self.smooth_scroll_started = Some(now);
                    self.smooth_scroll_dropped = 0;
                    self.smooth_scroll_reported = None;
                }
                self.smooth_scroll_last = Some(now);
            }
            _ => {}
        }
        self.host_release_pending = false;
        // Service completion stays live even when the view keeps its accepted layout.
        if matches!(&event, UiEvent::LocalDrag { .. } | UiEvent::Model(_))
            && let Some(control) = &self.control
        {
            control.tick_calls();
        }
        let mut local = false;
        if let UiEvent::LocalDrag { x, y } = event {
            local = true;
            event = UiEvent::Drag { x, y };
        }
        if matches!(&event, UiEvent::Model(values) if values.is_empty())
            && !self.widget_drag_is_active()
            && self
                .widget_hover_preview
                .as_ref()
                .is_some_and(|preview| preview.prepared_at.elapsed() >= WIDGET_HOVER_PREVIEW_RENEW)
        {
            self.prewarm_widget_drag_hover(self.pointer_at.0, self.pointer_at.1, false);
        }
        // Finish the old gesture before release can retire its final target. A final flush is exempt from the continuous-drag cap.
        if matches!(&event, UiEvent::Release)
            && let Some(pending) = self.slider_pending.take()
        {
            self.slider_sent = None;
            let _ = self.stage(pending)?;
        }
        // Service notifications carry no model delta: the source already owns
        // the latest data. Project it once on release; explicit model/geometry
        // changes still take the ordinary path immediately.
        if self.slider_local_active && matches!(&event,UiEvent::Model(values) if values.is_empty())
        {
            if self
                .slider_sent
                .is_some_and(|sent| sent.elapsed() >= std::time::Duration::from_millis(34))
                && let Some(pending) = self.slider_pending.take()
            {
                self.slider_sent = None;
                return self.stage(pending);
            }
            return Ok(None);
        }
        if !local {
            self.slider_local_active = false;
        }

        if matches!(event, UiEvent::Release)
            && self.session.stores.get("bead.pressed") == Some(&bead::core::Value::Bool(true))
        {
            self.slider_release_serial = self.slider_release_serial.wrapping_add(1);
        }
        if let UiEvent::Click { x, y } | UiEvent::Drag { x, y } | UiEvent::Hover { x, y } = &event {
            self.pointer_at = (*x, *y);
        }
        if matches!(&event, UiEvent::Model(values) if values.is_empty())
            && let Some(reply) = self
                .widgets_write
                .as_mut()
                .and_then(|ticket| ticket.try_take())
        {
            self.widgets_write = None;
            let document = self.widgets_write_document.take();
            let previous = self.widgets_write_previous.take();
            let success = reply.is_ok();
            let accepted_revision = reply.as_ref().ok().and_then(|reply| reply["rev"].as_u64());
            let reply_error = reply
                .as_ref()
                .err()
                .map(|error| error.message.clone())
                .unwrap_or_default();
            let feedback = if self.widgets_drag_write {
                self.widget_write_feedback.take()
            } else {
                None
            };
            let drag_write = self.widgets_drag_write;
            self.widgets_drag_write = false;
            if drag_write {
                self.widget_drag_model = None;
            }
            let mut history_revision = feedback
                .as_ref()
                .map(|item| item.previous_revision.saturating_add(1));
            let mut history_accepted = success;
            let changes = if self.widget_app || drag_write {
                let canonical = self
                    .widgets_topic
                    .as_ref()
                    .map(|topic| {
                        crate::widgets::values(&bead::core::source::Source::snapshot(&**topic))
                    })
                    .unwrap_or_default();
                if let Some(feedback) = &feedback {
                    let revision = accepted_revision
                        .unwrap_or_else(|| feedback.previous_revision.saturating_add(1));
                    history_revision = Some(revision);
                    history_accepted = success
                        && revision > feedback.previous_revision
                        && widget_revision(&canonical) <= revision;
                }
                let error = reply.as_ref().err().map(|error| error.message.clone());
                widget_write_resolution(
                    document,
                    previous,
                    accepted_revision,
                    error,
                    canonical,
                    self.widget_app,
                )
            } else {
                let (editing, error) = match reply {
                    Ok(_) => {
                        self.widgets_close_on_commit = true;
                        (false, String::new())
                    }
                    Err(error) => (
                        self.session.stores.get("panel.open")
                            == Some(&bead::core::Value::Bool(true)),
                        error.message,
                    ),
                };
                let mut changes = BTreeMap::from([
                    ("widgets.editing".into(), bead::core::Value::Bool(editing)),
                    ("widgets.error".into(), bead::core::Value::Text(error)),
                ]);
                if success && let Some(document) = document {
                    changes.insert("widgets.layout".into(), document);
                }
                changes
            };
            let scene = self.session.update(changes)?;
            if let Some(feedback) = feedback {
                send_widget_write_feedback(
                    feedback,
                    history_accepted,
                    &reply_error,
                    history_revision.unwrap_or(0),
                );
            }
            return self.finish_scene(scene);
        }
        if let UiEvent::Click { x, y } = &event {
            self.session.capture_pointer(*x, *y);
            self.prepare_widget_drag_preview(*x, *y);
        }
        let _pointer_input = matches!(&event, UiEvent::Click { .. } | UiEvent::Drag { .. })
            .then(|| self.session.pointer_input());
        if matches!(&event, UiEvent::Model(values) if values.is_empty())
            && self
                .slider_sent
                .is_some_and(|sent| sent.elapsed() >= std::time::Duration::from_millis(34))
            && let Some(pending) = self.slider_pending.take()
        {
            event = match pending {
                UiEvent::LocalDrag { x, y } => {
                    local = true;
                    UiEvent::Drag { x, y }
                }
                other => other,
            };
        }
        // A pearl's border lights up under the pointer; re-decorate only when that flips.
        if self.detached
            && let UiEvent::Hover { x, .. } = &event
            && let Some(chrome) = &mut self.chrome
            && chrome.engaged() != (*x >= 0.)
        {
            chrome.set_engaged(*x >= 0.);
            event = UiEvent::Relayout;
        }
        self.pending = None;
        if let UiEvent::Hover { x, y } = &event
            && let Some(owner) = &self.notifications
        {
            let next = self
                .session
                .action_at(*x, *y)
                .and_then(|(_, name, arg)| crate::notice_service::hover_key(&name, &arg));
            if next != self.notification_hover {
                if let Some(key) = self.notification_hover.take() {
                    owner.hover(&key, false);
                }
                if let Some(key) = next {
                    owner.hover(&key, true);
                    self.notification_hover = Some(key);
                }
            }
        }
        if let Some(prompt) = &self.prompt {
            let action = match &event {
                UiEvent::Command { command, .. } => match command.as_str() {
                    "up" if !self.session.focused_multiline() => Some("prompt.prev"),
                    "down" if !self.session.focused_multiline() => Some("prompt.next"),
                    "enter" if self.session.focused_action().is_none() => Some("prompt.choose"),
                    _ => None,
                },
                _ => None,
            };
            if let Some(action) = action {
                let mut values = self
                    .session
                    .action_values(action, bead::core::Value::Bool(true))?;
                let request = match values.remove("prompt.request") {
                    Some(bead::core::Value::Text(s)) => s,
                    _ => String::new(),
                };
                if !request.is_empty() {
                    values.extend(prompt.act(&request, &self.session.stores));
                }
                let scene = self.session.update(values)?;
                return self.finish_scene(scene);
            }
        }
        if self.extras.is_some() {
            if let UiEvent::Hover { x, y } = &event
                && let Some((_, name, bead::core::Value::Text(request))) =
                    self.session.action_at(*x, *y)
                && name == "extras.do"
                && let Some(id) = request
                    .strip_prefix("focus:")
                    .or_else(|| request.strip_prefix("activate:"))
            {
                let values = self
                    .session
                    .action_values("extras.select", bead::core::Value::Text(id.into()))?;
                let scene = self.session.update(values)?;
                return self.finish_scene(scene);
            }
            let action = match &event {
                UiEvent::Command { command, .. } => match command.as_str() {
                    "left"
                        if self
                            .extras
                            .as_ref()
                            .is_some_and(|e| e.kind == crate::extras::Kind::Wallpaper) =>
                    {
                        Some("extras.prev")
                    }
                    "right"
                        if self
                            .extras
                            .as_ref()
                            .is_some_and(|e| e.kind == crate::extras::Kind::Wallpaper) =>
                    {
                        Some("extras.next")
                    }
                    "up" => Some("extras.prev"),
                    "down" => Some("extras.next"),
                    "enter" if self.session.focused_action().is_none() => Some("extras.choose"),
                    _ => None,
                },
                UiEvent::Scroll { x, y, delta }
                    if self
                        .extras
                        .as_ref()
                        .is_some_and(|e| e.kind == crate::extras::Kind::Wallpaper)
                        && *delta != 0.
                        && self.session.stores.get("extras.wallpaper.layout")
                            != Some(&bead::core::Value::Text("hexagons".into()))
                        && self.session.view_nodes().iter().any(|node| {
                            (node.id.ends_with(".carousel-strip")
                                || node.id.ends_with(".spotlight-strip"))
                                && node.contains(*x, *y)
                        }) =>
                {
                    Some(if *delta > 0. {
                        "extras.next"
                    } else {
                        "extras.prev"
                    })
                }
                _ => None,
            };
            if self
                .extras
                .as_ref()
                .is_some_and(|e| e.kind == crate::extras::Kind::Wallpaper)
                && matches!(&event, UiEvent::Command { command, .. } if command == "escape")
            {
                let wakeup = self
                    .wakeup
                    .clone()
                    .ok_or("wallpaper picker close event unavailable")?;
                wakeup(UiEvent::CompanionVisibility(false));
                return Ok(None);
            }
            if let Some(action) = action {
                let mut values = self
                    .session
                    .action_values(action, bead::core::Value::Bool(true))?;
                let request = match values.get("extras.request") {
                    Some(bead::core::Value::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                if !request.is_empty() {
                    let wakeup = self
                        .wakeup
                        .clone()
                        .ok_or("extras event channel unavailable")?;
                    self.extras.as_ref().unwrap().act(
                        &request,
                        &self.session.stores,
                        std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new()))),
                    );
                    values.insert(
                        "extras.request".into(),
                        bead::core::Value::Text(String::new()),
                    );
                }
                let scene = self.session.update(values)?;
                return self.finish_scene(scene);
            }
        }
        // Welcome: Enter advances the tour, except on a tile of the page itself (QML parity).
        if self.welcome.is_some()
            && matches!(&event, UiEvent::Command { command, .. } if command == "enter")
            && !matches!(self.session.stores.get("bead.focused"),
                Some(bead::core::Value::Text(focused)) if focused.contains(".canvas."))
        {
            self.session.stores.commit(BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text(crate::welcome::NEXT.into()),
            )]))?;
        }
        if let UiEvent::Action { name, argument } | UiEvent::MotionAction { name, argument } =
            &event
            && name == "widgets.drag"
            && !widget_drag_targets_output(argument, self.output.as_deref())
        {
            return self.retire_widget_drag_on_other_output();
        }
        let mut host_action = match &event {
            UiEvent::Action { name, argument } | UiEvent::MotionAction { name, argument } => {
                Some((String::new(), name.clone(), argument.clone()))
            }
            UiEvent::Click { x, y } => self.session.press_action_at(*x, *y),
            UiEvent::Release => self
                .session
                .transfer_action(self.pointer_at.0, self.pointer_at.1, "drop")
                .or_else(|| {
                    self.session
                        .transfer_action(self.pointer_at.0, self.pointer_at.1, "cancel")
                })
                .or_else(|| self.session.release_action()),
            UiEvent::ContextMenu { x, y }
            | UiEvent::SecondaryClick { x, y }
            | UiEvent::HorizontalScroll { x, y, .. }
            | UiEvent::Scroll { x, y, .. } => self.session.action_at(*x, *y),
            UiEvent::Command { command, .. } if command == "enter" => self.session.focused_action(),
            UiEvent::Command { command, .. } if command == "left" || command == "right" => {
                self.session.range_action(command == "left")
            }
            UiEvent::Drag { x, y } => self
                .session
                .transfer_action(*x, *y, "move")
                .or_else(|| self.session.drag_action(*x, *y)),
            _ => None,
        };
        if matches!(
            &event,
            UiEvent::Scroll { .. } | UiEvent::HorizontalScroll { .. }
        ) && host_action
            .as_ref()
            .is_some_and(|(_, name, _)| matches!(name.as_str(), "select_app" | "toggle_favorite"))
        {
            host_action = None;
        }
        if let Some((_, name, argument)) = &mut host_action
            && name == "status.pill.open"
            && let Some(view) = crate::widgets::dropdown_view(argument)
        {
            *argument = bead::core::Value::Text(view.to_owned());
        }
        if self.widget_app
            && std::env::var_os("BEAD_PRIVATE_WINDOW_TRACE").is_some()
            && matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
        {
            eprintln!(
                "WIDGET_INPUT release={} point={:?} action={:?}",
                matches!(&event, UiEvent::Release),
                self.pointer_at,
                host_action.as_ref().map(|(id, name, _)| (id, name))
            );
        }
        let mut pointer_scene = None;
        if matches!(&event, UiEvent::Release) {
            self.host_release_pending = host_action.as_ref().is_some_and(|(_, name, _)| {
                name == "view.open"
                    || name == "status.pill.open"
                    || name == "launcher.open"
                    || name == "launcher.quick"
                    || name == "select_app"
                    || name == "toggle_favorite"
                    || name == "launcher.form"
                    || name == "bar.recording.stop"
                    || name == "bar.title"
                    || name == "dashboard.select"
                    || name == "dashboard.do"
                    || name == "cc.do"
                    || name == "chat.do"
                    || name == "settings.do"
                    || name == "colors.do"
                    || name == "welcome.do"
                    || name == "prompt.do"
                    || name == "extras.do"
                    || name.starts_with("backend.")
                    || name.starts_with("services.")
                    || name.starts_with("notifications.")
                    || name.starts_with("updates.")
                    || name.starts_with("network.")
                    || name.starts_with("bluetooth.")
                    || name.starts_with("session.")
            });
        }
        if let Some((_, name, argument)) = &host_action
            && name == "widgets.drag"
        {
            self.actions.authorize("widgets.drag")?;
            if self.widget_app {
                let argument = self.widget_argument(name, argument.clone());
                let event_value = match &argument {
                    bead::core::Value::List(records) => records.first().cloned(),
                    _ => None,
                };
                let mut changes = self.session.action_values(name, argument)?;
                let terminal_drop = event_value.as_ref().is_some_and(|value| {
                    value["phase"] == "drop"
                        && value["source_origin_valid"] == true
                        && !value["target_output_id"].as_str().unwrap_or("").is_empty()
                        && matches!(value["destination"].as_str(), Some("bar" | "desktop"))
                });
                if terminal_drop {
                    changes.insert("widgets.busy".into(), bead::core::Value::Bool(true));
                    changes.insert(
                        "widgets.error".into(),
                        bead::core::Value::Text(String::new()),
                    );
                    changes.insert(
                        "widgets.message".into(),
                        bead::core::Value::Text(String::new()),
                    );
                }
                match changes.get("widgets.drag.phase") {
                    Some(bead::core::Value::Text(phase)) if phase == "dragging" => {
                        self.widget_drag_epoch =
                            self.widget_drag_epoch.wrapping_add(1).max(1u64 << 63)
                    }
                    Some(bead::core::Value::Text(phase))
                        if phase == "settle" || phase == "return" =>
                    {
                        self.retire_widget_drag_after_feedback()
                    }
                    _ => {}
                }
                let scene = self.session.update(changes)?;
                if let Some(event_value) = event_value {
                    let event_json: serde_json::Value = event_value;
                    if matches!(
                        event_json["phase"].as_str(),
                        Some("move" | "drop" | "cancel")
                    ) && event_json["source_origin_valid"] == true
                    {
                        let epoch = crate::widgets::widget_drag::app_drag(event_json.clone());
                        match event_json["phase"].as_str() {
                            Some("drop") if terminal_drop => {
                                self.widget_drag_pending_epoch = Some(epoch);
                                self.widget_drag_latest_epoch = Some(epoch);
                            }
                            Some("move" | "cancel") => self.widget_drag_latest_epoch = Some(epoch),
                            _ => {}
                        }
                    }
                }
                if matches!(&event, UiEvent::Release) {
                    self.session.release()?;
                }
                return self.finish_scene(scene);
            }
            if !self.widget_editor && (self.bar || self.widget_canvas) {
                let feedback_target = crate::widgets::widget_drag::terminal_feedback(argument);
                if self.widgets_write.is_some() {
                    if let Some((pid, generation, epoch)) = feedback_target {
                        let _ = crate::widgets::widget_drag::send_app_feedback(
                            pid,
                            generation,
                            epoch,
                            false,
                            "Another widget save is still pending.",
                        );
                    }
                    if matches!(&event, UiEvent::Release) {
                        self.session.release()?;
                    }
                    return self.finish_scene(None);
                }
                let origin_x = match self.session.stores.get("bead.surface.x") {
                    Some(bead::core::Value::Number(value)) => *value,
                    _ => 0.,
                };
                let origin_y = match self.session.stores.get("bead.surface.y") {
                    Some(bead::core::Value::Number(value)) => *value,
                    _ => 0.,
                };
                let argument = if matches!(
                    &event,
                    UiEvent::Click { .. } | UiEvent::Drag { .. } | UiEvent::Release
                ) {
                    Self::widget_drag_origin_argument(argument, origin_x, origin_y)
                } else {
                    argument.clone()
                };
                let argument = self.widget_argument(name, argument);
                let feedback_target = crate::widgets::widget_drag::terminal_feedback(&argument);
                if self.widget_drag_controller.is_none() {
                    self.widget_drag_controller =
                        Some(crate::widgets::widget_drag::Controller::new()?);
                }
                self.widget_drag_controller_primed = false;
                let current = self.widget_host_values();
                let values = {
                    let controller = self
                        .widget_drag_controller
                        .as_mut()
                        .expect("controller initialized");
                    controller.sync(&current)?;
                    controller.drag(argument)?
                };
                let missing_preview = self.session.stores.get("widgets.drag.layout").is_none();
                let dragging = values.get("widgets.drag.active")
                    == Some(&bead::core::Value::Bool(true))
                    && matches!(values.get("widgets.drag.phase"),
                        Some(bead::core::Value::Text(phase)) if phase == "dragging");
                let was_dragging = self.widget_drag_model.as_ref().is_some_and(|model| {
                    model.get("widgets.drag.active") == Some(&bead::core::Value::Bool(true))
                        && matches!(model.get("widgets.drag.phase"),
                            Some(bead::core::Value::Text(phase)) if phase == "dragging")
                });
                let entered_dragging = dragging && !was_dragging;
                if entered_dragging && !missing_preview {
                    let projection = self.widget_drag_projection(&values);
                    self.publish_widget_drag_projection(projection);
                }
                let preflight: Result<Option<Scene>, Box<dyn Error>> =
                    if missing_preview && values.contains_key("widgets.drag.layout") {
                        Err(Box::new(std::io::Error::other(
                            "host does not declare widgets.drag.layout",
                        )))
                    } else {
                        let staged = self.stage_widget_drag_values(&values);
                        self.session
                            .update(staged)
                            .map_err(|error| -> Box<dyn Error> { Box::new(error) })
                            .and_then(|scene| self.finish_scene(scene))
                    };
                let mut accepted = true;
                let mut scene = match preflight {
                    Ok(scene) => scene,
                    Err(error) => {
                        accepted = false;
                        // The real bar/canvas renderer is the validation authority. Restore the
                        // last accepted gesture base and show return feedback without persisting.
                        let controller = self
                            .widget_drag_controller
                            .as_mut()
                            .expect("controller initialized");
                        let epoch = controller.drag_epoch();
                        let error_text = error.to_string();
                        let mut rollback = controller.drag(bead::core::Value::list(vec![
                            serde_json::json!({"phase":"cancel","epoch":epoch}),
                        ]))?;
                        rollback.insert(
                            "widgets.error".into(),
                            bead::core::Value::Text(error_text.clone()),
                        );
                        self.widget_drag_model = Some(rollback.clone());
                        if self.widget_drag_wanted {
                            let projection = self.widget_drag_projection(&rollback);
                            self.publish_widget_drag_projection(projection);
                        }
                        if matches!(rollback.get("widgets.drag.phase"),
                            Some(bead::core::Value::Text(phase)) if phase == "settle" || phase == "return")
                        {
                            self.retire_widget_drag_after_feedback();
                        }
                        let staged = self.stage_widget_drag_values(&rollback);
                        let restored = self.session.update(staged).ok().flatten();
                        if let Some((pid, generation, epoch)) = feedback_target {
                            let _ = crate::widgets::widget_drag::send_app_feedback(
                                pid,
                                generation,
                                epoch,
                                false,
                                &error_text,
                            );
                        }
                        self.finish_scene(restored)?
                    }
                };
                if accepted {
                    self.widget_drag_model = Some(values.clone());
                    match values.get("widgets.drag.phase") {
                        Some(bead::core::Value::Text(phase)) if phase == "dragging" => {
                            self.widget_drag_epoch =
                                self.widget_drag_epoch.wrapping_add(1).max(1u64 << 63)
                        }
                        Some(bead::core::Value::Text(phase))
                            if phase == "settle" || phase == "return" =>
                        {
                            self.retire_widget_drag_after_feedback()
                        }
                        _ => {}
                    }
                    if scene.is_none() {
                        self.publish_widget_models();
                    }
                    let write = values.get("widgets.write").and_then(|value| match value {
                        bead::core::Value::List(records) => records.first().cloned(),
                        _ => None,
                    });
                    if let Some(document) = write {
                        let identity = feedback_target
                            .map(|(pid, generation, epoch)| (pid, generation, Some(epoch)))
                            .or_else(|| {
                                crate::widgets::widget_drag::active_app_identity()
                                    .map(|(pid, generation)| (pid, generation, None))
                            });
                        self.widget_write_feedback =
                            identity.and_then(|(pid, generation, epoch)| {
                                Some(WidgetWriteFeedback {
                                    pid,
                                    generation,
                                    epoch,
                                    previous_document: widget_previous_document(
                                        &values,
                                        self.widget_seed().as_ref(),
                                    )?,
                                    previous_revision: widget_revision(&values),
                                    document: document.clone(),
                                })
                            });
                        self.begin_widget_drag_write(
                            document,
                            &values,
                            current.get("widgets.undo").cloned(),
                        )?;
                        if self.widgets_write.is_none() {
                            scene = self.refresh_widget_write_scene()?;
                            let message = self
                                .widget_drag_model
                                .as_ref()
                                .and_then(|model| model.get("widgets.error"))
                                .and_then(|value| {
                                    if let bead::core::Value::Text(error) = value {
                                        Some(error.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or("");
                            let feedback_revision = self
                                .widget_drag_model
                                .as_ref()
                                .map(widget_revision)
                                .unwrap_or_else(|| {
                                    self.widget_write_feedback
                                        .as_ref()
                                        .map_or(0, |item| item.previous_revision.saturating_add(1))
                                });
                            if let Some(feedback) = self.widget_write_feedback.take() {
                                send_widget_write_feedback(
                                    feedback,
                                    message.is_empty(),
                                    message,
                                    feedback_revision,
                                );
                            }
                        }
                    } else if let Some((pid, generation, epoch)) = feedback_target {
                        let message = values
                            .get("widgets.error")
                            .and_then(|value| {
                                if let bead::core::Value::Text(error) = value {
                                    (!error.is_empty()).then_some(error.as_str())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or("Widget drop was not accepted.");
                        let _ = crate::widgets::widget_drag::send_app_feedback(
                            pid, generation, epoch, false, message,
                        );
                    }
                } else {
                    self.retire_widget_drag_after_feedback();
                }
                if matches!(&event, UiEvent::Release) {
                    self.session.release()?;
                }
                return Ok(scene);
            }
            if !self.widget_editor {
                if let Some(wakeup) = &self.wakeup {
                    wakeup(if Self::widget_drag_motion(argument) {
                        UiEvent::CompanionMotionAction {
                            name: name.clone(),
                            argument: argument.clone(),
                        }
                    } else {
                        UiEvent::CompanionAction {
                            name: name.clone(),
                            argument: argument.clone(),
                        }
                    });
                }
                let released = if matches!(&event, UiEvent::Release) {
                    self.session.release()?
                } else {
                    None
                };
                return self.finish_scene(released);
            }
            let argument = if matches!(
                &event,
                UiEvent::Click { .. } | UiEvent::Drag { .. } | UiEvent::Release
            ) {
                let origin_x = match self.session.stores.get("bead.surface.x") {
                    Some(bead::core::Value::Number(value)) => *value,
                    _ => 0.,
                };
                let origin_y = match self.session.stores.get("bead.surface.y") {
                    Some(bead::core::Value::Number(value)) => *value,
                    _ => 0.,
                };
                Self::widget_drag_origin_argument(argument, origin_x, origin_y)
            } else {
                argument.clone()
            };
            let changes = self
                .session
                .action_values(name, self.widget_argument(name, argument))?;
            if matches!(&event, UiEvent::Release) {
                self.session.release()?;
            }
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let Some((_, name, bead::core::Value::Text(argument))) = &host_action
            && name == "plugins.do"
            && (self.widget_app || self.launcher_quick.is_some())
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let changes = crate::plugins::act(self.session.root(), argument);
            self.session.stores.commit(changes)?;
            if !self.widget_app {
                // B0208 launcher `>plugin` mode: re-project its rows from the plugin state.
                let scene = self.session.update(BTreeMap::new())?;
                return self.finish_scene(scene);
            }
            // Enable/disable/settings change the mounted plugins: rebuild from disk now; other
            // views follow through the watched plugin state file.
            let scene = self.session.reload()?;
            return self.finish_scene(Some(scene));
        }
        if let Some((_, name, argument)) = &host_action
            && name == "widgets.navigate"
            && self.widget_app
            && matches!(
                &event,
                UiEvent::Click { .. }
                    | UiEvent::Drag { .. }
                    | UiEvent::Release
                    | UiEvent::Action { .. }
                    | UiEvent::Command { .. }
            )
        {
            self.actions.authorize(name)?;
            let changes = self
                .session
                .action_values(name, self.widget_argument(name, argument.clone()))?;
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let Some((id, name, argument)) = &host_action
            && name == "widgets.edit"
            && (matches!(
                &event,
                UiEvent::Click { .. }
                    | UiEvent::Drag { .. }
                    | UiEvent::Release
                    | UiEvent::Action { .. }
            ) || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            if !self.widget_editor && (self.bar || self.widget_canvas) {
                if self.widgets_write.is_some() {
                    return Ok(None);
                }
                let argument = self.widget_argument(name, argument.clone());
                let operation = match &argument {
                    bead::core::Value::List(records) => records
                        .first()
                        .and_then(|record| record["op"].as_str())
                        .unwrap_or_default(),
                    _ => "",
                };
                if operation == "inspect" {
                    // B0208 edit mode: a click on a placed widget opens its own settings in the
                    // Widgets app. Routing may spawn or call the app, so it never blocks the UI.
                    if let bead::core::Value::List(records) = &argument
                        && let Some(record) = records.first()
                    {
                        let target = serde_json::json!({"widget": record["widget"],
                            "group": record["group"], "iid": record["iid"]});
                        let _ = std::thread::Builder::new()
                            .name("widgets-inspect".into())
                            .spawn(move || crate::app::route(0, "widgets", "open", &[target]));
                    }
                    return Ok(None);
                }
                let persistent = matches!(
                    operation,
                    "add" | "remove" | "style" | "setting" | "undo" | "reset"
                );
                if self.widget_drag_controller.is_none() {
                    self.widget_drag_controller =
                        Some(crate::widgets::widget_drag::Controller::new()?);
                }
                self.widget_drag_controller_primed = false;
                let current = self.widget_host_values();
                let values = {
                    let controller = self
                        .widget_drag_controller
                        .as_mut()
                        .expect("widget model initialized");
                    controller.sync(&current)?;
                    controller.edit(argument)?
                };
                let document = values.get("widgets.draft").and_then(|value| match value {
                    bead::core::Value::List(records) => records.first().cloned(),
                    _ => None,
                });
                let before = values.get("widgets.layout").and_then(|value| match value {
                    bead::core::Value::List(records) => records.first(),
                    _ => None,
                });
                let write_document =
                    document.filter(|document| persistent && before != Some(document));
                let staged = self.stage_widget_drag_values(&values);
                let preflight = self
                    .session
                    .update(staged)
                    .map_err(|error| -> Box<dyn Error> { Box::new(error) })
                    .and_then(|scene| self.finish_scene(scene));
                let mut scene = match preflight {
                    Ok(scene) => scene,
                    Err(error) => {
                        if self.session.stores.get("widgets.error").is_some() {
                            let restored = self
                                .session
                                .update(BTreeMap::from([(
                                    "widgets.error".into(),
                                    bead::core::Value::Text(error.to_string()),
                                )]))
                                .ok()
                                .flatten();
                            self.widget_drag_controller = None;
                            self.widget_drag_controller_primed = false;
                            self.widget_drag_model = None;
                            return self.finish_scene(restored);
                        } else {
                            return Err(error);
                        }
                    }
                };
                self.widget_drag_model = Some(values.clone());
                if scene.is_none() {
                    self.publish_widget_models();
                }
                if let Some(document) = write_document {
                    self.widget_write_feedback = crate::widgets::widget_drag::active_app_identity()
                        .and_then(|(pid, generation)| {
                            Some(WidgetWriteFeedback {
                                pid,
                                generation,
                                epoch: None,
                                previous_document: widget_previous_document(
                                    &values,
                                    self.widget_seed().as_ref(),
                                )?,
                                previous_revision: widget_revision(&values),
                                document: document.clone(),
                            })
                        });
                    self.begin_widget_drag_write(
                        document,
                        &values,
                        current.get("widgets.undo").cloned(),
                    )?;
                    if self.widgets_write.is_none() {
                        scene = self.refresh_widget_write_scene()?;
                        let message = self
                            .widget_drag_model
                            .as_ref()
                            .and_then(|model| model.get("widgets.error"))
                            .and_then(|value| {
                                if let bead::core::Value::Text(error) = value {
                                    Some(error.as_str())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or("");
                        let revision = self
                            .widget_drag_model
                            .as_ref()
                            .map(widget_revision)
                            .unwrap_or_else(|| {
                                self.widget_write_feedback
                                    .as_ref()
                                    .map_or(0, |item| item.previous_revision.saturating_add(1))
                            });
                        if let Some(feedback) = self.widget_write_feedback.take() {
                            send_widget_write_feedback(
                                feedback,
                                message.is_empty(),
                                message,
                                revision,
                            );
                        }
                    }
                }
                if matches!(&event, UiEvent::Release) {
                    self.session.release()?;
                }
                if !matches!(
                    values.get("widgets.drag.active"),
                    Some(bead::core::Value::Bool(true))
                ) {
                    self.widget_drag_controller = None;
                    self.widget_drag_controller_primed = false;
                    self.widget_drag_model = None;
                }
                return Ok(scene);
            }
            if !self.widget_editor {
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::CompanionAction {
                        name: name.clone(),
                        argument: argument.clone(),
                    });
                }
                let released = if matches!(&event, UiEvent::Release) {
                    self.session.release()?
                } else {
                    None
                };
                return self.finish_scene(released);
            }
            if self.widgets_write.is_some() {
                return Ok(None);
            }
            let mut argument = argument.clone();
            if matches!(&event, UiEvent::Click { .. } | UiEvent::Drag { .. })
                && self.session.has_preview(id)
            {
                self.pointer_feedback(id, &event, true)?;
                if let Some(pointer) = self.session.pointer_argument(id) {
                    argument = pointer;
                }
            }
            if self.widget_app {
                let operation = match &argument {
                    bead::core::Value::List(records) => records
                        .first()
                        .and_then(|record| record["op"].as_str())
                        .unwrap_or_default(),
                    _ => "",
                };
                let persistent = matches!(
                    operation,
                    "toggle-form" | "style" | "setting" | "undo" | "reset" | "default-trigger"
                );
                let old_layout = self
                    .session
                    .stores
                    .get("widgets.layout")
                    .cloned()
                    .unwrap_or_else(|| bead::core::Value::list(Vec::new()));
                let old_undo = self
                    .session
                    .stores
                    .get("widgets.undo")
                    .cloned()
                    .unwrap_or_else(|| bead::core::Value::list(Vec::new()));
                let changes = self
                    .session
                    .action_values(name, self.widget_argument(name, argument))?;
                let Some(document) = changes.get("widgets.draft").and_then(|value| match value {
                    bead::core::Value::List(records) => records.first().cloned(),
                    _ => None,
                }) else {
                    let scene = self.session.update(changes)?;
                    return self.finish_scene(scene);
                };
                let before = match &old_layout {
                    bead::core::Value::List(records) => records.first(),
                    _ => None,
                };
                if !persistent || before == Some(&document) {
                    let scene = self.session.update(changes)?;
                    return self.finish_scene(scene);
                }
                let old_revision = match self.session.stores.get("widgets.rev") {
                    Some(bead::core::Value::Number(revision)) => *revision as u64,
                    _ => 0,
                };
                self.widgets_write_previous =
                    Some((old_layout.clone(), old_undo.clone(), old_revision));
                let mut changes = changes;
                changes.insert("widgets.busy".into(), bead::core::Value::Bool(true));
                changes.insert(
                    "widgets.error".into(),
                    bead::core::Value::Text(String::new()),
                );
                let scene = self.session.update(changes)?;
                let result: Result<WidgetWriteResult, String> = if let Some(client) =
                    &self.widgets_client
                {
                    let status = client.status();
                    (|| -> Result<_, String> {
                        let method = status
                            .methods
                            .get("settings.set")
                            .filter(|method| method.v == 1)
                            .ok_or("Widget settings are unavailable")?;
                        let boot = status
                            .boot
                            .clone()
                            .ok_or("Widget settings are disconnected")?;
                        let rev = match self.session.stores.get("widgets.rev") {
                            Some(bead::core::Value::Number(rev)) => *rev as u64,
                            _ => 0,
                        };
                        let wakeup = self
                            .wakeup
                            .clone()
                            .ok_or("Widget event channel unavailable")?;
                        client.begin_call(
                                "settings.set".into(),
                                serde_json::json!({"store":"widgets","key":["layout"],"value":document,"rev":rev}),
                                std::time::Duration::from_millis(u64::from(method.timeout).clamp(1, 30_000)),
                                crate::nacre_source::CallGuard { boot, version: 1 },
                                Some(std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new())))),
                            ).map(WidgetWriteResult::Pending).map_err(|error| error.message)
                    })()
                } else {
                    self.widgets_file
                        .as_deref()
                        .ok_or("Widget layout path unavailable".to_owned())
                        .and_then(|path| {
                            let json = serde_json::to_value(&document)
                                .map_err(|error| error.to_string())?;
                            crate::widgets::save_file_revisioned(path, &json, old_revision)
                                .map_err(|error| error.to_string())
                        })
                        .map(WidgetWriteResult::Saved)
                };
                let final_scene = match result {
                    Ok(WidgetWriteResult::Pending(ticket)) => {
                        self.widgets_write = Some(ticket);
                        self.widgets_write_document = Some(bead::core::Value::list(vec![document]));
                        scene
                    }
                    Ok(WidgetWriteResult::Saved(rev)) => {
                        self.widgets_write_previous = None;
                        self.session
                            .update(BTreeMap::from([
                                (
                                    "widgets.layout".into(),
                                    bead::core::Value::list(vec![document]),
                                ),
                                ("widgets.draft".into(), bead::core::Value::list(Vec::new())),
                                ("widgets.busy".into(), bead::core::Value::Bool(false)),
                                ("widgets.rev".into(), bead::core::Value::Number(rev as f64)),
                            ]))?
                            .or(scene)
                    }
                    Err(error) => {
                        self.widgets_write_previous = None;
                        self.session
                            .update(BTreeMap::from([
                                ("widgets.layout".into(), old_layout),
                                ("widgets.draft".into(), bead::core::Value::list(Vec::new())),
                                ("widgets.undo".into(), old_undo),
                                ("widgets.busy".into(), bead::core::Value::Bool(false)),
                                ("widgets.error".into(), bead::core::Value::Text(error)),
                            ]))?
                            .or(scene)
                    }
                };
                return self.finish_scene(final_scene);
            }
            let mut changes = self
                .session
                .action_values(name, self.widget_argument(name, argument))?;
            let write = changes.get("widgets.write").and_then(|value| match value {
                bead::core::Value::List(items) => items.first().cloned(),
                _ => None,
            });
            changes.insert("widgets.write".into(), bead::core::Value::list(Vec::new()));
            if write.is_some() {
                changes.insert("widgets.editing".into(), bead::core::Value::Bool(true));
            }
            // The markup registry validates the draft before any external write.
            let scene = self.session.update(changes.clone())?;
            if let Some(document) = write {
                let result: Result<Option<u64>, String> = if let Some(client) = &self.widgets_client
                {
                    let status = client.status();
                    let result = (|| -> Result<_, String> {
                        let method = status
                            .methods
                            .get("settings.set")
                            .filter(|method| method.v == 1)
                            .ok_or("Widget settings are unavailable")?;
                        let boot = status
                            .boot
                            .clone()
                            .ok_or("Widget settings are disconnected")?;
                        let rev = match changes.get("widgets.base_rev") {
                            Some(bead::core::Value::Number(rev)) => *rev as u64,
                            _ => 0,
                        };
                        let wakeup = self
                            .wakeup
                            .clone()
                            .ok_or("Widget event channel unavailable")?;
                        client.begin_call(
                            "settings.set".into(),
                            serde_json::json!({"store":"widgets","key":["layout"],"value":document,"rev":rev}),
                            std::time::Duration::from_millis(u64::from(method.timeout).clamp(1, 30_000)),
                            crate::nacre_source::CallGuard { boot, version: 1 },
                            Some(std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new())))),
                        ).map_err(|error| error.message)
                    })();
                    match result {
                        Ok(ticket) => {
                            self.widgets_write = Some(ticket);
                            self.widgets_write_document =
                                Some(bead::core::Value::list(vec![document.clone()]));
                            Ok(None)
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    self.widgets_file
                        .as_deref()
                        .ok_or("Widget layout path unavailable".to_owned())
                        .and_then(|path| {
                            let expected = match changes.get("widgets.base_rev") {
                                Some(bead::core::Value::Number(rev)) => *rev as u64,
                                _ => match self.session.stores.get("widgets.rev") {
                                    Some(bead::core::Value::Number(rev)) => *rev as u64,
                                    _ => 0,
                                },
                            };
                            crate::widgets::save_file_revisioned(path, &document, expected)
                                .map(Some)
                                .map_err(|error| error.to_string())
                        })
                };
                if let Ok(Some(revision)) = result.as_ref() {
                    changes.insert(
                        "widgets.layout".into(),
                        bead::core::Value::list(vec![document]),
                    );
                    changes.insert(
                        "widgets.rev".into(),
                        bead::core::Value::Number(*revision as f64),
                    );
                    changes.insert("widgets.editing".into(), bead::core::Value::Bool(false));
                }
                if let Err(error) = result {
                    let failed = self.session.update(BTreeMap::from([
                        ("widgets.editing".into(), bead::core::Value::Bool(true)),
                        ("widgets.error".into(), bead::core::Value::Text(error)),
                    ]))?;
                    return self.finish_scene(failed.or(scene));
                }
            }
            if self.widgets_write.is_none()
                && changes.get("widgets.editing") == Some(&bead::core::Value::Bool(false))
            {
                self.widgets_close_on_commit = true;
            }
            let final_scene = self.session.update(changes)?.or(scene);
            return self.finish_scene(final_scene);
        }
        if matches!(&event, UiEvent::Click { .. } | UiEvent::Drag { .. })
            && let Some((id, action_name, argument)) = &mut host_action
            && self.session.has_preview(id)
        {
            // Retained ranges already painted the frame; update their value and
            // authored argument without materializing or laying out the panel.
            if local {
                let motion = self.session.view_nodes().iter().find_map(|node| {
                    let scope = node.id.rsplit_once('.')?.0;
                    if !id.starts_with(&format!("{scope}.")) {
                        return None;
                    }
                    let bead::core::Value::List(records) = node.metadata.as_ref()? else {
                        return None;
                    };
                    let motion = records.first()?.get("pointer_motion")?;
                    Some((
                        node.id.clone(),
                        motion.get("argument")?.clone(),
                        motion.get("label").cloned(),
                    ))
                });
                let (thumb_id, format, label) = motion.map_or((None, None, None), |item| {
                    (Some(item.0), Some(item.1), item.2)
                });
                let label = label.and_then(|mut label| {
                    let authored_id = label.get("id")?.as_str()?;
                    let relative = authored_id.strip_prefix("../");
                    if let Some(relative) = relative {
                        let slider_id = thumb_id.as_deref()?.rsplit_once('.')?.0;
                        let owner = slider_id.rsplit_once('.').map_or("", |(owner, _)| owner);
                        let target = if owner.is_empty() {
                            relative.to_owned()
                        } else {
                            format!("{owner}.{relative}")
                        };
                        label["id"] = serde_json::Value::String(target);
                    }
                    Some(label)
                });
                if self.control.is_none() || action_name != "cc.do" || format.is_none() {
                    local = false;
                }
                if local && let UiEvent::Drag { x, y } = &event {
                    if let Some(feedback) = self.session.local_range_feedback_with_label(
                        id,
                        *x,
                        format.as_ref(),
                        label.as_ref(),
                    )? {
                        *argument = feedback.argument;
                        pointer_scene = feedback.scene;
                        self.slider_local_active = true;
                        bead::core::profile::position("slider_local_feedback", *x, *y);
                        bead::core::profile::mark("slider_local_feedback");
                    } else {
                        local = false;
                    }
                }
            }
            if !local {
                self.slider_local_active = false;
                pointer_scene = self.pointer_feedback(id, &event, true)?;
                if let Some(continuous) = self.session.pointer_argument(id) {
                    *argument = continuous;
                }
            }
            if self
                .slider_sent
                .is_some_and(|sent| sent.elapsed() < std::time::Duration::from_millis(34))
            {
                self.slider_pending = Some(if local {
                    UiEvent::LocalDrag {
                        x: self.pointer_at.0,
                        y: self.pointer_at.1,
                    }
                } else {
                    event
                });
                return self.finish_scene(pointer_scene);
            }
            self.slider_pending = None;
            self.slider_sent = Some(std::time::Instant::now());
        }
        if let Some((_, name, _)) = &mut host_action
            && name == "services.tray"
            && matches!(&event, UiEvent::SecondaryClick { .. })
        {
            *name = "services.tray.secondary".into();
        }
        // Notices act on click or Enter; right-click on a card dismisses it (QML parity),
        // while scrolling or right-clicking a button never fires an action.
        if let Some((_, name, _)) = &mut host_action
            && name.starts_with("notifications.")
        {
            match &event {
                UiEvent::Click { .. } | UiEvent::Release | UiEvent::Command { .. } => {}
                UiEvent::ContextMenu { .. } if name == "notifications.card" => {
                    *name = "notifications.dismiss".into();
                }
                _ => host_action = None,
            }
        }
        if matches!(&event, UiEvent::SecondaryClick { .. })
            && !host_action.as_ref().is_some_and(|(id, name, arg)| {
                name == "services.tray.secondary"
                    || (self.routes.is_some() && self.bar_route(&event, id, name, arg).is_some())
            })
        {
            return Ok(None);
        }
        if matches!(&event, UiEvent::HorizontalScroll { .. })
            && !host_action
                .as_ref()
                .is_some_and(|(_, name, _)| name == "services.tray")
        {
            return Ok(None);
        }
        if self.bar
            && self.status_pill.panel().is_some()
            && matches!(&event, UiEvent::Click { .. })
            && !host_action.as_ref().is_some_and(|(_, name, _)| {
                matches!(
                    name.as_str(),
                    "status.pill.open" | "view.open" | "updates.open"
                )
            })
            && let Some(zone) = self.status_keep_open_zone()
            && !zone.contains(self.pointer_at)
        {
            self.status_pill.outside_click(self.pointer_at, zone);
            self.schedule_status_pill();
            self.sync_status_peek_route()?;
            let scene = self.publish_status_pill()?;
            if host_action.is_none() {
                return self.finish_scene(scene);
            }
        }
        if !self.dock_status.is_empty()
            && (matches!(&event, UiEvent::Click { .. })
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
            && host_action
                .as_ref()
                .is_some_and(|(_, name, _)| matches!(name.as_str(), "page" | "detail"))
            && let Some(status) = self.active_status()
        {
            status.network_cancel();
            status.bluetooth_pair_leave();
        }
        if let Some((_, name, arg)) = &mut host_action
            && name == "view.open"
            && *arg == bead::core::Value::Text("sound".into())
            && matches!(&event, UiEvent::Scroll { .. })
        {
            *name = "services.audio".into();
        }
        if let Some((_, name, arg)) = &mut host_action
            && name == "status.pill.open"
            && *arg == bead::core::Value::Text("audio".into())
            && matches!(&event, UiEvent::Scroll { .. })
        {
            *name = "services.audio".into();
        }
        if let Some((_, name, bead::core::Value::Text(view))) = &host_action
            && name == "dashboard.select"
            && let Some(page) = crate::dashboard::Page::from_view(&format!("dashboard-{view}"))
            && self.dashboard.is_some()
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            self.status_pill.escape();
            self.status_pill_route = None;
            self.schedule_status_pill();
            let status_scene = self.publish_status_pill()?;
            let scene = self.select_dashboard_page(page)?.or(status_scene);
            return self.finish_scene(scene);
        }
        if let Some((id, name, bead::core::Value::Text(view))) = &host_action
            && name == "status.pill.open"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            let owner = if id.is_empty() {
                view.clone()
            } else {
                id.clone()
            };
            if !self.consume_dismissed_owner_click(&owner) {
                self.status_pill.click_icon(owner);
            }
            self.schedule_status_pill();
            let scene = self.publish_status_pill()?;
            self.actions.authorize(name)?;
            self.sync_status_peek_route()?;
            return self.finish_scene(scene);
        }
        if let Some((_, name, _)) = &host_action
            && matches!(name.as_str(), "updates.refresh" | "updates.open")
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("updates completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = self.active_status() {
                status.updates_action(name, callback);
            }
            let scene = self.session.update(BTreeMap::new())?;
            return self.finish_scene(scene);
        }
        if let Some((id, name, bead::core::Value::Text(view))) = &host_action
            && self.bar
            && name == "view.open"
            && matches!(view.as_str(), "calendar" | "session")
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            if !self.consume_dismissed_owner_click(id) {
                self.status_pill.click_icon(id.clone());
            }
            self.schedule_status_pill();
            self.actions.authorize(name)?;
            self.sync_status_peek_route()?;
            let scene = self.publish_status_pill()?;
            return self.finish_scene(scene);
        }
        if let Some((id, name, bead::core::Value::Text(view))) = &host_action
            && name == "view.open"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command {command,..} if command == "enter"))
        {
            self.status_pill.escape();
            self.status_pill_route = None;
            self.schedule_status_pill();
            let scene = self.publish_status_pill()?;
            self.actions.authorize(name)?;
            self.wakeup
                .as_ref()
                .ok_or("view event channel unavailable")?(UiEvent::CompanionSelect {
                view: view.clone(),
                output: None,
                toggle: true,
                center: self.popout_center(id, view),
            });
            return self.finish_scene(scene);
        }
        if let Some((_, name, _)) = &host_action
            && matches!(name.as_str(), "network.toggle_wifi" | "network.scan")
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("Wi-Fi completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = self.active_status() {
                if name == "network.scan" {
                    status.network_scan(callback);
                } else {
                    status.wifi_action(callback);
                }
            }
            let scene = self.session.update(BTreeMap::new())?;
            return self.finish_scene(scene);
        }
        if let Some((node, name, arg)) = &host_action
            && matches!(
                name.as_str(),
                "network.open"
                    | "network.cancel"
                    | "network.join"
                    | "network.forget_ask"
                    | "network.forget"
            )
            && ((matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                && !node.ends_with("field"))
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("network completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            // Whatever happens, a typed password leaves the input buffer once it was used.
            let mut clear = BTreeMap::from([(
                crate::status_views::SECRET_STORE.to_owned(),
                bead::core::Value::Text(String::new()),
            )]);
            if let Some(status) = self.active_status() {
                match (name.as_str(), arg) {
                    ("network.open", bead::core::Value::Text(view)) => status.network_open(view),
                    ("network.cancel", _) => {
                        status.network_cancel();
                        clear.insert(
                            "network.hidden_ssid".into(),
                            bead::core::Value::Text(String::new()),
                        );
                    }
                    ("network.join", _) => status.network_join(&self.session.stores, callback),
                    ("network.forget_ask", bead::core::Value::Text(key)) => {
                        status.network_forget(key, false, callback)
                    }
                    ("network.forget", bead::core::Value::Text(key)) => {
                        status.network_forget(key, true, callback)
                    }
                    _ => {}
                }
            }
            clear.insert("network.reveal".into(), bead::core::Value::Bool(false));
            let scene = self.session.update(clear.clone())?;
            let scene = self.focus_network_form(scene, clear)?;
            return self.finish_scene(scene);
        }
        if let Some((_, name, bead::core::Value::Text(key))) = &host_action
            && (name == "network.connect" || name == "network.disconnect")
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("network completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = self.active_status() {
                status.network_link_action(name, key, callback);
            }
            let scene = self.session.update(BTreeMap::new())?;
            let scene = self.focus_network_form(scene, BTreeMap::new())?;
            return self.finish_scene(scene);
        }
        if let (Some(dashboard), Some((_, name, bead::core::Value::Text(arg)))) =
            (&self.dashboard, &host_action)
            && name == "dashboard.do"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("dashboard completion event channel unavailable")?;
            let wake: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            dashboard.borrow().act(arg, wake);
            let released = if matches!(&event, UiEvent::Release) {
                self.session.release()?
            } else {
                None
            };
            let scene = self.session.update(BTreeMap::new())?.or(released);
            return self.finish_scene(scene);
        }
        if let (Some(control), Some((id, name, bead::core::Value::Text(arg)))) =
            (&self.control, &host_action)
            && name == "cc.do"
            && (matches!(
                &event,
                UiEvent::Click { .. } | UiEvent::Drag { .. } | UiEvent::Release
            ) || matches!(&event, UiEvent::Command { command, .. } if command == "enter" || command == "left" || command == "right"))
        {
            self.actions.authorize(name)?;
            control.act(arg);
            // An interactive tool (recorder, screenshot, picker) starts after the panel is gone.
            if control.take_close() {
                let scene = self.session.update(BTreeMap::from([(
                    "panel.open".into(),
                    bead::core::Value::Bool(false),
                )]))?;
                return self.finish_scene(scene);
            }
            if local {
                return if pointer_scene.is_some() {
                    self.finish_scene(pointer_scene)
                } else {
                    Ok(None)
                };
            }
            // A level step keeps the focus so the drag continues over the next step.
            let scene = match pointer_scene.take().map_or_else(
                || self.pointer_feedback(id, &event, true),
                |scene| Ok(Some(scene)),
            ) {
                Ok(scene) => scene,
                Err(_) => self.session.update(BTreeMap::new())?,
            };
            return self.finish_scene(scene);
        }
        if let (Some(chat), Some((id, name, bead::core::Value::Text(arg)))) =
            (&self.chat, &host_action)
            && name == "chat.do"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, shift: false } if command == "enter"))
        {
            if self.trace || std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() {
                let event_name = match &event {
                    UiEvent::Click { .. } => "click",
                    UiEvent::Release => "release",
                    UiEvent::Command { .. } => "command",
                    _ => "other",
                };
                eprintln!("INPUT_HIT surface=ai-chat node={id} event={event_name}");
            }
            self.actions.authorize(name)?;
            let mut outcome = chat.act(arg, &self.session.stores);
            if matches!(&event, UiEvent::Release) {
                outcome
                    .stores
                    .insert("bead.pressed".into(), bead::core::Value::Bool(false));
                outcome
                    .stores
                    .insert("bead.focused".into(), bead::core::Value::Text(id.clone()));
            }
            if let Some(copied) = outcome.copy {
                self.wakeup
                    .as_ref()
                    .ok_or("clipboard event channel unavailable")?(
                    UiEvent::ClipboardSet(copied)
                );
            }
            let scene = self.session.update(outcome.stores)?;
            return self.finish_scene(scene);
        }
        if let (Some(settings), Some((id, name, bead::core::Value::Text(arg)))) =
            (&self.settings, &host_action)
            && name == "settings.do"
            && (matches!(
                &event,
                UiEvent::Click { .. } | UiEvent::Drag { .. } | UiEvent::Release
            ) || matches!(&event, UiEvent::Command { command, .. } if command == "enter" || command == "left" || command == "right"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("settings completion event channel unavailable")?;
            let wake: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            let edit = match self.session.stores.get("settings.edit.text") {
                Some(bead::core::Value::Text(t)) => t.clone(),
                _ => String::new(),
            };
            if let Some(bead::core::Value::Text(secret)) =
                self.session.stores.get("settings.secret")
            {
                settings.secret(secret);
            }
            let mut changes = settings.act(arg, &edit, wake);
            // Any settings action (a choice in an open drop-down, a page change) closes the open list.
            if !arg.starts_with("do:vol|") {
                changes.insert(
                    "settings.select".into(),
                    bead::core::Value::Text(String::new()),
                );
            }
            // A slider step keeps the focus so the drag continues over the next step.
            let scene = if matches!(&event, UiEvent::Release) {
                self.session.update(changes)?
            } else {
                match arg.starts_with("do:vol|").then(|| {
                    pointer_scene.take().map_or_else(
                        || self.pointer_feedback(id, &event, true),
                        |scene| Ok(Some(scene)),
                    )
                }) {
                    Some(Ok(scene)) => scene,
                    _ => self.session.update(changes)?,
                }
            };
            return self.finish_scene(scene);
        }
        if let (Some(colors), Some((id, name, bead::core::Value::Text(arg)))) =
            (&self.colors, &host_action)
            && name == "colors.do"
            && (matches!(
                &event,
                UiEvent::Click { .. } | UiEvent::Drag { .. } | UiEvent::Release
            ) || matches!(&event, UiEvent::Command { command, .. } if command == "enter" || command == "left" || command == "right"))
        {
            self.actions.authorize(name)?;
            let changes = colors.act(arg, &self.session.stores);
            // A slider step keeps the focus so the drag continues over the next step.
            let level = arg.starts_with("brightness:") || arg.starts_with("saturation:");
            let scene = match level.then(|| self.pointer_feedback(id, &event, false)) {
                Some(Ok(scene)) => scene,
                _ => self.session.update(changes)?,
            };
            return self.finish_scene(scene);
        }
        if let (Some(welcome), Some((_, name, bead::core::Value::Text(arg)))) =
            (&self.welcome, &host_action)
            && name == "welcome.do"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let outcome = welcome.act(arg, &self.session.stores);
            if outcome.close {
                // The window retires itself, as a panel's view does.
                self.wakeup
                    .as_ref()
                    .ok_or("window close event channel unavailable")?(
                    UiEvent::CompanionVisibility(false),
                );
            }
            let scene = self.session.update(outcome.changes)?;
            return self.finish_scene(scene);
        }
        if let (Some(prompt), Some((_, name, bead::core::Value::Text(request)))) =
            (&self.prompt, &host_action)
            && name == "prompt.do"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let changes = prompt.act(request, &self.session.stores);
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let (Some(extras), Some((_, name, bead::core::Value::Text(request)))) =
            (&self.extras, &host_action)
            && name == "extras.do"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("extras event channel unavailable")?;
            let mut changes = BTreeMap::new();
            let service_request = if let Some(id) = request.strip_prefix("wallpaper:select:") {
                changes.extend(
                    self.session
                        .action_values("extras.select", bead::core::Value::Text(id.into()))?,
                );
                format!("wallpaper:activate:{id}")
            } else {
                request.clone()
            };
            extras.act(
                &service_request,
                &self.session.stores,
                std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new()))),
            );
            changes.insert(
                "extras.request".into(),
                bead::core::Value::Text(String::new()),
            );
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let (Some(menu), Some((_, name, arg))) = (&self.session_menu, &host_action)
            && name.starts_with("session.")
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("session completion event channel unavailable")?;
            let wake: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            match (name.as_str(), arg) {
                ("session.select", bead::core::Value::Text(key)) => menu.select(key, wake),
                ("session.confirm", _) => menu.confirm(wake),
                ("session.cancel", _) => menu.cancel(),
                _ => {}
            }
            // Keyboard path: the item that now asks holds focus, so Enter confirms.
            let focus = menu.confirming().map_or(BTreeMap::new(), |key| {
                BTreeMap::from([(
                    "bead.focused".into(),
                    bead::core::Value::Text(format!("session-popout.act-{key}.main")),
                )])
            });
            let scene = self.session.update(focus)?;
            return self.finish_scene(scene);
        }
        if let Some((_, name, _)) = &host_action
            && name == "bluetooth.toggle_power"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("Bluetooth completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = self.active_status() {
                status.bluetooth_action(callback);
            }
            let scene = self.session.update(BTreeMap::new())?;
            return self.finish_scene(scene);
        }
        if let Some((node, name, arg)) = &host_action
            && matches!(
                name.as_str(),
                "bluetooth.pair"
                    | "bluetooth.scan"
                    | "bluetooth.link"
                    | "bluetooth.pair_respond"
                    | "bluetooth.pair_cancel"
                    | "bluetooth.pair_retry"
            )
            && ((matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                && !node.ends_with("field"))
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("Bluetooth completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            // A typed PIN or passkey leaves the input buffer once it was used or the flow ended.
            let mut clear = true;
            if let Some(status) = self.active_status() {
                match (name.as_str(), arg) {
                    ("bluetooth.pair", bead::core::Value::Text(key)) => {
                        status.bluetooth_pair(key, callback)
                    }
                    ("bluetooth.link", bead::core::Value::Text(key)) => {
                        status.bluetooth_link(key, callback)
                    }
                    ("bluetooth.scan", _) => status.bluetooth_scan(callback),
                    ("bluetooth.pair_respond", bead::core::Value::Text(answer)) => {
                        clear =
                            status.bluetooth_pair_respond(answer == "accept", &self.session.stores);
                    }
                    ("bluetooth.pair_cancel", _) => status.bluetooth_pair_cancel(),
                    ("bluetooth.pair_retry", _) => status.bluetooth_pair_retry(callback),
                    _ => {}
                }
            }
            let changes = if clear {
                BTreeMap::from([
                    (
                        crate::status_views::BLUETOOTH_SECRET_STORE.to_owned(),
                        bead::core::Value::Text(String::new()),
                    ),
                    ("bluetooth.reveal".into(), bead::core::Value::Bool(false)),
                ])
            } else {
                BTreeMap::new()
            };
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let (Some((_, client)), Some((_, name, _))) = (&self.recording_topic, &host_action)
            && name == "bar.recording.stop"
            && matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
        {
            self.actions.authorize(name)?;
            let client = client.clone();
            // The daemon stops the recorder and cleans up its sound mix; the topic then hides the item.
            std::thread::spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                if let Err(error) = runtime.block_on(client.call(
                    "recorder.stop".into(),
                    serde_json::json!({}),
                    std::time::Duration::from_secs(5),
                )) {
                    eprintln!(
                        "BAR_ACTION_REFUSED method=recorder.stop reason={}",
                        error.message
                    );
                }
            });
            return self.finish_scene(None);
        }
        if let Some((_, name, _)) = &host_action
            && name == "launcher.open"
            && self.launcher_trigger
        {
            self.actions.authorize(name)?;
            self.wakeup
                .as_ref()
                .ok_or("launcher event channel unavailable")?(
                UiEvent::CompanionVisibility(true)
            );
            return self.finish_scene(None);
        }
        if let (Some(quick), Some((_, name, bead::core::Value::Text(id)))) =
            (&self.launcher_quick, &host_action)
            && name == "launcher.quick"
            && (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            quick.act(id);
            let scene = if matches!(&event, UiEvent::Release) {
                None
            } else {
                self.session.update(BTreeMap::new())?
            };
            return self.finish_scene(scene);
        }
        if let (Some(menu), Some((_, name, bead::core::Value::Text(row_id)))) =
            (&self.launcher, &host_action)
            && name == "toggle_favorite"
        {
            self.actions.authorize(name)?;
            let changes = match menu.favorite(row_id) {
                Ok(()) => BTreeMap::new(),
                Err(error) => BTreeMap::from([(
                    "launcher.error".into(),
                    bead::core::Value::Text(error.message),
                )]),
            };
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let (Some(menu), UiEvent::Command { command, .. }) = (&self.launcher, &event)
            && (self.session.surrounding_text().is_some()
                || host_action
                    .as_ref()
                    .is_some_and(|(_, name, _)| name == "select_app"))
            && matches!(command.as_str(), "up" | "down" | "left" | "right")
        {
            let buttons: Vec<_> = self
                .session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("select_app"))
                .collect();
            if let Some(first) = buttons.first() {
                let first_rect = bead::ui::rect(&first.draw);
                let columns = buttons
                    .iter()
                    .filter(|node| (bead::ui::rect(&node.draw).y - first_rect.y).abs() < 0.5)
                    .count()
                    .max(1);
                let pitch = buttons
                    .iter()
                    .map(|node| f64::from(bead::ui::rect(&node.draw).y - first_rect.y))
                    .filter(|distance| *distance > 0.5)
                    .min_by(f64::total_cmp)
                    .unwrap_or(f64::from(first_rect.h));
                let viewport_height = self
                    .session
                    .view_nodes()
                    .iter()
                    .find(|node| node.scroll_store.as_deref() == Some("launcher.scroll.y"))
                    .map(|node| bead::ui::rect(&node.draw).h)
                    .unwrap_or_else(|| first.clip.map_or(first_rect.h, |clip| clip.h));
                let geometry = crate::launcher::NavigationGeometry {
                    columns,
                    pitch,
                    row_height: f64::from(first_rect.h),
                    viewport_height: f64::from(viewport_height),
                };
                if let Some(changes) = menu.navigate(&self.session.stores, command, geometry) {
                    let scene = self.session.update(changes)?;
                    let scene = self.finish_scene(scene)?;
                    if bead::core::profile::enabled() {
                        println!("MENU_NAV_DONE changed={}", scene.is_some());
                    }
                    return Ok(scene);
                }
            }
        }
        // The launcher's inline form (B0199): its buttons, and Enter in a one-line field.
        if let Some(menu) = &self.launcher {
            let form_open = matches!(self.session.stores.get("launcher.form.kind"),
                Some(bead::core::Value::Text(kind)) if !kind.is_empty());
            let request = match (&host_action, &event) {
                (Some((_, name, bead::core::Value::Text(request))), _)
                    if name == "launcher.form" =>
                {
                    (matches!(&event, UiEvent::Click { .. } | UiEvent::Release)
                        || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
                    .then(|| request.clone())
                }
                (_, UiEvent::Command { command, .. })
                    if form_open
                        && command == "enter"
                        && self.session.surrounding_text().is_some()
                        && !self.session.focused_multiline() =>
                {
                    Some("submit".to_owned())
                }
                _ => None,
            };
            if let Some(request) = request {
                self.actions.authorize("launcher.form")?;
                let changes = menu.form_act(&request, &self.session.stores);
                let scene = self.session.update(changes)?;
                return self.finish_scene(scene);
            }
        }
        let launcher_id = if let Some((_, name, bead::core::Value::Text(id))) = &host_action {
            (name == "select_app").then(|| id.clone())
        } else if matches!(&event,UiEvent::Command{command,..} if command=="enter")
            && self.session.surrounding_text().is_some()
        {
            match self.session.stores.get("launcher.selection") {
                Some(bead::core::Value::Text(id)) => Some(id.clone()),
                _ => None,
            }
        } else {
            None
        };
        if let (Some(menu), Some(id)) = (&self.launcher, launcher_id) {
            self.actions.authorize("select_app")?;
            let changes = match menu.activate(&id) {
                Ok(result) => crate::launcher::activation_changes(result),
                Err(error) => std::collections::BTreeMap::from([(
                    "launcher.error".into(),
                    bead::core::Value::Text(error.message),
                )]),
            };
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene);
        }
        if let Some((_, name, arg)) = host_action.as_ref()
            && name.starts_with("notifications.")
        {
            self.actions.authorize(name)?;
            self.notifications
                .as_ref()
                .ok_or("notification server unavailable")?
                .action(name, arg)?;
            return self.finish_scene(None);
        }
        if let Some(routes) = &self.routes
            && let Some((id, name, arg)) = host_action.as_ref()
            && (name.starts_with("services.")
                || name == "bar.title"
                || name == "backend.workspace.switch"
                || name == "backend.window.focus"
                || (name == "status.pill.open"
                    && matches!(
                        event,
                        UiEvent::Scroll { .. } | UiEvent::SecondaryClick { .. }
                    )))
        {
            // Daemon bar mode: the published route or nothing. Unrouted events
            // (and controls the daemon does not advertise) never fall back to
            // the local compositor, audio or tray paths.
            self.actions.authorize(name)?;
            if let Some(route) = self.bar_route(&event, id, name, arg)
                && let Err(reason) =
                    routes.send(&route, self.output.as_deref(), std::sync::Arc::new(|| {}))
            {
                eprintln!(
                    "BAR_ACTION_REFUSED method={} reason={reason}",
                    route.method()
                );
            }
            if self.session.has_preview(id)
                && matches!(&event, UiEvent::Click { .. } | UiEvent::Drag { .. })
            {
                let scene = pointer_scene.take().map_or_else(
                    || self.pointer_feedback(id, &event, true),
                    |scene| Ok(Some(scene)),
                )?;
                return self.finish_scene(scene);
            }
            return self.finish_scene(None);
        }
        if let Some((id, name, arg)) = host_action.as_ref()
            && name.starts_with("services.")
            && (!matches!(
                &event,
                UiEvent::Scroll { .. } | UiEvent::HorizontalScroll { .. }
            ) || name == "services.audio"
                || name == "services.tray")
        {
            let delta = match &event {
                UiEvent::Scroll { delta, .. } | UiEvent::HorizontalScroll { delta, .. } => {
                    Some(*delta)
                }
                _ => None,
            };
            if delta.is_none() || name == "services.audio" || name == "services.tray" {
                self.tray_anchor = self.popout_center(id, "tray");
                let result = self.desktop_action(
                    name,
                    arg.clone(),
                    delta,
                    matches!(event, UiEvent::ContextMenu { .. }),
                    matches!(event, UiEvent::HorizontalScroll { .. }),
                );
                if name.starts_with("services.tray") || name.starts_with("services.menu") {
                    if result.is_err() {
                        eprintln!("TRAY_ACTION unavailable_or_stale");
                    }
                } else {
                    result?;
                }
            }
            if self.session.has_preview(id)
                && matches!(&event, UiEvent::Click { .. } | UiEvent::Drag { .. })
            {
                let scene = pointer_scene.take().map_or_else(
                    || self.pointer_feedback(id, &event, true),
                    |scene| Ok(Some(scene)),
                )?;
                return self.finish_scene(scene);
            }
            if !self.bar {
                self.pending_focus = Some(id.clone());
            }
            event = UiEvent::Desktop(
                self.desktop
                    .as_ref()
                    .ok_or("desktop unavailable after action")?
                    .snapshot(),
            );
        }
        if let Some((id, name, arg)) = host_action
            && name.starts_with("backend.")
            && !matches!(event, UiEvent::Scroll { .. } | UiEvent::ContextMenu { .. })
        {
            self.backend_action(&name, arg)?;
            if !self.bar {
                let scene = self.session.focus_id(&id)?;
                return self.finish_scene(scene);
            }
            event = UiEvent::Relayout;
        }
        let reload = matches!(event, UiEvent::Reload);
        if reload {
            self.fonts.clear();
            if self.launcher_initial_projection.get()
                && let Some(menu) = &self.launcher
            {
                menu.prime_initial(&self.session.stores)?;
            }
        }
        let mut scene = match event {
            UiEvent::WallpaperPalette(values) => {
                self.session
                    .set_runtime_palette(bead::render::Palette(std::sync::Arc::new(
                        values.into_iter().collect(),
                    )))?
            }
            UiEvent::Model(mut values) => {
                if values.remove(WALLPAPER_LUMA_POKE).is_some() {
                    values.extend(crate::wallpaper_luma::values(self.output.as_deref()));
                }
                let policy = match (
                    values.remove("widgets.policy.pid"),
                    values.remove("widgets.policy.generation"),
                    values.remove("widgets.policy.open"),
                    values.remove("widgets.policy.floating"),
                ) {
                    (
                        Some(bead::core::Value::Number(pid)),
                        Some(bead::core::Value::Number(generation)),
                        Some(bead::core::Value::Bool(open)),
                        Some(bead::core::Value::Bool(floating)),
                    ) if pid.is_finite()
                        && pid > 0.
                        && pid <= u32::MAX as f64
                        && generation.is_finite()
                        && generation > 0. =>
                    {
                        Some((pid as u32, generation as u64, open, floating))
                    }
                    _ => None,
                };
                if let Some((pid, generation, open, floating)) = policy
                    && !self.widget_app_state(pid, generation, open, floating)
                {
                    values.remove("widgets.app.open");
                    values.remove("widgets.editing");
                }
                if let Some((x, done)) = self.advance_dashboard_pearl() {
                    values.insert("dashboard.pearl.x".into(), bead::core::Value::Number(x));
                    if done {
                        values.insert(
                            "dashboard.pearl.moving".into(),
                            bead::core::Value::Bool(false),
                        );
                    }
                }
                // The host's edge cue and the bar's widget context (`bar.*`, `bead.desktop.*`,
                // workspaces, windows, title, viewport) only reach views that declare them: the
                // widget live projection is offered to whichever panel is open.
                values.retain(|id, _| {
                    !(id.starts_with("bead.edge.") || is_bar_widget_context(id))
                        || self.session.stores.get(id).is_some()
                });
                if !self.dock_status.is_empty()
                    && (values.contains_key("cc.page") || values.contains_key("cc.detail"))
                {
                    self.leave_docked_flow();
                    values.extend(Self::cleared_docked_secrets());
                }

                if self.workspaces.view.follow(&values)
                    && let Some(backend) = &self.backend
                {
                    values.insert(
                        "bead.workspaces".into(),
                        bead::core::Value::list(
                            self.workspaces
                                .rows(&backend.snapshot(), self.output.as_deref()),
                        ),
                    );
                }
                let scene = if let Some(menu) = &mut self.tray_menu {
                    let snapshot = self
                        .desktop
                        .as_ref()
                        .ok_or("desktop unavailable")?
                        .snapshot();
                    self.session.update(menu.values(&snapshot))?
                } else if self.launcher.is_some() {
                    self.session.update(values)?
                } else {
                    self.session.update_model(values)?
                };
                if let Some(snapshot) = self.backend.as_ref().map(|backend| backend.snapshot()) {
                    self.try_float_widgets_window(&snapshot);
                }
                scene
            }
            UiEvent::Desktop(snapshot) => {
                // Audio and bus callbacks can arrive on different threads.
                // Never resurrect a removed item from an older publication.
                if snapshot.revision < self.desktop_revision {
                    return Ok(None);
                }
                self.desktop_revision = snapshot.revision;
                let snapshot = self.with_daemon_tray(snapshot);
                if self.notification_primary
                    && let Some(owner) = &self.notifications
                {
                    owner.audio_changed(&snapshot.audio);
                }
                let mut values = if let Some(menu) = &mut self.tray_menu {
                    menu.values(&snapshot)
                } else {
                    crate::desktop::values(&snapshot, &self.menu_item)
                };
                if std::mem::take(&mut self.tray_close) {
                    values.insert("panel.open".into(), bead::core::Value::Bool(false));
                }
                if self.tray_menu.is_some()
                    && !self.tray_requested_close
                    && values.get("panel.open") == Some(&bead::core::Value::Bool(false))
                    && let Some(wakeup) = &self.wakeup
                {
                    self.tray_requested_close = true;
                    wakeup(UiEvent::CompanionVisibility(false));
                }
                if self.bar && !self.tray_enabled && self.tray_topic.is_none() {
                    values.insert("bead.desktop.tray".into(), bead::core::Value::list(vec![]));
                }
                if self.bar && self.menu_item.is_empty() {
                    values.extend(crate::desktop::tooltip_values(&snapshot, &self.tray_hover));
                }
                if let Some(source) = &self.bar_daemon {
                    values.extend(source.values());
                }
                if self.widget_canvas {
                    // B0193: a covered canvas ignores service ticks (the host sends a full
                    // projection on uncover) and only takes seconds while it leases them.
                    if self.widgets_paused() {
                        return Ok(None);
                    }
                    if self.clock_seconds_lease.is_none() {
                        values.remove(CLOCK_SECONDS);
                    }
                }
                if let Some(id) = self.pending_focus.take() {
                    values.insert("bead.focused".into(), bead::core::Value::Text(id));
                }
                if self.notification_primary {
                    self.session.update_model(values)?
                } else {
                    self.session.update(values)?
                }
            }
            UiEvent::OutputName(name) => {
                self.output = Some(name.clone());
                let luma = crate::wallpaper_luma::values(Some(&name));
                scope_recovery(&mut self.session, Some(&name));
                let values = if let Some(backend) = &self.backend {
                    let snapshot = backend.snapshot();
                    let mut values = backend_values(&snapshot, Some(&name), &self.workspaces)?;
                    let (title, focused_window) =
                        focused_title(&snapshot, Some(&name), self.last_focused_window.as_ref());
                    self.last_focused_window = focused_window;
                    values.insert("bead.focused_title".into(), bead::core::Value::Text(title));
                    values
                } else {
                    BTreeMap::from([("bead.output.name".into(), bead::core::Value::Text(name))])
                };
                let mut values = values;
                if self.bar {
                    values.extend(luma);
                }
                let scene = self.session.update_model(values)?;
                // Publish a fresh accepted generation under the stable name's
                // recovery key after this host-context transaction commits.
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::Reload);
                }
                scene
            }
            UiEvent::Backend(snapshot) => {
                if self.widget_canvas && self.widgets_paused() {
                    return Ok(None);
                }
                let mut values =
                    backend_values(&snapshot, self.output.as_deref(), &self.workspaces)?;
                let (title, focused_window) = focused_title(
                    &snapshot,
                    self.output.as_deref(),
                    self.last_focused_window.as_ref(),
                );
                self.last_focused_window = focused_window;
                let title_only =
                    backend_title_only(&values, |id| self.session.stores.get(id).cloned());
                if !title_only {
                    bead::core::vprintln!(
                        "BACKEND_UPDATE backend={} connected={} workspaces={} windows={}",
                        snapshot.backend,
                        snapshot.connected,
                        snapshot.workspaces.len(),
                        snapshot.windows.len()
                    );
                }
                values.insert("bead.focused_title".into(), bead::core::Value::Text(title));
                // Keep the complete backend model current (including window titles); the
                // view-read filter in update_model prevents unrelated bar work.
                let scene = self.session.update_model(values)?;
                self.try_float_widgets_window(&snapshot);
                scene
            }
            UiEvent::CompanionModel(_) => {
                return Err("companion model reached primary script".into());
            }
            UiEvent::CompanionVisibility(_)
            | UiEvent::NativePearlMotion(_)
            | UiEvent::NativePearlPlaced { .. }
            | UiEvent::NativePearlPrewarmed { .. }
            | UiEvent::CompanionSelect { .. }
            | UiEvent::CompanionSelectAt { .. }
            | UiEvent::CompanionOffer { .. }
            | UiEvent::CanvasPrepare { .. }
            | UiEvent::CanvasOffer { .. }
            | UiEvent::CanvasModel { .. }
            | UiEvent::CompanionAction { .. }
            | UiEvent::CompanionMotionAction { .. }
            | UiEvent::HostModel(_)
            | UiEvent::ShellHidden(_)
            | UiEvent::LivePalette(_)
            | UiEvent::ControlChanged => {
                return Err("companion visibility reached script".into());
            }
            UiEvent::Panel(_, _)
            | UiEvent::Window(_, _)
            | UiEvent::Overlay(_, _)
            | UiEvent::Canvas(_, _)
            | UiEvent::Output(_, _, _) => {
                return Err("session envelope reached script".into());
            }
            UiEvent::PanelHover { opened, anchor } => {
                // Lifecycle input is optional. Only views declaring hover policy
                // receive these authored stores; ordinary popouts keep their schema.
                let mut values = BTreeMap::new();
                if self.session.stores.get("panel.anchor_hovered").is_some() {
                    values.insert(
                        "panel.anchor_hovered".into(),
                        bead::core::Value::Bool(anchor),
                    );
                }
                if let Some(opened) = opened
                    && self.session.stores.get("panel.hover_opened").is_some()
                {
                    values.insert("panel.hover_opened".into(), bead::core::Value::Bool(opened));
                }
                if let Some(opened) = opened {
                    if self.chat.is_some() {
                        values.insert(
                            "bead.focused".into(),
                            bead::core::Value::Text(
                                if opened { "" } else { "chat.composer" }.into(),
                            ),
                        );
                    } else if opened && self.session.stores.get("bead.focused").is_some() {
                        values.insert(
                            "bead.focused".into(),
                            bead::core::Value::Text(String::new()),
                        );
                    }
                }
                if values.is_empty() {
                    None
                } else {
                    self.session.update(values)?
                }
            }
            UiEvent::PanelVisibility(open) => {
                if self.chat.is_some() {
                    crate::app::notify_panel_visibility(open);
                }
                if open {
                    self.tray_requested_close = false;
                }
                let mut values = panel_visibility_changes(open, self.launcher.is_some());
                if !open {
                    for key in ["panel.hover_opened", "panel.anchor_hovered"] {
                        if self.session.stores.get(key).is_some() {
                            values.insert(key.into(), bead::core::Value::Bool(false));
                        }
                    }
                }
                if !open && self.chat.is_some() {
                    values.extend(
                        self.session
                            .action_values("close", bead::core::Value::Bool(true))?,
                    );
                }
                if self.widget_editor && !open {
                    values.extend(BTreeMap::from([
                        ("widgets.editing".into(), bead::core::Value::Bool(false)),
                        ("widgets.draft".into(), bead::core::Value::list(Vec::new())),
                        ("widgets.undo".into(), bead::core::Value::list(Vec::new())),
                        ("widgets.drag.active".into(), bead::core::Value::Bool(false)),
                    ]));
                }
                if !open && !self.dock_status.is_empty() {
                    self.leave_docked_flow();
                    values.extend(Self::cleared_docked_secrets());
                }
                self.session.update(values)?
            }
            UiEvent::Fatal(error) => return Err(error.into()),
            UiEvent::Worker(_) => return Err("worker packet cannot reach script executor".into()),
            UiEvent::ResourcesPrepared(_) | UiEvent::RetireResources(_) => {
                return Err("resource wake cannot reach script executor".into());
            }
            UiEvent::DebugScriptStall(ms) => {
                let clock = || crate::backends::clock_ns(nix::libc::CLOCK_MONOTONIC);
                println!("DEBUG_SCRIPT_STALL begin_ns={} duration_ms={ms}", clock());
                std::thread::sleep(std::time::Duration::from_millis(ms));
                println!("DEBUG_SCRIPT_STALL end_ns={}", clock());
                None
            }
            UiEvent::ClipboardText(text) => self.session.key(&text, false)?,
            UiEvent::ClipboardSet(_) | UiEvent::ClipboardRead => {
                return Err("clipboard transport event reached script".into());
            }
            UiEvent::Remote(request) => {
                let call = &request.call;
                let wallpaper_open_route = call.target == "nacre"
                    && call.method == "launcher"
                    && call
                        .args
                        .first()
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|query| query.starts_with(">wallpaper"));
                if wallpaper_open_route {
                    eprintln!(
                        "WALLPAPER_OPEN_REMOTE_RECEIVED monotonic_ns={}",
                        bead::core::profile::monotonic_ns()
                    );
                }
                let private_widget_feedback = self.widget_app
                    && call.target == "bead.widgets"
                    && call.method == "drag-feedback";
                let mut feedback_scene = None;
                let validation = if private_widget_feedback || call.target == "bead.osd" {
                    call.validate()
                        .map_err(|e| bead::services::ipc::Reply::error(call.id, "invalid-call", e))
                } else {
                    crate::commands::validate(call)
                };
                let reply = match validation {
                    Err(reply) => reply,
                    Ok(()) => match (call.target.as_str(), call.method.as_str()) {
                        ("bead.widgets", "drag-feedback") if self.widget_app => {
                            let payload = call.args.first();
                            let valid = payload.is_some_and(|payload| {
                                payload["app"] == "nacre-widgets"
                                    && payload["pid"].as_u64() == Some(std::process::id() as u64)
                                    && payload["generation"].as_u64().is_some()
                                    && payload["success"].as_bool().is_some()
                                    && payload["error"].as_str().is_some()
                            });
                            if !valid {
                                bead::services::ipc::Reply::error(
                                    call.id,
                                    "invalid-feedback",
                                    "invalid Widgets feedback payload",
                                )
                            } else {
                                let payload = payload.expect("validated");
                                let history = payload["kind"] == "history";
                                let epoch = payload["epoch"].as_u64();
                                let pid = payload["pid"]
                                    .as_u64()
                                    .and_then(|pid| u32::try_from(pid).ok())
                                    .unwrap_or_default();
                                let generation = payload["generation"].as_u64().unwrap_or_default();
                                let identity_matches = pid == std::process::id() && generation > 0;
                                let drag_matches = epoch.is_some_and(|epoch| {
                                    widget_drag_feedback_matches(
                                        std::process::id(),
                                        self.widget_drag_pending_epoch,
                                        pid,
                                        epoch,
                                    )
                                });
                                if history {
                                    if !identity_matches || (epoch.is_some() && !drag_matches) {
                                        bead::services::ipc::Reply::ok(
                                            call.id,
                                            serde_json::Value::Null,
                                        )
                                    } else {
                                        let previous = payload["previous_document"]
                                            .as_object()
                                            .map(|_| payload["previous_document"].clone());
                                        let document = payload["document"]
                                            .as_object()
                                            .map(|_| payload["document"].clone());
                                        let previous_revision =
                                            payload["previous_revision"].as_u64();
                                        let revision = payload["revision"].as_u64();
                                        let history_valid = payload["success"] == true
                                            && previous.is_some()
                                            && document.is_some()
                                            && previous_revision.is_some()
                                            && revision.is_some();
                                        if !history_valid {
                                            bead::services::ipc::Reply::error(
                                                call.id,
                                                "invalid-feedback",
                                                "invalid widget history feedback",
                                            )
                                        } else {
                                            let previous = previous.expect("validated");
                                            let document = document.expect("validated");
                                            let previous_revision =
                                                previous_revision.expect("validated");
                                            let revision = revision.expect("validated");
                                            let current_revision = self
                                                .session
                                                .stores
                                                .get("widgets.rev")
                                                .and_then(|value| {
                                                    if let bead::core::Value::Number(value) = value
                                                    {
                                                        Some(*value as u64)
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .unwrap_or(0);
                                            let last_history_revision = self
                                                .session
                                                .stores
                                                .get("widgets.history.revision")
                                                .and_then(|value| {
                                                    if let bead::core::Value::Number(value) = value
                                                    {
                                                        Some(*value as u64)
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .unwrap_or(0);
                                            let layout =
                                                match self.session.stores.get("widgets.layout") {
                                                    Some(bead::core::Value::List(records)) => {
                                                        records.first().cloned()
                                                    }
                                                    _ => None,
                                                };
                                            let initial =
                                                match self.session.stores.get("widgets.initial") {
                                                    Some(bead::core::Value::List(records)) => {
                                                        records.first().cloned()
                                                    }
                                                    _ => None,
                                                };
                                            let seed = self.widget_seed();
                                            let current_document = widget_history_current_document(
                                                layout,
                                                initial,
                                                seed.as_ref(),
                                                current_revision,
                                                previous_revision,
                                            );
                                            let current_draft_empty = matches!(self.session.stores.get("widgets.draft"),
                                            Some(bead::core::Value::List(records)) if records.is_empty());
                                            let native_busy = epoch.is_none()
                                                && self.session.stores.get("widgets.busy")
                                                    == Some(&bead::core::Value::Bool(true));
                                            let accepted = !native_busy
                                                && current_draft_empty
                                                && widget_history_feedback_matches(
                                                    current_revision,
                                                    last_history_revision,
                                                    previous_revision,
                                                    revision,
                                                    current_document.as_ref(),
                                                    &previous,
                                                    &document,
                                                );
                                            if epoch.is_some() {
                                                self.widget_drag_pending_epoch = None;
                                            }
                                            if accepted {
                                                let changes = self.session.action_values(
                                                    "widgets.history.parent",
                                                    bead::core::Value::list(vec![payload.clone()]),
                                                )?;
                                                feedback_scene = self.session.update(changes)?;
                                            } else if epoch.is_some()
                                                && current_draft_empty
                                                && current_revision <= revision
                                            {
                                                feedback_scene =
                                                    self.session.update(BTreeMap::from([(
                                                        "widgets.busy".into(),
                                                        bead::core::Value::Bool(false),
                                                    )]))?;
                                            }
                                            bead::services::ipc::Reply::ok(
                                                call.id,
                                                serde_json::Value::Null,
                                            )
                                        }
                                    }
                                } else if !identity_matches || !drag_matches {
                                    bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                                } else {
                                    self.widget_drag_pending_epoch = None;
                                    let mut changes = BTreeMap::from([(
                                        "widgets.busy".into(),
                                        bead::core::Value::Bool(false),
                                    )]);
                                    if epoch.is_some() && self.widget_drag_latest_epoch == epoch {
                                        let success = payload["success"].as_bool().unwrap_or(false);
                                        let error = if success {
                                            String::new()
                                        } else {
                                            payload["error"]
                                                .as_str()
                                                .unwrap_or("Widget layout could not be saved.")
                                                .chars()
                                                .take(512)
                                                .collect()
                                        };
                                        changes.insert(
                                            "widgets.error".into(),
                                            bead::core::Value::Text(error),
                                        );
                                        changes.insert(
                                            "widgets.message".into(),
                                            bead::core::Value::Text(if success {
                                                "Changes saved".into()
                                            } else {
                                                String::new()
                                            }),
                                        );
                                    }
                                    feedback_scene = self.session.update(changes)?;
                                    bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                                }
                            }
                        }
                        ("bead.osd", "show" | "preview") => match self
                            .actions
                            .authorize("notifications.feedback")
                            .map_err(|_| "notification feedback capability denied")
                            .and_then(|_| {
                                let owner = self
                                    .notifications
                                    .as_ref()
                                    .or(self.notice_host.as_ref())
                                    .ok_or("notification server unavailable");
                                owner.and_then(|owner| {
                                    if call.method == "preview" {
                                        owner.preview(&call.args)
                                    } else {
                                        owner.feedback(&call.args)
                                    }
                                })
                            }) {
                            Ok(value) => bead::services::ipc::Reply::ok(call.id, value),
                            Err(e) => bead::services::ipc::Reply::error(call.id, "osd", e),
                        },
                        ("notifs", _) => match self
                            .notifications
                            .as_ref()
                            .or(self.notice_host.as_ref())
                            .ok_or("notification server unavailable")
                            .and_then(|owner| owner.command(call))
                        {
                            Ok(value) => bead::services::ipc::Reply::ok(call.id, value),
                            Err(e) => {
                                bead::services::ipc::Reply::error(call.id, "notifications", e)
                            }
                        },
                        ("nacre", "controls")
                            if call
                                .args
                                .first()
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|page| {
                                    matches!(
                                        page,
                                        "home"
                                            | "edit"
                                            | "network"
                                            | "bluetooth"
                                            | "battery"
                                            | "audio"
                                            | "sound"
                                    )
                                }) =>
                        {
                            self.actions.authorize("view.open")?;
                            let page = call.args[0].as_str().unwrap_or("home");
                            let detail = !matches!(page, "home" | "edit");
                            let page = if page == "sound" { "audio" } else { page };
                            let wakeup = self
                                .wakeup
                                .as_ref()
                                .ok_or("control event channel unavailable")?;
                            wakeup(UiEvent::CompanionSelect {
                                view: "control".into(),
                                output: call.output.clone(),
                                toggle: false,
                                center: None,
                            });
                            wakeup(UiEvent::CompanionModel(BTreeMap::from([
                                (
                                    "cc.page".into(),
                                    bead::core::Value::Text(
                                        if detail { "detail" } else { page }.into(),
                                    ),
                                ),
                                (
                                    "cc.detail".into(),
                                    bead::core::Value::Text(if detail { page } else { "" }.into()),
                                ),
                                ("cc.scroll".into(), bead::core::Value::Number(0.)),
                                (
                                    "bead.focused".into(),
                                    bead::core::Value::Text(String::new()),
                                ),
                            ])));
                            bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                        }
                        ("drawers", "toggle")
                            if call.args.first().and_then(serde_json::Value::as_str)
                                == Some("dashboard")
                                && self.session.stores.get("bar.popouts")
                                    == Some(&bead::core::Value::Bool(true)) =>
                        {
                            self.actions.authorize("view.open")?;
                            let wakeup = self
                                .wakeup
                                .as_ref()
                                .ok_or("dashboard event channel unavailable")?;
                            if request.context.panel_open
                                && request
                                    .context
                                    .panel_view
                                    .as_deref()
                                    .and_then(crate::dashboard::Page::from_view)
                                    .is_some()
                            {
                                wakeup(UiEvent::CompanionVisibility(false));
                            } else {
                                wakeup(UiEvent::ShellHidden(Some(false)));
                                wakeup(UiEvent::CompanionSelect {
                                    view: "dashboard-today".into(),
                                    output: call.output.clone(),
                                    toggle: false,
                                    center: None,
                                });
                            }
                            bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                        }
                        ("nacre", "launcher" | "menu" | "clipboard" | "mode")
                            if self.launcher_trigger =>
                        {
                            match crate::key_routes::launcher(call) {
                                Some(Ok(launch)) => {
                                    self.actions.authorize("launcher.open")?;
                                    let wakeup = self
                                        .wakeup
                                        .as_ref()
                                        .ok_or("launcher event channel unavailable")?;
                                    let showing = request.context.panel_open
                                        && request
                                            .context
                                            .panel_view
                                            .as_deref()
                                            .unwrap_or("launcher")
                                            == launch.target_view.as_deref().unwrap_or("launcher");
                                    let wallpaper_open =
                                        launch.target_view.as_deref() == Some("wallpaper");
                                    if launch.toggle
                                        && showing
                                        && (launch.target_view.is_some()
                                            || self.last_launch.as_ref() == Some(&launch))
                                    {
                                        self.last_launch = None;
                                        wakeup(UiEvent::CompanionVisibility(false));
                                    } else {
                                        self.last_launch = Some(launch);
                                        wakeup(UiEvent::ShellHidden(Some(false)));
                                        if let Some(view) = self
                                            .last_launch
                                            .as_ref()
                                            .and_then(|l| l.target_view.clone())
                                        {
                                            wakeup(UiEvent::CompanionSelect {
                                                view,
                                                output: call.output.clone(),
                                                toggle: false,
                                                center: None,
                                            });
                                        } else {
                                            let values = self
                                                .last_launch
                                                .as_ref()
                                                .expect("launch stored")
                                                .model();
                                            *self
                                                .launcher_memory
                                                .as_ref()
                                                .ok_or("launcher view state unavailable")?
                                                .lock()
                                                .map_err(|_| "launcher view state unavailable")? =
                                                values.clone();
                                            wakeup(UiEvent::CompanionVisibility(true));
                                            wakeup(UiEvent::CompanionModel(values));
                                        }
                                    }
                                    if wallpaper_open {
                                        eprintln!(
                                            "WALLPAPER_OPEN_REMOTE_ENQUEUED monotonic_ns={}",
                                            bead::core::profile::monotonic_ns()
                                        );
                                    }
                                    bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                                }
                                Some(Err(message)) => bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    message,
                                ),
                                None => bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    "compatibility route belongs to a later native feature",
                                ),
                            }
                        }
                        ("brightness", "step") if self.brightness.is_some() => {
                            self.actions.authorize("key.brightness")?;
                            let output = self.focused_output();
                            let direction = call.args[0].as_str().unwrap_or_default();
                            match self
                                .brightness
                                .as_ref()
                                .ok_or("brightness keys unavailable")?
                                .step(direction, output.as_deref())
                            {
                                Ok(()) => {
                                    bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                                }
                                Err(message) => bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    message,
                                ),
                            }
                        }
                        ("brightness", "set") if self.brightness.is_some() => {
                            self.actions.authorize("key.brightness")?;
                            let output = self.focused_output();
                            match self
                                .brightness
                                .as_ref()
                                .ok_or("brightness keys unavailable")?
                                .set(call.args[0].as_f64().unwrap_or(f64::NAN), output.as_deref())
                            {
                                Ok(()) => bead::services::ipc::Reply::ok(
                                    call.id,
                                    serde_json::Value::String("ok".into()),
                                ),
                                Err(message) => bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    message,
                                ),
                            }
                        }
                        ("keyboardLight", "step") if self.brightness.is_some() => {
                            self.actions.authorize("key.brightness")?;
                            match self
                                .brightness
                                .as_ref()
                                .ok_or("keyboard light keys unavailable")?
                                .keyboard_step(call.args[0].as_str().unwrap_or_default())
                            {
                                Ok(()) => {
                                    bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                                }
                                Err(message) => bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    message,
                                ),
                            }
                        }
                        ("nacre", "state") => {
                            let snapshot = self.backend.as_ref().map(|b| b.snapshot());
                            let view = self
                                .launcher_memory
                                .as_ref()
                                .and_then(|m| m.lock().ok().map(|m| m.clone()))
                                .unwrap_or_default();
                            let view_text = |key: &str| match view.get(key) {
                                Some(bead::core::Value::Text(s)) => s.as_str(),
                                _ => "",
                            };
                            let active_view = request.context.panel_view.as_deref().unwrap_or(
                                if self.launcher_trigger {
                                    "launcher"
                                } else {
                                    "sound"
                                },
                            );
                            let dashboard_page = crate::dashboard::Page::from_view(active_view);
                            let wallpaper_picker_open =
                                request.context.panel_open && active_view == "wallpaper";
                            let launcher_open = request.context.panel_open
                                && active_view == "launcher"
                                || wallpaper_picker_open;
                            let edge_menu = if request.context.panel_open
                                && active_view != "launcher"
                                && dashboard_page.is_none()
                            {
                                active_view
                            } else {
                                ""
                            };
                            let core_views = self.session.stores.get("bar.popouts")
                                == Some(&bead::core::Value::Bool(true));
                            let state = serde_json::json!({"active":snapshot.as_ref().and_then(|s|s.workspaces.iter().find(|w|w.focused||w.active)).map(|w|w.index).unwrap_or(0),"workspaces":snapshot.as_ref().map(|s|s.workspaces.len()).unwrap_or(0),"dashboard":request.context.panel_open && dashboard_page.is_some(),"edgeMenu":edge_menu,"clickEdgeMenus":false,"tab":dashboard_page.map(|page| page.tab()).unwrap_or(0),"launcher":launcher_open,"session":false,"osd":false,"controlSection":"home","feedback":false,"hidden":request.context.hidden,"left":false,"leftPinned":false,"query":view_text("launcher.search.text"),"launcherMode":if wallpaper_picker_open {"wallpaper"}else if self.launcher_trigger {view_text("launcher.mode")}else{"apps"},"launcherRoute":if wallpaper_picker_open {"picker"}else if self.launcher_trigger {view_text("launcher.route")}else{"home"},"reveal":false,"notificationsSuppressed":false,"bead":{"protocol":1,"available":if request.context.panel_configured {if core_views {vec!["bar","launcher","sound-output","calendar","network-status","bluetooth-status","battery-status","dashboard"]}else if self.launcher_trigger {vec!["bar","launcher"]}else{vec!["bar","sound-output"]}}else{vec!["bar"]},"panelOpen":request.context.panel_open}});
                            #[cfg(all(feature = "trace", target_os = "linux", target_env = "gnu"))]
                            let state = {
                                let mut state = state;
                                // SAFETY: read-only by-value allocator accounting.
                                let heap = unsafe { nix::libc::mallinfo2() };
                                state["bead"]["heap"] = serde_json::json!({"live":heap.uordblks,"free":heap.fordblks,"mapped":heap.hblkhd});
                                state
                            };
                            bead::services::ipc::Reply::ok(
                                call.id,
                                serde_json::Value::String(state.to_string()),
                            )
                        }
                        ("compositorState", "state") => {
                            if let Some(backend) = &self.backend {
                                bead::services::ipc::Reply::ok(
                                    call.id,
                                    serde_json::Value::String(serde_json::to_string(
                                        &*backend.snapshot(),
                                    )?),
                                )
                            } else {
                                bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    "compositor not configured",
                                )
                            }
                        }
                        (_, "state") if call.target.starts_with("windowTitle-") => {
                            let title = match self.session.stores.get("bead.focused_title") {
                                Some(bead::core::Value::Text(title)) => title.as_str(),
                                _ => "",
                            };
                            let retained = self.backend.as_ref().is_some_and(|backend| {
                                self.last_focused_window.is_some()
                                    && focused_title(
                                        &backend.snapshot(),
                                        self.output.as_deref(),
                                        None,
                                    )
                                    .1
                                    .is_none()
                            });
                            bead::services::ipc::Reply::ok(
                                call.id,
                                serde_json::Value::String(serde_json::to_string(
                                    &serde_json::json!({"title":title,"retained":retained}),
                                )?),
                            )
                        }
                        _ => bead::services::ipc::Reply::error(
                            call.id,
                            "unavailable",
                            "compatibility route belongs to a later native feature",
                        ),
                    },
                };
                let _ = request.reply.send(reply);
                feedback_scene
            }
            UiEvent::Timer { token } => {
                if token == self.widget_drag_epoch {
                    if self.widget_app {
                        let epoch = match self.session.stores.get("widgets.drag.epoch") {
                            Some(bead::core::Value::Number(epoch)) => *epoch,
                            _ => 0.,
                        };
                        let changes = self.session.action_values(
                            "widgets.drag",
                            bead::core::Value::list(vec![
                                serde_json::json!({"phase":"retire","epoch":epoch}),
                            ]),
                        )?;
                        self.session.update(changes)?
                    } else if let Some(controller) = self.widget_drag_controller.as_mut() {
                        let epoch = controller.drag_epoch();
                        let values = controller.drag(bead::core::Value::list(vec![
                            serde_json::json!({"phase":"retire","epoch":epoch}),
                        ]))?;
                        self.widget_drag_model = Some(values.clone());
                        self.widget_drag_retire_cleanup = true;
                        let staged = self.stage_widget_drag_state(&values);
                        let scene = self
                            .session
                            .update(staged)
                            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;
                        // Retire the drag canvas as soon as the settle timer fires,
                        // even when this update also produced a scene. In that case
                        // `finish_scene` may be deferred; leaving the old offer open
                        // retains the ghost's renderer buffers until another update.
                        let projection = self.widget_drag_projection(&values);
                        self.publish_widget_drag_projection(projection);
                        scene
                    } else {
                        None
                    }
                } else if token == self.status_pill_epoch {
                    let before = self.status_pill.clone();
                    self.status_pill.advance(std::time::Instant::now());
                    self.schedule_status_pill();
                    if before.peek() != self.status_pill.peek()
                        || before.panel() != self.status_pill.panel()
                    {
                        self.sync_status_peek_route()?;
                    }
                    self.publish_status_pill()?
                } else {
                    None
                }
            }
            // Notice and OSD sessions have no status-pill stores; their hover is the
            // card/OSD hover key above.
            UiEvent::PointerRegion(_) if self.notifications.is_some() => None,
            UiEvent::PointerRegion(entered)
                if self.session.stores.get("panel.hover_opened").is_some() =>
            {
                self.session.pointer_region(entered)?
            }
            UiEvent::PointerRegion(entered) => {
                let before = self.status_pill.clone();
                if entered {
                    self.status_pill.enter_region();
                } else {
                    self.status_pill.leave_region(std::time::Instant::now());
                }
                self.schedule_status_pill();
                if before.peek() != self.status_pill.peek() {
                    self.sync_status_peek_route()?;
                }
                self.publish_status_pill()?
            }
            UiEvent::PanelDismissed => {
                if let Some(host) = &self.notice_host {
                    host.panel_changed(None);
                }
                self.last_launch = None;
                if self.status_pill_route.is_some() {
                    if let Some(owner) = self.status_pill.panel().map(String::from) {
                        self.status_owner_dismissed = Some((owner, std::time::Instant::now()));
                    }
                    self.status_pill.escape();
                    self.status_pill_route = None;
                    self.schedule_status_pill();
                    self.publish_status_pill()?
                } else {
                    None
                }
            }
            UiEvent::PanelNavigated(_) => None,
            UiEvent::PanelSelected(view) => {
                if let Some(host) = &self.notice_host {
                    host.panel_changed(Some(&view));
                }
                if self
                    .status_pill_route
                    .as_deref()
                    .is_some_and(|route| route != view)
                {
                    self.status_pill.escape();
                    self.status_pill_route = None;
                    self.schedule_status_pill();
                    self.publish_status_pill()?
                } else {
                    None
                }
            }
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Restore(saved) => {
                let values = crate::pearls::restore_state(saved, &self.session.stores);
                self.session.update(values)?
            }
            // The Colors view sizes its pieces from the window width: project again.
            UiEvent::Relayout if self.colors.is_some() => self.session.update(BTreeMap::new())?,
            // The Welcome layout reads the window size: a resize projects it again.
            UiEvent::Relayout if self.welcome.is_some() => self.session.update(BTreeMap::new())?,
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::PanelDetached(detached) => {
                if detached != self.detached && self.spare_chrome.is_some() {
                    std::mem::swap(&mut self.chrome, &mut self.spare_chrome);
                    self.detached = detached;
                    if detached && let Some(chrome) = &mut self.chrome {
                        chrome.set_engaged(false);
                    }
                }
                if self.session.stores.get("view.detached").is_some() {
                    self.session.update(BTreeMap::from([(
                        "view.detached".into(),
                        bead::core::Value::Bool(detached),
                    )]))?
                } else {
                    self.session.relayout()?
                }
            }
            UiEvent::PearlGuide(_) => return Err("guide event reached script".into()),
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::Hover { x, y } => {
                self.prewarm_widget_drag_hover(x, y, true);
                let before = self.status_pill.clone();
                let pill_item = self
                    .session
                    .action_at(x, y)
                    .and_then(|(id, name, argument)| {
                        if name == "status.pill.open" {
                            let hover = crate::widgets::dropdown_hover(&argument);
                            crate::widgets::dropdown_view(&argument)?;
                            if hover || self.status_pill.panel().is_some() {
                                Some(id)
                            } else {
                                None
                            }
                        } else if name == "view.open" {
                            (id.rsplit('.').next() == Some("clock")).then_some(id)
                        } else {
                            None
                        }
                    });
                let hovered = self
                    .session
                    .with_local_feedback(|session| session.hover(x, y))?;
                let pill = if self.bar || self.widget_canvas {
                    if let Some(item) = pill_item {
                        self.status_pill.enter_icon(item, std::time::Instant::now());
                    } else {
                        self.status_pill.leave_region(std::time::Instant::now());
                    }
                    self.schedule_status_pill();
                    self.publish_status_pill()?
                } else {
                    None
                };
                if before.peek() != self.status_pill.peek() {
                    self.sync_status_peek_route()?;
                }
                if self.bar && self.menu_item.is_empty() {
                    self.tray_hover = self
                        .session
                        .action_at(x, y)
                        .and_then(|(_, name, arg)| {
                            if name == "services.tray" {
                                if let bead::core::Value::Text(id) = arg {
                                    Some(id)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    let snapshot = self.with_daemon_tray(
                        self.desktop
                            .as_ref()
                            .ok_or("desktop unavailable")?
                            .snapshot(),
                    );
                    self.session
                        .update(crate::desktop::tooltip_values(&snapshot, &self.tray_hover))?
                        .or(hovered)
                        .or(pill)
                } else {
                    hovered.or(pill)
                }
            }
            UiEvent::Drag { x, y } | UiEvent::LocalDrag { x, y } => self.session.drag(x, y)?,
            UiEvent::Release if self.host_release_pending => {
                self.host_release_pending = false;
                self.session.release_without_activation()?
            }
            UiEvent::Release => self.session.release()?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "clipboard-copy" | "clipboard-cut" => {
                    if let Some(text) = self.session.selected_text() {
                        self.wakeup
                            .as_ref()
                            .ok_or("clipboard event channel unavailable")?(
                            UiEvent::ClipboardSet(text),
                        );
                    }
                    if command == "clipboard-cut" {
                        self.session.key("", false)?
                    } else {
                        None
                    }
                }
                "clipboard-paste" => {
                    self.wakeup
                        .as_ref()
                        .ok_or("clipboard event channel unavailable")?(
                        UiEvent::ClipboardRead
                    );
                    None
                }
                "escape"
                    if self
                        .session_menu
                        .as_ref()
                        .is_some_and(|menu| menu.confirming().is_some()) =>
                {
                    let key = self.session_menu.as_ref().and_then(|menu| {
                        let key = menu.confirming();
                        menu.cancel();
                        key
                    });
                    // Hand focus back to the item that was asking.
                    self.session.update(key.map_or(BTreeMap::new(), |key| {
                        BTreeMap::from([(
                            "bead.focused".into(),
                            bead::core::Value::Text(format!("session-popout.act-{key}.main")),
                        )])
                    }))?
                }
                "escape"
                    if self.active_status().is_some()
                        && matches!(
                            self.session.stores.get("network.view"),
                            Some(bead::core::Value::Text(view)) if view != "list"
                        ) =>
                {
                    // Esc leaves the prompt, hidden form or saved list before it closes the panel.
                    if let Some(status) = self.active_status() {
                        status.network_cancel();
                    }
                    self.session.update(BTreeMap::from([
                        (
                            crate::status_views::SECRET_STORE.to_owned(),
                            bead::core::Value::Text(String::new()),
                        ),
                        (
                            "network.hidden_ssid".into(),
                            bead::core::Value::Text(String::new()),
                        ),
                        ("network.reveal".into(), bead::core::Value::Bool(false)),
                    ]))?
                }
                "escape"
                    if self
                        .active_status()
                        .as_ref()
                        .is_some_and(|status| status.bluetooth_pairing_open()) =>
                {
                    // Esc cancels the pairing page before it closes the panel.
                    if let Some(status) = self.active_status() {
                        status.bluetooth_pair_cancel();
                    }
                    self.session.update(BTreeMap::from([
                        (
                            crate::status_views::BLUETOOTH_SECRET_STORE.to_owned(),
                            bead::core::Value::Text(String::new()),
                        ),
                        ("bluetooth.reveal".into(), bead::core::Value::Bool(false)),
                    ]))?
                }
                "escape"
                    if !self.dock_status.is_empty()
                        && self.session.stores.get("cc.page")
                            != Some(&bead::core::Value::Text("home".into())) =>
                {
                    self.session.update(BTreeMap::from([
                        ("cc.page".into(), bead::core::Value::Text("home".into())),
                        ("cc.scroll".into(), bead::core::Value::Number(0.)),
                        (
                            "network.secret".into(),
                            bead::core::Value::Text(String::new()),
                        ),
                        (
                            "bluetooth.secret".into(),
                            bead::core::Value::Text(String::new()),
                        ),
                    ]))?
                }
                // Esc closes an open Settings drop-down before anything else.
                "escape"
                    if matches!(
                        self.session.stores.get("settings.select"),
                        Some(bead::core::Value::Text(open)) if !open.is_empty()
                    ) =>
                {
                    self.session.update(BTreeMap::from([(
                        "settings.select".into(),
                        bead::core::Value::Text(String::new()),
                    )]))?
                }
                "escape" if self.prompt.is_some() => {
                    let changes = self
                        .prompt
                        .as_ref()
                        .map(|prompt| prompt.act("cancel", &self.session.stores))
                        .unwrap_or_default();
                    self.session.update(changes)?
                }
                "escape" if self.session.stores.get("panel.open").is_some() => {
                    // A half-typed password never outlives its panel.
                    let mut values = std::collections::BTreeMap::from([(
                        "panel.open".into(),
                        bead::core::Value::Bool(false),
                    )]);
                    if self.chat.is_some() {
                        values.extend(
                            self.session
                                .action_values("close", bead::core::Value::Bool(true))?,
                        );
                    }
                    if self
                        .session
                        .stores
                        .get(crate::status_views::BLUETOOTH_SECRET_STORE)
                        .is_some()
                    {
                        if let Some(status) = self.active_status() {
                            status.bluetooth_pair_leave();
                        }
                        values.insert(
                            crate::status_views::BLUETOOTH_SECRET_STORE.into(),
                            bead::core::Value::Text(String::new()),
                        );
                    }
                    if self
                        .session
                        .stores
                        .get(crate::status_views::SECRET_STORE)
                        .is_some()
                    {
                        if let Some(status) = self.active_status() {
                            status.network_cancel();
                        }
                        values.insert(
                            crate::status_views::SECRET_STORE.into(),
                            bead::core::Value::Text(String::new()),
                        );
                    }
                    self.session.update(values)?
                }
                "escape" if self.welcome.is_some() => {
                    self.wakeup
                        .as_ref()
                        .ok_or("window close event channel unavailable")?(
                        UiEvent::CompanionVisibility(false),
                    );
                    None
                }
                "escape" if self.bar => {
                    self.status_pill.escape();
                    self.sync_status_peek_route()?;
                    self.schedule_status_pill();
                    self.publish_status_pill()?
                }
                "tab" => self.session.focus_next(shift)?,
                "enter" if shift && self.session.focused_multiline() => {
                    self.session.input_newline()?
                }
                "enter" => self.session.activate_focused()?,
                "up" | "down" if self.session.focused_multiline() => {
                    self.session.input_vertical(command == "up", shift)?
                }
                "up" | "down" => self.session.focus_next(command == "up")?,
                "left" | "right" if self.session.focused_range() => {
                    self.session.step_range(command == "left")?
                }
                "left" | "right" if self.session.surrounding_text().is_none() => {
                    self.session.focus_next(command == "left")?
                }
                key => {
                    let key = match key {
                        "left" => bead::ui::input::EditKey::Left,
                        "right" => bead::ui::input::EditKey::Right,
                        "home" => bead::ui::input::EditKey::Home,
                        "end" => bead::ui::input::EditKey::End,
                        "delete" => bead::ui::input::EditKey::Delete,
                        "select-all" => bead::ui::input::EditKey::SelectAll,
                        _ => return Err("unsupported input command".into()),
                    };
                    self.session.edit_key(key, shift)?
                }
            },
            UiEvent::WatchError(error) => return Err(error.into()),
            UiEvent::ContextMenu { .. }
            | UiEvent::SecondaryClick { .. }
            | UiEvent::WallpaperFrame(_)
            | UiEvent::WallpaperRefresh
            | UiEvent::WallpaperFailed(_)
            | UiEvent::WallpaperTick(_)
            | UiEvent::HorizontalScroll { .. } => None,
            UiEvent::Click { x, y } => {
                if self.widget_drag_suppress_hover_until_leave
                    && self.session.transfer_candidate_at(x, y).is_some()
                {
                    self.widget_drag_suppress_hover_until_leave = false;
                    self.widget_drag_suppress_hover_bounds = None;
                    self.prewarm_widget_drag_hover(x, y, true);
                }
                self.session.click(x, y)?
            }
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Action { name, argument } | UiEvent::MotionAction { name, argument } => {
                let changes = self.session.action_values(&name, argument)?;
                self.session.update(changes)?
            }
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
        };
        // Lua actions can request an edge call (including pointer navigation buttons).
        // Drain that request after the action commits, just as keyboard navigation does.
        if let Some(extras) = &self.extras
            && let Some(bead::core::Value::Text(request)) =
                self.session.stores.get("extras.request")
            && !request.is_empty()
        {
            let request = request.clone();
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("extras event channel unavailable")?;
            extras.act(
                &request,
                &self.session.stores,
                std::sync::Arc::new(move || wakeup(UiEvent::Model(BTreeMap::new()))),
            );
            scene = self
                .session
                .update(BTreeMap::from([(
                    "extras.request".into(),
                    bead::core::Value::Text(String::new()),
                )]))?
                .or(scene);
        }
        let scene = self.finish_scene(scene)?;
        if self.widget_drag_retire_cleanup && scene.is_none() {
            self.widget_drag_controller = None;
            self.widget_drag_controller_primed = false;
            self.widget_drag_model = None;
            self.widget_drag_retire_cleanup = false;
            self.widget_drag_wanted = false;
            self.retire_widget_hover_preview();
        }
        Ok(scene)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn resource_counts(&self) -> String {
        let mut counts = self.session.resource_counts();
        if let Some(extras) = &self.extras {
            counts.push_str(&format!(" extras_images_bytes={}", extras.image_bytes()));
        }
        if bead::core::profile::enabled() && self.launcher.is_some() {
            use bead::core::Value;
            let stores = &self.session.stores;
            let pending = stores.get("launcher.pending") == Some(&Value::Bool(true));
            let home = matches!(stores.get("launcher.engine_route"), Some(Value::Text(route)) if route.split('.').next() == Some("home"))
                && matches!(stores.get("launcher.search.text"), Some(Value::Text(query)) if query.trim().is_empty());
            let number = |name| match stores.get(name) {
                Some(Value::Number(n)) => *n as usize,
                _ => 0,
            };
            let favorites = match stores.get("launcher.apps") {
                Some(Value::List(rows)) => {
                    rows.iter().filter(|row| row["favorite"] == true).count()
                }
                _ => 0,
            };
            counts.push_str(&format!(" menu_pending={pending} menu_home={home} menu_count={} menu_total={} menu_favorites={favorites}", number("launcher.result_count"), number("launcher.total_count")));
        }
        if std::env::var_os("BEAD_SERVICE_DIAGNOSTICS").is_some() {
            if let Some(desktop) = &self.desktop {
                let (tasks, retained) = desktop.bus_task_counts();
                counts.push_str(&format!(
                    " desktop_subscriptions={} desktop_tasks={} desktop_retained_tasks={}",
                    desktop.subscription_count(),
                    tasks,
                    retained
                ));
            }
            #[cfg(all(feature = "trace", target_os = "linux", target_env = "gnu"))]
            {
                // SAFETY: glibc returns a by-value read-only snapshot of its allocator accounting.
                let heap = unsafe { nix::libc::mallinfo2() };
                counts.push_str(&format!(
                    " heap_live={} heap_free={} heap_mapped={}",
                    heap.uordblks, heap.fordblks, heap.hblkhd
                ));
            }
        }
        counts
    }
    fn state_summary(&self) -> String {
        if !self.trace && std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_none() {
            return String::new();
        }
        let summary = self.session.state_summary();
        if self.widget_app && std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() {
            let viewport = self.session.view_nodes().iter().find_map(|node| {
                let Some(bead::core::Value::List(records)) = &node.metadata else {
                    return None;
                };
                let marker = records
                    .iter()
                    .find(|record| record["widget_layout"] == true)?;
                Some((
                    serde_json::json!([marker["viewport_width"], marker["viewport_height"]]),
                    marker["catalogue"].clone(),
                ))
            });
            if let (Some((viewport, catalogue)), Ok(serde_json::Value::Object(mut values))) = (
                viewport,
                serde_json::from_str::<serde_json::Value>(&summary),
            ) {
                // Proof of the accepted layout, never a store or requested viewport.
                values.insert("__bead_committed_viewport".into(), viewport);
                values.insert("__bead_widget_catalogue".into(), catalogue);
                return serde_json::to_string(&values).unwrap_or(summary);
            }
        }
        summary
    }
    fn scroll_regions(&self) -> Vec<bead::render::Rect> {
        if self.extras.as_ref().is_some_and(|extras| {
            extras.kind == crate::extras::Kind::Wallpaper
                && self.session.stores.get("extras.wallpaper.layout")
                    != Some(&bead::core::Value::Text("hexagons".into()))
        }) {
            return Vec::new();
        }
        self.session.scroll_regions()
    }
    fn wheel_step(&self) -> f64 {
        match self.session.stores.get("bead.scroll.wheel_step") {
            Some(bead::core::Value::Number(step)) if step.is_finite() && *step > 0. => *step,
            _ => 100.,
        }
    }
    fn outside_dismiss(&self) -> bool {
        self.session.stores.get("chat.pinned") != Some(&bead::core::Value::Bool(true))
    }
    fn saved_state(&self) -> serde_json::Value {
        crate::pearls::capture_state(&self.session.stores)
    }
    fn focused_dismiss(&self) -> bool {
        self.session.stores.get("panel.open").is_some()
            && self
                .session
                .focused_action()
                .is_some_and(|(_, name, _)| name == "close")
    }
    fn presented(&mut self, revision: u64) {
        let now = std::time::Instant::now();
        if let Some(duration) = self.pending_panel_animation_duration.take() {
            self.panel_animation_hold_until = Some(now + duration + self.smooth_frame_interval);
        }
        let name = self.smooth_surface();
        if let Some(started) = self.smooth_open_started.take() {
            eprintln!(
                "PANEL_OPEN_LATENCY name={} ms_to_first_content_frame={:.3} monotonic_ns={}",
                name,
                now.duration_since(started).as_secs_f64() * 1000.,
                bead::core::profile::monotonic_ns()
            );
        }
        while let Some((started, surface)) = self.smooth_key_starts.pop_front() {
            eprintln!(
                "INPUT_ECHO_LATENCY surface={surface} ms={:.3}",
                now.duration_since(started).as_secs_f64() * 1000.
            );
        }
        let missed = self
            .smooth_last_presented
            .map(|last| {
                let frames = now.duration_since(last).as_nanos()
                    / self.smooth_frame_interval.as_nanos().max(1);
                frames.saturating_sub(1) as u64
            })
            .unwrap_or_default();
        let animation_active = self.session.preview_deadline().is_some();
        if animation_active {
            if self.smooth_animation_dropped.is_none() {
                self.smooth_bar_startup_animation = suppress_startup_bar_jank(
                    self.bar,
                    self.smooth_user_interacted,
                    now.duration_since(self.smooth_started_at),
                );
            }
            *self.smooth_animation_dropped.get_or_insert(0) += missed;
        } else if let Some(dropped) = self.smooth_animation_dropped.take() {
            if self.smooth_bar_startup_animation {
                eprintln!(
                    "FRAME_JANK_SUPPRESSED surface=bar dropped={} reason=startup_before_input",
                    dropped + missed
                );
            } else {
                eprintln!("FRAME_JANK surface={name} dropped={}", dropped + missed);
            }
            self.smooth_bar_startup_animation = false;
        }
        if self.smooth_scroll_started.is_some() {
            self.smooth_scroll_dropped += missed;
            if self
                .smooth_scroll_reported
                .is_none_or(|last| now.duration_since(last) >= std::time::Duration::from_millis(80))
            {
                eprintln!(
                    "SCROLL_JANK surface={name} dropped={}",
                    self.smooth_scroll_dropped
                );
                self.smooth_scroll_reported = Some(now);
                self.smooth_scroll_dropped = 0;
            }
        }
        self.smooth_last_presented = Some(now);
        if let Some(warmer) = &mut self.icon_warmer {
            warmer.start();
        }
        if self.session.presented(revision).is_err() {
            eprintln!("RECOVERY_ERROR code=save_bundle");
        }
    }
    fn commit(&mut self) -> bool {
        if let Some(scene) = self.pending.take() {
            self.session.commit(scene);
            if self.bar
                && self.session.stores.get("widgets.drag.active")
                    == Some(&bead::core::Value::Bool(true))
                && let Some(geometry) = self
                    .widget_seed()
                    .and_then(|seed| seed["geometry"].as_array().cloned())
            {
                crate::widgets::widget_drag::note_bar_geometry(
                    self.output.as_deref().unwrap_or(""),
                    geometry,
                );
            }
            self.announce_widget_app();
            let wants_seconds = self.session.view_nodes().iter().any(|node| {
                matches!(&node.metadata, Some(bead::core::Value::List(records))
                    if records.first().is_some_and(|seed|
                        (self.widget_app && seed["widget_clock_preview_seconds"] == true)
                        || (!self.widget_app && (self.bar || self.widget_canvas)
                            && seed["widget_clock_seconds"] == true)))
            });
            let paused = self.widget_canvas && self.widgets_paused();
            if wants_seconds && !paused {
                if self.clock_seconds_lease.is_none() {
                    self.clock_seconds_lease = self
                        .desktop
                        .as_ref()
                        .map(|desktop| desktop.clock_seconds_lease());
                }
            } else {
                self.clock_seconds_lease = None;
            }
            if self.widget_canvas
                && self.status_pill_route.is_some()
                && let Err(error) = self.sync_status_peek_route()
            {
                eprintln!("WIDGET_ANCHOR_ERROR {error}");
            }
            if self.widget_editor && !self.widget_app {
                let keys = [
                    "widgets.layout",
                    "widgets.draft",
                    "widgets.editing",
                    "widgets.selected",
                    "widgets.geometry",
                    "widgets.canvas.width",
                    "widgets.canvas.height",
                    "widgets.drag.active",
                    "widgets.drag.source",
                    "widgets.drag.x",
                    "widgets.drag.y",
                ];
                let projection: BTreeMap<_, _> = keys
                    .into_iter()
                    .filter_map(|key| {
                        self.session
                            .stores
                            .get(key)
                            .map(|value| (key.into(), value.clone()))
                    })
                    .collect();
                if self.widgets_projection.as_ref() != Some(&projection) {
                    self.widgets_projection = Some(projection.clone());
                    if let Some(wakeup) = &self.wakeup {
                        wakeup(UiEvent::HostModel(projection));
                    }
                }
            }

            if self.bar {
                let editing = self.session.stores.get("widgets.editing")
                    == Some(&bead::core::Value::Bool(true));
                let draft = self.session.stores.get("widgets.draft").cloned();
                let saved = self.session.stores.get("widgets.layout").cloned();
                let dragging = self.session.stores.get("widgets.drag.active")
                    == Some(&bead::core::Value::Bool(true))
                    && matches!(self.session.stores.get("widgets.drag.phase"),
                        Some(bead::core::Value::Text(phase)) if phase=="dragging"||phase=="settle");
                let transient = self.session.stores.get("widgets.drag.layout").cloned();
                let layout = if dragging
                    && matches!(&transient,Some(bead::core::Value::List(documents)) if !documents.is_empty())
                {
                    transient
                } else if editing
                    && matches!(&draft,Some(bead::core::Value::List(documents)) if !documents.is_empty())
                {
                    draft
                } else {
                    saved
                };
                if layout != self.widgets_canvas_layout || editing != self.widgets_canvas_editing {
                    self.widgets_canvas_editing = editing;
                    self.widgets_canvas_layout = layout.clone();
                    let open = match layout {
                        Some(bead::core::Value::List(documents)) => documents
                            .first()
                            .and_then(|doc| doc["hosts"]["canvas"]["instances"].as_array())
                            .is_some_and(|instances| !instances.is_empty()),
                        _ => false,
                    };
                    let wanted = open;
                    if wanted != self.widgets_canvas_wanted {
                        self.widgets_canvas_wanted = wanted;
                        if let Some(wakeup) = &self.wakeup {
                            wakeup(UiEvent::CanvasOffer {
                                view: "widgets-canvas".into(),
                                output: self.output.clone(),
                                open: wanted,
                                values: self.widget_host_values(),
                            });
                        }
                    }
                }
            }

            self.pending_dashboard = None;
            let covered = self.sync_widgets_covered();
            if self.bar && self.widgets_canvas_wanted && !covered {
                let mut projection = self.widget_host_values();
                if !canvas_needs_seconds(self.widgets_canvas_layout.as_ref()) {
                    projection.remove(CLOCK_SECONDS);
                }
                if self.widgets_projection.as_ref() != Some(&projection) {
                    bead::core::vprintln!(
                        "CANVAS_PROJECTION path=host changed={}",
                        projection_diff(self.widgets_projection.as_ref(), &projection)
                    );
                    self.widgets_projection = Some(projection.clone());
                    if let Some(wakeup) = &self.wakeup {
                        if self.session.stores.get("widgets.editing")
                            == Some(&bead::core::Value::Bool(true))
                        {
                            let live = projection
                                .iter()
                                .filter(|(key, _)| !key.starts_with("widgets."))
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect();
                            wakeup(UiEvent::CompanionModel(live));
                        }
                        if self.widgets_canvas_wanted {
                            wakeup(UiEvent::CanvasModel {
                                view: "widgets-canvas".into(),
                                output: self.output.clone(),
                                values: projection,
                            });
                        }
                    }
                }
            }
            if self.bar && !self.widgets_canvas_wanted {
                self.widgets_projection = None;
            }
            if self.widgets_close_on_commit {
                self.widgets_close_on_commit = false;
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::CompanionVisibility(false));
                }
            }
            if self.launcher.is_some()
                && let Some(memory) = &self.launcher_memory
                && let Ok(mut saved) = memory.lock()
            {
                *saved = crate::launcher::remember(&self.session.stores);
            }
            if let Some(file) = &mut self.state_file
                && let Err(error) = file.save(&self.session.stores)
            {
                eprintln!("STATE_ERROR save: {error}");
            }
            if let Some(view) = self.pending_panel_navigation.take() {
                // Baking the next body must not consume the indicator's visible motion.
                if let Some(animation) = &mut self.dashboard_pearl_animation {
                    animation.started = std::time::Instant::now();
                    self.dashboard_pearl_deadline =
                        Some(animation.started + std::time::Duration::from_millis(16));
                }
                scope_recovery(&mut self.session, self.output.as_deref());
                if let Some(wakeup) = &self.wakeup {
                    wakeup(UiEvent::PanelNavigated(view));
                }
            }
        }
        self.publish_widget_models();
        if self.widget_drag_retire_cleanup {
            self.widget_drag_controller = None;
            self.widget_drag_controller_primed = false;
            self.widget_drag_model = None;
            self.widget_drag_retire_cleanup = false;
            self.widget_drag_wanted = false;
            self.retire_widget_hover_preview();
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        if let Some(previous) = self.pending_dashboard.take() {
            if let Some(dashboard) = &self.dashboard {
                *dashboard.borrow_mut() = previous.view;
            }
            self.dashboard_subscription = previous.subscriptions;
            self.dashboard_pearl_animation = previous.animation;
            self.dashboard_pearl_deadline = previous.deadline;
            self.pending_panel_navigation = None;
        }
        let mut scene = self.session.reject(error);
        if scene.font_files.is_empty()
            && let Ok(files) = crate::font_sources::select(&scene)
        {
            scene.font_files = files;
        }
        scene
    }
}

fn backend_values(
    snapshot: &bead::backends::Snapshot,
    output: Option<&str>,
    workspaces: &crate::workspace_rows::Rows,
) -> Result<std::collections::BTreeMap<String, bead::core::Value>, Box<dyn Error>> {
    use bead::core::Value;
    let output_id = output.and_then(|name| {
        snapshot
            .outputs
            .iter()
            .find(|o| o.name == name)
            .map(|o| &o.id)
    });
    let window_matches = |w: &&bead::backends::Window| {
        output.is_none()
            || output_id.is_some_and(|id| {
                w.output.as_ref() == Some(id)
                    || (w.output.is_none()
                        && w.workspace.as_ref().is_some_and(|ws| {
                            snapshot.workspaces.iter().any(|workspace| {
                                &workspace.id == ws && workspace.output.as_ref() == Some(id)
                            })
                        }))
            })
    };
    let mut values = std::collections::BTreeMap::from([
        (
            "bead.output.name".into(),
            Value::Text(output.unwrap_or_default().into()),
        ),
        ("bead.connected".into(), Value::Bool(snapshot.connected)),
        (
            "bead.focused_title".into(),
            Value::Text(focused_title(snapshot, output, None).0),
        ),
        (
            "bead.outputs".into(),
            Value::list(
                snapshot
                    .outputs
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
        (
            "bead.workspaces".into(),
            Value::list(workspaces.rows(snapshot, output)),
        ),
        (
            "bead.windows".into(),
            Value::list(
                snapshot
                    .windows
                    .iter()
                    .filter(window_matches)
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
    ]);
    for (name, supported) in &snapshot.capabilities.features {
        values.insert(format!("bead.cap.{name}"), Value::Bool(*supported));
    }
    Ok(values)
}

fn backend_title_only(
    values: &BTreeMap<String, bead::core::Value>,
    mut current: impl FnMut(&str) -> Option<bead::core::Value>,
) -> bool {
    values.iter().all(|(id, value)| {
        if id == "bead.focused_title" {
            return true;
        }
        let Some(old) = current(id) else {
            return false;
        };
        if id == "bead.windows" {
            return same_windows_except_titles(value, &old);
        }
        old == *value
    })
}

fn same_windows_except_titles(new: &bead::core::Value, old: &bead::core::Value) -> bool {
    let (bead::core::Value::List(new), bead::core::Value::List(old)) = (new, old) else {
        return new == old;
    };
    let without_titles = |items: &[serde_json::Value]| {
        items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(object) = item.as_object_mut() {
                    object.remove("title");
                }
                item
            })
            .collect::<Vec<_>>()
    };
    without_titles(new) == without_titles(old)
}

#[cfg(test)]
mod backend_title_update_tests {
    use super::*;

    #[test]
    fn backend_window_title_churn_is_title_only_but_keeps_window_values() {
        use bead::core::Value;
        let make_snapshot = |title: &str| bead::backends::Snapshot {
            backend: "fixture".into(),
            connected: true,
            windows: vec![bead::backends::Window {
                id: bead::backends::Id("w".into()),
                title: title.into(),
                app_id: "chat".into(),
                workspace: None,
                output: None,
                focused: true,
                floating: false,
                fullscreen: None,
                urgent: false,
            }],
            ..Default::default()
        };
        let before = backend_values(
            &make_snapshot("◐ …"),
            None,
            &crate::workspace_rows::Rows::default(),
        )
        .unwrap();
        let after = backend_values(
            &make_snapshot("◑ …"),
            None,
            &crate::workspace_rows::Rows::default(),
        )
        .unwrap();
        assert!(backend_title_only(&after, |id| before.get(id).cloned()));
        assert_ne!(before.get("bead.windows"), after.get("bead.windows"));

        let changed_identity = BTreeMap::from([(
            "bead.windows".into(),
            Value::list(vec![serde_json::json!({"id": "other", "title": "◑ …"})]),
        )]);
        assert!(!backend_title_only(&changed_identity, |id| before
            .get(id)
            .cloned()));
    }
}

/// Retains the last caption through a transient focus clear while its window
/// remains on the active workspace, matching Nacre's bar title behavior.
fn focused_title(
    snapshot: &bead::backends::Snapshot,
    output: Option<&str>,
    previous: Option<&bead::backends::Id>,
) -> (String, Option<bead::backends::Id>) {
    if !snapshot.connected {
        return (String::new(), None);
    }
    let output_id = output.and_then(|name| {
        snapshot
            .outputs
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| &candidate.id)
    });
    if output.is_some() && output_id.is_none() {
        return (String::new(), None);
    }
    let window_matches = |window: &&bead::backends::Window| {
        output.is_none()
            || output_id.is_some_and(|id| {
                window.output.as_ref() == Some(id)
                    || (window.output.is_none()
                        && window.workspace.as_ref().is_some_and(|workspace_id| {
                            snapshot.workspaces.iter().any(|workspace| {
                                &workspace.id == workspace_id
                                    && workspace.output.as_ref() == Some(id)
                            })
                        }))
            })
    };
    let focused = snapshot
        .windows
        .iter()
        .filter(window_matches)
        .find(|window| window.focused);
    if let Some(window) = focused {
        return (window.title.clone(), Some(window.id.clone()));
    }

    let retained = previous.and_then(|id| {
        snapshot
            .windows
            .iter()
            .filter(window_matches)
            .find(|window| {
                &window.id == id
                    && snapshot.workspaces.iter().any(|workspace| {
                        (workspace.focused || workspace.active)
                            && (output_id.is_none() || workspace.output.as_ref() == output_id)
                            && window.workspace.as_ref() == Some(&workspace.id)
                    })
            })
    });
    retained.map_or_else(
        || (String::new(), None),
        |window| (window.title.clone(), Some(window.id.clone())),
    )
}

/// Exercises actual bar hit testing and native service routes on explicit proof buses.
pub fn private_service_proof() -> Result<(), Box<dyn Error>> {
    if std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref() != Ok("1") {
        return Err("requires private proof environment".into());
    }
    let palette = bead::render::Palette::read(std::path::Path::new("shell/palette.json"))?;
    let fallback = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: vec![],
        width: 1920.,
        slide: None,
        primitives: vec![],
        offset: (0., 0.),
        bounds: bead::render::Rect::new(0., 0., 1920., 50.),
    };
    let mut session = bead::ui::Session::new(
        "shell".into(),
        "bar/main.luau".into(),
        "shell/palette.json".into(),
        palette,
        fallback,
    );
    session.set_viewport(1920, 1200);
    session.stores = session
        .stores
        .reconcile(&std::collections::BTreeMap::from([
            (
                "bead.focused_title".into(),
                bead::core::Value::Text("Fixture title".into()),
            ),
            ("bead.workspaces".into(), bead::core::Value::list(vec![])),
        ]))?;
    let mut runtime = Runtime::new(session).with_desktop()?;
    let desktop = runtime
        .desktop
        .as_ref()
        .ok_or("proof desktop unavailable")?
        .clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while desktop.snapshot().audio.volume.is_none()
        || desktop.snapshot().tray.iter().all(|t| t.menu.is_empty())
    {
        if std::time::Instant::now() > deadline {
            return Err("proof services timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    runtime
        .session
        .update(crate::desktop::values(&desktop.snapshot(), ""))?;
    runtime.stage(UiEvent::Reload)?;
    runtime.commit();
    fn position(runtime: &Runtime, action: &str) -> Result<(f32, f32), Box<dyn Error>> {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|n| n.action.as_deref() == Some(action))
            .ok_or("action node missing")?;
        let bead::render::Primitive::Rounded { rect, .. } = node.draw else {
            return Err("action bounds missing".into());
        };
        Ok((rect.x + rect.w / 2., rect.y + rect.h / 2.))
    }
    let (x, y) = position(&runtime, "services.audio")?;
    let previous = desktop
        .snapshot()
        .audio
        .muted
        .ok_or("proof mute unavailable")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while desktop.snapshot().audio.muted == Some(previous) {
        if std::time::Instant::now() > deadline {
            return Err("click mute not observed".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before = desktop
        .snapshot()
        .audio
        .volume
        .ok_or("proof volume unavailable")?;
    runtime.stage(UiEvent::Scroll { x, y, delta: -1. })?;
    runtime.commit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while (desktop
        .snapshot()
        .audio
        .volume
        .ok_or("proof volume unavailable")?
        - before)
        .abs()
        < 0.01
    {
        if std::time::Instant::now() > deadline {
            return Err("scroll volume not observed".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    runtime.stage(UiEvent::Desktop(desktop.snapshot()))?;
    runtime.commit();
    let (x, y) = position(&runtime, "services.tray")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    let item_menu = desktop.snapshot().tray[0].item_is_menu;
    if !item_menu {
        runtime.stage(UiEvent::ContextMenu { x, y })?;
        runtime.commit();
    }
    let (x, y) = position(&runtime, "services.menu")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert!(runtime.menu_item.is_empty());
    println!(
        "BAR_SERVICE_ROUTES click_mute=true scroll_volume=true tray_activate_or_item_menu=true right_click_prepare=true menu_event=true item_is_menu={item_menu}"
    );
    Ok(())
}

#[cfg(test)]
mod audio_runtime_tests;
#[cfg(test)]
mod bar_routes_runtime_tests;
#[cfg(test)]
mod bluetooth_power_tests;
#[cfg(test)]
mod chat_runtime_tests;
#[cfg(test)]
mod join_runtime_tests;
#[cfg(test)]
mod launcher_reopen_tests;
#[cfg(test)]
mod link_runtime_tests;
#[cfg(test)]
mod output_runtime_tests;
#[cfg(test)]
mod pairing_runtime_tests;
#[cfg(test)]
mod prompt_runtime_tests;
#[cfg(test)]
mod scroll_runtime_tests;
#[cfg(test)]
mod session_menu_runtime_tests;
#[cfg(test)]
mod shell_route_smoke_tests;
#[cfg(test)]
mod widget_app_tests;
#[cfg(test)]
mod widget_drag_preview_tests;
#[cfg(test)]
mod widget_editor_tests;
#[cfg(test)]
mod widget_layout_property_tests;
#[cfg(test)]
mod widget_startup_tests;
#[cfg(test)]
mod wifi_runtime_tests;
