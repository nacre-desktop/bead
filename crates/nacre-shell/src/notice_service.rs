// SPDX-License-Identifier: GPL-3.0-only
//! B0020: the resident notice stack, history and OSD as a consumer of the daemon's
//! published `notifications` topic and `osd` events (N0017, N0043). The daemon owns
//! the bus name, expiry, history and the DND setting; this side only projects
//! snapshots, keeps local expansion/hover/OSD-hide state, and sends the published
//! revision-checked methods. It never starts a bus owner or replays a mutation.
//! B0030: it runs inside the main shell on its UI-provider daemon link. Idle it
//! holds no surface; a bar session offers the transient `notices` and `osd` panel
//! views, which never displace a view the user opened.
use crate::{
    i18n::tr,
    nacre_source::{CallGuard, CallTicket, CallWake, Client},
    notifications::{Handle, Notices, Wakeup},
};
use bead::{
    core::{
        Value,
        source::{Snapshot, Source, Subscription},
    },
    wayland::UiEvent,
};
use nacre_protocol::{
    core_services::{Notification, NotificationsState},
    osd::{OsdEvent, OsdKind},
};
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::Hash,
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Host actions the notice views may bind. `notifications.osd` only marks the OSD
/// card so pointer hover can pause its hide timer.
pub const ACTIONS: [&str; 10] = [
    "notifications.card",
    "notifications.details",
    "notifications.dismiss",
    "notifications.forget",
    "notifications.action",
    "notifications.dnd",
    "notifications.feedback",
    "notifications.clear",
    "notifications.clear_popups",
    "notifications.osd",
];
/// QML `NacreOsd.hideDelay` and `NacreOsdEvents` arming interval.
const OSD_HIDE: Duration = Duration::from_millis(2000);
const OSD_ARM: Duration = Duration::from_millis(400);
const MAX_CALLS: usize = 16;

/// Topics the shared daemon link must subscribe for this service. `compositor` only
/// names the focused output for new popups; without it they use the leading output.
pub const TOPICS: [&str; 3] = ["notifications", "osd", "compositor"];
/// Overlay views the service offers (see `UiEvent::CompanionOffer`).
pub const VIEW_NOTICES: &str = "notices";
pub const VIEW_OSD: &str = "osd";

#[derive(Default)]
pub struct Options {
    /// View families that may stay shown beside popups (launcher, dashboard...): a
    /// view named `p` or `p-...` for a prefix `p`. Every other panel view suppresses
    /// popups, as QML does for popouts, the session menu and the control drawer.
    pub coexist: Vec<String>,
    /// QML `NacreNotifications.actionOnClick`: no published runtime source, so the
    /// frozen default (false) holds unless the launcher sets this flag.
    pub action_on_click: bool,
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis().min(u128::from(u64::MAX)) as u64)
}

/// Defaults for every model key, so the first paint never reads an unknown name.
pub fn seed() -> BTreeMap<String, Value> {
    let mut out = crate::notifications::values(&Json::Null);
    out.extend(project(None, false, &BTreeSet::new(), 0));
    out.insert("notifications.message".into(), Value::Text(String::new()));
    out
}

/// Pointer-hover identity of a card (`id|revision`) or the OSD; `None` elsewhere.
pub fn hover_key(name: &str, arg: &Value) -> Option<String> {
    if name == "notifications.osd" {
        return Some("osd".into());
    }
    if !matches!(
        name,
        "notifications.card"
            | "notifications.details"
            | "notifications.dismiss"
            | "notifications.action"
    ) {
        return None;
    }
    let Value::Text(text) = arg else { return None };
    Some(text.split('|').take(2).collect::<Vec<_>>().join("|"))
}

fn time_str(age_ms: u64) -> String {
    let s = age_ms / 1000;
    match s {
        0..60 => "now".into(),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}
fn urgency(n: &Notification) -> u8 {
    match n.urgency.as_str() {
        "critical" => 2,
        "low" => 0,
        _ => 1,
    }
}
fn local_icon_path(value: &str) -> Option<String> {
    let value = value.strip_prefix("file://localhost").unwrap_or(value);
    let value = value.strip_prefix("file://").unwrap_or(value);
    std::path::Path::new(value)
        .is_absolute()
        .then(|| value.to_owned())
}
fn theme_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn choose_icon_path(
    icon: Option<&str>,
    desktop_entry: Option<&str>,
    image_path: Option<&str>,
    mut is_file: impl FnMut(&str) -> bool,
    mut theme_icon: impl FnMut(&str) -> Option<String>,
    mut desktop_icon: impl FnMut(&str) -> Option<String>,
    fallback: &str,
) -> String {
    if let Some(icon) = icon {
        if std::path::Path::new(icon)
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "nacre" | "nacre-notifications"
                )
            })
        {
            return fallback.to_owned();
        }
        if let Some(path) = local_icon_path(icon).filter(|p| is_file(p)) {
            return path;
        }
        if theme_name(icon)
            && let Some(path) = theme_icon(icon)
        {
            return path;
        }
    }
    if let Some(entry) = desktop_entry {
        if let Some(name) = desktop_icon(entry) {
            if let Some(path) = local_icon_path(&name).filter(|p| is_file(p)) {
                return path;
            }
            if theme_name(&name)
                && let Some(path) = theme_icon(&name)
            {
                return path;
            }
        }
        let basename = entry.rsplit('/').next().unwrap_or(entry);
        let name = basename.strip_suffix(".desktop").unwrap_or(basename);
        if theme_name(name)
            && let Some(path) = theme_icon(name)
        {
            return path;
        }
    }
    if let Some(path) = image_path.and_then(local_icon_path).filter(|p| is_file(p)) {
        return path;
    }
    fallback.to_owned()
}
/// Preserve the daemon's app icon hints after resolving usable assets at icon scale.
pub(crate) fn icon_fields(
    icon: Option<&str>,
    desktop_entry: Option<&str>,
    image_path: Option<&str>,
) -> Json {
    let theme = icon_theme();
    let key = IconQueryKey {
        theme: theme.clone(),
        icon: icon.unwrap_or_default().to_owned(),
        desktop_entry: desktop_entry.unwrap_or_default().to_owned(),
        image_path: image_path.unwrap_or_default().to_owned(),
        size: 48,
    };
    let cache = ICON_PATHS.get_or_init(|| Mutex::new(IconPathCache::new(theme.clone())));
    let path = {
        let mut cache = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        cache.set_theme(theme.clone());
        if let Some(path) = cache.query(&key) {
            path
        } else {
            let path = choose_icon_path(
                icon,
                desktop_entry,
                image_path,
                |path| std::path::Path::new(path).is_file(),
                |name| cache.theme_icon(&theme, name, key.size),
                crate::workspace_rows::desktop_icon_name,
                "",
            );
            cache.insert_query(key, path.clone());
            path
        }
    };
    json!({
        "icon": icon.unwrap_or_default(),
        "icon_path": path,
        "desktop_entry": desktop_entry.unwrap_or_default(),
        "image_path": image_path.unwrap_or_default(),
    })
}

const ICON_CACHE_LIMIT: usize = 512;
static ICON_PATHS: OnceLock<Mutex<IconPathCache>> = OnceLock::new();
static ICON_THEME: OnceLock<Mutex<String>> = OnceLock::new();

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconKey {
    theme: String,
    name: String,
    size: u16,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconQueryKey {
    theme: String,
    icon: String,
    desktop_entry: String,
    image_path: String,
    size: u16,
}

struct Lru<K: Eq + Hash + Clone, V: Clone> {
    values: HashMap<K, (V, u64)>,
    clock: u64,
}
impl<K: Eq + Hash + Clone, V: Clone> Default for Lru<K, V> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            clock: 0,
        }
    }
}
impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    fn get(&mut self, key: &K) -> Option<V> {
        self.clock = self.clock.wrapping_add(1);
        let (value, used) = self.values.get_mut(key)?;
        *used = self.clock;
        Some(value.clone())
    }
    fn insert(&mut self, key: K, value: V) {
        self.clock = self.clock.wrapping_add(1);
        if !self.values.contains_key(&key) && self.values.len() >= ICON_CACHE_LIMIT {
            if let Some(oldest) = self
                .values
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone())
            {
                self.values.remove(&oldest);
            }
        }
        self.values.insert(key, (value, self.clock));
    }
    fn clear(&mut self) {
        self.values.clear();
    }
}

struct IconPathCache {
    theme: String,
    lookups: Lru<IconKey, Option<String>>,
    queries: Lru<IconQueryKey, String>,
}
impl IconPathCache {
    fn new(theme: String) -> Self {
        Self {
            theme,
            lookups: Lru::default(),
            queries: Lru::default(),
        }
    }
    fn set_theme(&mut self, theme: String) {
        if self.theme != theme {
            self.theme = theme;
            self.lookups.clear();
            self.queries.clear();
        }
    }
    fn theme_icon(&mut self, theme: &str, name: &str, size: u16) -> Option<String> {
        let key = IconKey {
            theme: theme.to_owned(),
            name: name.to_owned(),
            size,
        };
        self.lookup_or_insert(key, || {
            if theme.is_empty() {
                freedesktop_icons::lookup(name).with_size(size).find()
            } else {
                freedesktop_icons::lookup(name)
                    .with_theme(theme)
                    .with_size(size)
                    .find()
            }
            .map(|path| path.to_string_lossy().into_owned())
        })
    }
    fn lookup_or_insert(
        &mut self,
        key: IconKey,
        resolve: impl FnOnce() -> Option<String>,
    ) -> Option<String> {
        if let Some(path) = self.lookups.get(&key) {
            return path;
        }
        let path = resolve();
        self.lookups.insert(key, path.clone());
        path
    }
    fn query(&mut self, key: &IconQueryKey) -> Option<String> {
        self.queries.get(key)
    }
    fn insert_query(&mut self, key: IconQueryKey, path: String) {
        self.queries.insert(key, path);
    }
}

fn icon_theme() -> String {
    ICON_THEME
        .get_or_init(|| {
            Mutex::new(freedesktop_icons::default_theme_gtk().unwrap_or_default())
        })
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Re-read the effective icon theme only on a shell reload; a changed theme
/// invalidates both positive and negative icon lookup results.
pub(crate) fn refresh_icon_theme() {
    let next = freedesktop_icons::default_theme_gtk().unwrap_or_default();
    let current = ICON_THEME.get_or_init(|| Mutex::new(next.clone()));
    let mut current = current.lock().unwrap_or_else(|poison| poison.into_inner());
    if *current != next {
        *current = next.clone();
        if let Some(cache) = ICON_PATHS.get() {
            cache
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .set_theme(next);
        }
    }
}

#[cfg(test)]
mod icon_cache_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn lookup_cache_hits_for_each_theme_name_size_and_invalidates_on_theme_change() {
        let mut cache = IconPathCache::new("one".into());
        let calls = Cell::new(0);
        for _ in 0..40 {
            assert_eq!(
                cache.lookup_or_insert(
                    IconKey {
                        theme: "one".into(),
                        name: "same-icon".into(),
                        size: 48,
                    },
                    || {
                        calls.set(calls.get() + 1);
                        Some("/icons/same-icon.svg".into())
                    },
                ),
                Some("/icons/same-icon.svg".into())
            );
        }
        assert_eq!(calls.get(), 1);
        cache.set_theme("two".into());
        cache.lookup_or_insert(
            IconKey {
                theme: "two".into(),
                name: "same-icon".into(),
                size: 48,
            },
            || {
                calls.set(calls.get() + 1);
                None
            },
        );
        assert_eq!(calls.get(), 2, "negative results are cached per theme too");
        cache.lookup_or_insert(
            IconKey {
                theme: "two".into(),
                name: "same-icon".into(),
                size: 48,
            },
            || {
                calls.set(calls.get() + 1);
                Some("unexpected".into())
            },
        );
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn icon_path_queries_are_bounded_lru_entries() {
        let mut cache = IconPathCache::new("theme".into());
        for i in 0..ICON_CACHE_LIMIT + 1 {
            cache.insert_query(
                IconQueryKey {
                    theme: "theme".into(),
                    icon: format!("icon-{i}"),
                    desktop_entry: String::new(),
                    image_path: String::new(),
                    size: 48,
                },
                String::new(),
            );
        }
        assert_eq!(cache.queries.values.len(), ICON_CACHE_LIMIT);
        assert!(cache
            .query(&IconQueryKey {
                theme: "theme".into(),
                icon: "icon-0".into(),
                desktop_entry: String::new(),
                image_path: String::new(),
                size: 48,
            })
            .is_none());
    }
}
fn row(id: &str, n: &Notification, expanded: bool, history: bool, now: u64) -> Json {
    let rev = n.revision.unwrap_or(0);
    let default = n.default_action.as_ref().map_or("", |d| d.0.as_str());
    // The default action is never a button; history rows have no live actions.
    let actions: Vec<Json> = if history {
        vec![]
    } else {
        n.actions
            .order
            .iter()
            .filter(|a| a.0 != default)
            .filter_map(|a| {
                n.actions.items.get(a).map(
                    |x| json!({"id": a.0, "ref": format!("{id}|{rev}|{}", a.0), "label": x.label}),
                )
            })
            .collect()
    };
    let mut row = json!({"id": id, "rev": rev, "key": format!("{id}|{rev}"),
        "app": n.app_name, "summary": n.summary, "body": n.body,
        "urgency": urgency(n),
        "expanded": expanded, "actions": actions, "default": default,
        "time_str": time_str(now.saturating_sub(n.created_ms)), "history": history});
    if let Some(fields) = icon_fields(
        n.icon.as_deref(),
        n.desktop_entry.as_deref(),
        n.image_path.as_deref(),
    )
    .as_object()
    {
        for (key, value) in fields {
            row[key] = value.clone();
        }
    }
    row
}

/// Pure projection of the last-good snapshot. `live` is false while disconnected
/// or while the producer reports itself stale/unavailable: the retained display
/// stays, popups and controls go.
fn project(
    good: Option<&NotificationsState>,
    live: bool,
    expanded: &BTreeSet<String>,
    now: u64,
) -> BTreeMap<String, Value> {
    let mut popups = vec![];
    let mut history = vec![];
    let (mut dnd, mut inhibited, mut count, mut more) = (false, false, None, false);
    let (mut storage, mut writable) = ("unavailable", false);
    if let Some(s) = good {
        dnd = s.do_not_disturb;
        inhibited = s.inhibited == Some(true);
        count = s.history_count;
        more = s.history_more == Some(true);
        if live {
            for id in s.popup_order.iter().flatten() {
                if let Some(n) = s.active.items.get(id) {
                    popups.push(row(&id.0, n, expanded.contains(&id.0), false, now));
                }
            }
        }
        for id in &s.history.order {
            if let Some(n) = s.history.items.get(id) {
                history.push(row(&id.0, n, expanded.contains(&id.0), true, now));
            }
        }
        let st = s.history_storage.as_ref();
        writable = st.is_some_and(|x| x.writable);
        storage = match st {
            Some(x) if x.error.is_some() => "error",
            Some(x) if x.ready => "ready",
            _ => "loading",
        };
    }
    let state = match (good.is_some(), live) {
        (true, true) => "ready",
        (true, false) => "stale",
        _ => "unavailable",
    };
    BTreeMap::from([
        ("notifications.open".into(), Value::Bool(!popups.is_empty())),
        ("notifications.rows".into(), Value::list(popups)),
        ("notifications.dnd".into(), Value::Bool(dnd)),
        ("notifications.inhibited".into(), Value::Bool(inhibited)),
        ("notifications.state".into(), Value::Text(state.into())),
        ("notifications.controls".into(), Value::Bool(live)),
        ("notifications.history".into(), Value::list(history)),
        (
            "notifications.history_count".into(),
            Value::Number(count.unwrap_or(0) as f64),
        ),
        (
            "notifications.history_known".into(),
            Value::Bool(count.is_some()),
        ),
        ("notifications.history_more".into(), Value::Bool(more)),
        (
            "notifications.history_state".into(),
            Value::Text(storage.into()),
        ),
        ("notifications.writable".into(), Value::Bool(writable)),
    ])
}

/// What one UI action turns into, decided against the current snapshot.
#[derive(Debug, PartialEq)]
pub(crate) enum Plan {
    Toggle(String),
    Call(&'static str, Json),
}
pub(crate) fn plan(
    name: &str,
    arg: &Value,
    state: &NotificationsState,
    action_on_click: bool,
) -> Result<Plan, &'static str> {
    let text = if let Value::Text(t) = arg {
        t.as_str()
    } else {
        ""
    };
    let mut parts = text.split('|');
    let (id, rev, extra) = (
        parts.next().unwrap_or(""),
        parts.next().and_then(|r| r.parse::<u64>().ok()),
        parts.next(),
    );
    let notice = || {
        let rev = rev.ok_or("invalid notification reference")?;
        let n = state
            .active
            .items
            .iter()
            .chain(state.history.items.iter())
            .find(|(k, _)| k.0 == id)
            .map(|(_, n)| n)
            .ok_or("bead.common.changed-elsewhere")?;
        (n.revision == Some(rev))
            .then_some((n, rev))
            .ok_or("bead.common.changed-elsewhere")
    };
    Ok(match name {
        "notifications.details" => Plan::Toggle(id.into()),
        "notifications.card" => {
            let (n, rev) = notice()?;
            let live = state.active.items.keys().any(|k| k.0 == id);
            match n
                .default_action
                .as_ref()
                .filter(|_| action_on_click && live)
            {
                Some(default) => Plan::Call(
                    "notifications.invoke_action",
                    json!({"id": id, "revision": rev, "action": default.0}),
                ),
                None => Plan::Toggle(id.into()),
            }
        }
        "notifications.dismiss" | "notifications.forget" => {
            let (_, rev) = notice()?;
            Plan::Call(
                "notifications.dismiss",
                json!({"id": id, "revision": rev, "forget": name == "notifications.forget"}),
            )
        }
        "notifications.action" => {
            let (n, rev) = notice()?;
            let action = extra.ok_or("invalid action reference")?;
            let live = state.active.items.keys().any(|k| k.0 == id);
            if !live || !n.actions.items.keys().any(|a| a.0 == action) {
                return Err("bead.common.changed-elsewhere");
            }
            Plan::Call(
                "notifications.invoke_action",
                json!({"id": id, "revision": rev, "action": action}),
            )
        }
        "notifications.dnd" => {
            let enabled = match arg {
                Value::Bool(b) => *b,
                _ => !state.do_not_disturb,
            };
            Plan::Call("notifications.set_dnd", json!({"enabled": enabled}))
        }
        "notifications.clear" => Plan::Call("notifications.clear", json!({"scope": "history"})),
        "notifications.clear_popups" => {
            Plan::Call("notifications.clear", json!({"scope": "popups"}))
        }
        _ => return Err("unknown notification action"),
    })
}

/// What the OSD view shows: the last `osd` event, cleaned. The kind-to-glyph/label
/// mapping is data in `shell/osd/kinds.luau`; the host only projects the event.
struct Shown {
    kind: OsdKind,
    /// Level fraction 0..=1 (volume is clamped), or 1/0 for the toggles.
    value: f64,
    muted: bool,
    label: String,
    device: String,
}
fn kind_name(kind: OsdKind) -> &'static str {
    match kind {
        OsdKind::Volume => "volume",
        OsdKind::Output => "output",
        OsdKind::Microphone => "microphone",
        OsdKind::Brightness => "brightness",
        OsdKind::Keyboard => "keyboard",
        OsdKind::NightLight => "night_light",
        OsdKind::PowerProfile => "power_profile",
        OsdKind::Capslock => "capslock",
    }
}
fn shown_from(event: &OsdEvent) -> Option<Shown> {
    event.valid().then(|| Shown {
        kind: event.kind,
        value: event.value.unwrap_or(0.).clamp(0., 1.),
        muted: event.muted == Some(true),
        label: event.label.clone().unwrap_or_default(),
        device: event.device.clone().unwrap_or_default(),
    })
}
/// Model keys of the OSD view; hidden keeps the last values for the close animation.
fn osd_values(shown: Option<&Shown>) -> BTreeMap<String, Value> {
    let Some(s) = shown else {
        return BTreeMap::from([("osd.open".into(), Value::Bool(false))]);
    };
    BTreeMap::from([
        ("osd.open".into(), Value::Bool(true)),
        ("osd.kind".into(), Value::Text(kind_name(s.kind).into())),
        ("osd.value".into(), Value::Number(s.value)),
        ("osd.muted".into(), Value::Bool(s.muted)),
        ("osd.label".into(), Value::Text(s.label.clone())),
        ("osd.device".into(), Value::Text(s.device.clone())),
    ])
}

#[derive(Default)]
struct Osd {
    armed: Option<Instant>,
    shown: Option<Shown>,
    hovered: bool,
    deadline: Option<Instant>,
}
impl Osd {
    fn show(&mut self, shown: Shown, now: Instant) {
        if self.armed.is_none_or(|t| now < t) {
            return;
        }
        self.shown = Some(shown);
        self.deadline = (!self.hovered).then_some(now + OSD_HIDE);
    }
}

#[derive(Default)]
struct Inner {
    good: Option<NotificationsState>,
    live: bool,
    expanded: BTreeSet<String>,
    osd: Osd,
    message: String,
    calls: Vec<(String, CallTicket)>,
    wakeups: BTreeMap<u64, Wakeup>,
    /// Bar sessions offering the panel views; the lowest id leads.
    drivers: BTreeMap<u64, Wakeup>,
    /// The retained count last sent to the bars (None until a bar is driven).
    bar_count: Option<u64>,
    next: u64,
    /// Another view (launcher, dashboard, popout) is shown: popups are suppressed.
    other_panel: bool,
    /// Compositor-focused output name (published `compositor` topic), if known.
    focus: Option<String>,
    /// See [`Options::coexist`].
    coexist: Vec<String>,
    offered: Option<&'static str>,
    hover: Option<(String, u64)>,
    /// The daemon refused provider-only calls: this process is not the UI provider.
    denied: bool,
    sent_inhibit: bool,
    stop: bool,
}
struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn models(g: &Inner) -> BTreeMap<String, Value> {
    let mut out = project(g.good.as_ref(), g.live, &g.expanded, wall_ms());
    out.extend(osd_values(g.osd.shown.as_ref()));
    out.insert(
        "notifications.message".into(),
        Value::Text(if g.message.is_empty() {
            String::new()
        } else {
            tr(&g.message, &[])
        }),
    );
    out
}
fn popups_open(g: &Inner) -> bool {
    g.live
        && g.good.as_ref().is_some_and(|s| {
            s.popup_order
                .iter()
                .flatten()
                .any(|id| s.active.items.contains_key(id))
        })
}
/// The transient view that should be up: the OSD, else the popup stack unless
/// another view is shown (the daemon hides popups then, and never replays them).
fn wanted(g: &Inner) -> Option<&'static str> {
    if g.osd.shown.is_some() {
        Some(VIEW_OSD)
    } else if !g.other_panel && popups_open(g) {
        Some(VIEW_NOTICES)
    } else {
        None
    }
}
/// Pushes the current models and view offers; wake-ups run outside the lock.
fn publish(shared: &Shared) {
    let (values, offer, output, wakeups, drivers, count) = {
        let mut g = shared.lock();
        let want = wanted(&g);
        let close = if want.is_none() {
            g.offered.take()
        } else {
            None
        };
        g.offered = want.or(g.offered);
        let values = models(&g);
        // Use the authoritative stored total when known. While storage is loading or
        // unavailable, QML counts the retained rows it already has, rather than zero.
        let retained = g.good.as_ref().map_or(0, |state| {
            state.history_count.unwrap_or_else(|| {
                (state.history.items.len()
                    + state
                        .active
                        .items
                        .values()
                        .filter(|n| n.retained == Some(true))
                        .count()) as u64
            })
        });
        let count = (g.bar_count != Some(retained)).then_some(retained);
        g.bar_count = Some(retained);
        (
            values,
            want.map(|v| (v, true)).or(close.map(|v| (v, false))),
            g.focus.clone(),
            g.wakeups.values().cloned().collect::<Vec<_>>(),
            g.drivers.values().cloned().collect::<Vec<_>>(),
            count,
        )
    };
    for wakeup in wakeups {
        wakeup(UiEvent::Model(values.clone()));
    }
    if let Some(count) = count {
        for driver in &drivers {
            driver(UiEvent::Model(bar_count(count)));
        }
    }
    if let (Some(driver), Some((view, open))) = (drivers.first(), offer) {
        driver(UiEvent::CompanionOffer {
            view: view.into(),
            output,
            open,
        });
    }
}
fn bar_count(count: u64) -> BTreeMap<String, Value> {
    BTreeMap::from([(
        "bar.notifications.count".into(),
        Value::Number(count as f64),
    )])
}

/// Whether panel view `view` suppresses popups: every view outside the coexisting
/// families (`p` or `p-...`).
fn suppresses(coexist: &[String], view: &str) -> bool {
    !coexist.iter().any(|p| {
        view == p
            || view
                .strip_prefix(p.as_str())
                .is_some_and(|rest| rest.starts_with('-'))
    })
}
/// New popups follow the compositor focus; a missing or absent topic clears it.
fn on_compositor(shared: &Shared, snap: &Snapshot) {
    let focus = snap
        .connected
        .then(|| snap.data["focus"]["outputId"].as_str())
        .flatten()
        .filter(|name| !name.is_empty() && name.len() <= 128)
        .map(str::to_owned);
    shared.lock().focus = focus;
}
fn on_notifications(shared: &Shared, snap: &Snapshot) {
    {
        let mut g = shared.lock();
        let parsed = serde_json::from_value::<NotificationsState>((*snap.data).clone())
            .ok()
            .filter(|s| s.producer_valid());
        match (snap.connected, parsed) {
            (true, Some(state)) => {
                g.live = state.health.available && !state.health.stale;
                let ids: BTreeSet<_> = state
                    .active
                    .items
                    .keys()
                    .chain(state.history.items.keys())
                    .map(|k| k.0.clone())
                    .collect();
                g.expanded.retain(|k| ids.contains(k));
                if g.hover
                    .as_ref()
                    .is_some_and(|(id, _)| !state.popup_order.iter().flatten().any(|p| &p.0 == id))
                {
                    g.hover = None;
                }
                g.good = Some(state);
            }
            // Absent capability, legacy snapshot or malformed payload: nothing is
            // displayed from it, and last-good data is kept visibly stale.
            _ => {
                g.live = false;
                g.hover = None;
                g.sent_inhibit = false;
                g.denied = false;
                if !snap.connected {
                    g.calls.clear();
                }
            }
        }
    }
    shared.wake.notify_all();
    publish(shared);
}
/// One `osd` event (N0043). The daemon coalesces bursts and never replays, so each
/// event is a real change to show now.
fn deliver(shared: &Shared, event: &OsdEvent) {
    let Some(shown) = shown_from(event) else {
        return;
    };
    shared.lock().osd.show(shown, Instant::now());
    shared.wake.notify_all();
    publish(shared);
}
/// Live services that receive `osd` events from the shell's one daemon link (the
/// link's event hook is fixed before the service exists, as for the chat events).
static SINKS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());
pub fn route_event(message: &nacre_protocol::Message) {
    use nacre_protocol::{Message, osd};
    let Message::Ev {
        topic, name, data, ..
    } = message
    else {
        return;
    };
    if topic != osd::TOPIC || name != osd::EVENT {
        return;
    }
    let Ok(event) = serde_json::from_value::<OsdEvent>(data.clone()) else {
        return;
    };
    let sinks: Vec<_> = {
        let mut all = SINKS.lock().unwrap_or_else(|p| p.into_inner());
        all.retain(|w| w.strong_count() > 0);
        all.iter().filter_map(Weak::upgrade).collect()
    };
    for shared in sinks {
        deliver(&shared, &event);
    }
}

/// Hides the OSD at its deadline and keeps the provider-only suppression in sync.
/// Calls are made here, never inside a topic callback on the transport thread.
fn worker(shared: Arc<Shared>, client: Client) {
    let mut g = shared.lock();
    loop {
        if g.stop {
            return;
        }
        let now = Instant::now();
        let want = g.osd.shown.is_some() || g.other_panel;
        if want != g.sent_inhibit && !g.denied && g.live {
            g.sent_inhibit = want;
            drop(g);
            let _ = send(
                &shared,
                &client,
                "notifications.set_inhibited",
                json!({"inhibited": want}),
            );
            g = shared.lock();
            continue;
        }
        match g.osd.deadline {
            Some(d) if d <= now => {
                g.osd.deadline = None;
                g.osd.shown = None;
                drop(g);
                publish(&shared);
                g = shared.lock();
            }
            Some(d) => {
                g = shared
                    .wake
                    .wait_timeout(g, d - now)
                    .unwrap_or_else(|p| p.into_inner())
                    .0
            }
            None => g = shared.wake.wait(g).unwrap_or_else(|p| p.into_inner()),
        }
    }
}

fn poll(g: &mut Inner) {
    let mut done = vec![];
    g.calls
        .retain_mut(|(method, ticket)| match ticket.try_take() {
            None => true,
            Some(reply) => {
                done.push((method.clone(), reply));
                false
            }
        });
    for (method, reply) in done {
        let provider_only = matches!(
            method.as_str(),
            "notifications.set_hover" | "notifications.set_inhibited"
        );
        match reply {
            Ok(_) => {
                if !provider_only {
                    g.message.clear();
                }
            }
            Err(e) if provider_only && e.code == "permission_denied" => g.denied = true,
            Err(_) if provider_only => {}
            Err(e) if e.code == "stale_revision" => {
                g.message = "bead.common.changed-elsewhere".into();
            }
            Err(_) => g.message = "bead.common.request-failed".into(),
        }
    }
}
/// One published method call, only while the daemon advertises it at version 1.
fn send(
    shared: &Arc<Shared>,
    client: &Client,
    method: &str,
    params: Json,
) -> Result<(), &'static str> {
    let status = client.status();
    let advertised = status
        .methods
        .get(method)
        .filter(|m| m.v == 1)
        .ok_or("bead.notifications.unavailable")?;
    let boot = status
        .boot
        .clone()
        .ok_or("bead.notifications.unavailable")?;
    if !client.snapshot().connected {
        return Err("bead.notifications.unavailable");
    }
    let timeout = Duration::from_millis(u64::from(advertised.timeout).clamp(1, 10_000));
    let weak: Weak<Shared> = Arc::downgrade(shared);
    let wake: CallWake = Arc::new(move || {
        if let Some(shared) = weak.upgrade() {
            poll(&mut shared.lock());
            publish(&shared);
        }
    });
    let mut g = shared.lock();
    poll(&mut g);
    if g.calls.len() >= MAX_CALLS {
        return Err("Too many requests in flight");
    }
    let ticket = client
        .begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        )
        .map_err(|_| "bead.common.request-failed")?;
    g.calls.push((method.into(), ticket));
    Ok(())
}

pub struct Service {
    shared: Arc<Shared>,
    client: Client,
    action_on_click: bool,
    _subscriptions: Vec<Subscription>,
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
    }
}
struct Lease(u64, Weak<Shared>);
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(s) = self.1.upgrade() {
            let mut g = s.lock();
            g.wakeups.remove(&self.0);
            g.drivers.remove(&self.0);
        }
    }
}

impl Service {
    /// `client` is the shell's UI-provider daemon link, subscribed to [`TOPICS`].
    /// Hover pause and suppression are accepted by the daemon only from that provider.
    pub fn start(client: Client, options: Options) -> Result<Arc<Self>, String> {
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner::default()),
            wake: Condvar::new(),
        });
        shared.lock().osd.armed = Some(Instant::now() + OSD_ARM);
        SINKS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(Arc::downgrade(&shared));
        shared.lock().coexist = options.coexist;
        let mut subscriptions = vec![];
        type Handler = fn(&Shared, &Snapshot);
        for (topic, handler) in [
            ("notifications", on_notifications as Handler),
            // Subscribing starts the daemon's producers; the events arrive by `route_event`.
            ("osd", |_, _| {}),
        ] {
            let weak = Arc::downgrade(&shared);
            subscriptions.push(
                client
                    .topic(topic)
                    .and_then(|t| {
                        t.subscribe(Arc::new(move |snap| {
                            if let Some(shared) = weak.upgrade() {
                                handler(&shared, &snap);
                            }
                        }))
                    })
                    .map_err(|e| e.message)?,
            );
        }
        // Best effort: popups fall back to the leading output when focus is unknown.
        let weak = Arc::downgrade(&shared);
        if let Ok(subscription) = client.topic("compositor").and_then(|t| {
            t.subscribe(Arc::new(move |snap| {
                if let Some(shared) = weak.upgrade() {
                    on_compositor(&shared, &snap);
                }
            }))
        }) {
            subscriptions.push(subscription);
        }
        let (state, link) = (shared.clone(), client.clone());
        std::thread::Builder::new()
            .name("notice-service".into())
            .stack_size(256 * 1024)
            .spawn(move || worker(state, link))
            .map_err(|e| e.to_string())?;
        println!("NOTIFICATIONS_DAEMON consumer=true in_process=true");
        Ok(Arc::new(Self {
            shared,
            client,
            action_on_click: options.action_on_click,
            _subscriptions: subscriptions,
        }))
    }
    fn send(&self, method: &str, params: Json) -> Result<(), &'static str> {
        send(&self.shared, &self.client, method, params)
    }
}

impl Notices for Service {
    fn subscribe(&self, wakeup: Wakeup, _primary: bool) -> Handle {
        let id = {
            let mut g = self.shared.lock();
            g.next += 1;
            let id = g.next;
            g.wakeups.insert(id, wakeup);
            id
        };
        publish(&self.shared);
        Box::new(Lease(id, Arc::downgrade(&self.shared)))
    }
    fn drive(&self, wakeup: Wakeup) -> Handle {
        let id = {
            let mut g = self.shared.lock();
            g.next += 1;
            let id = g.next;
            g.drivers.insert(id, wakeup.clone());
            id
        };
        // A new bar starts from the current count.
        wakeup(UiEvent::Model(bar_count(
            self.shared.lock().bar_count.unwrap_or(0),
        )));
        publish(&self.shared);
        Box::new(Lease(id, Arc::downgrade(&self.shared)))
    }
    fn panel_changed(&self, view: Option<&str>) {
        {
            let mut g = self.shared.lock();
            let other = view.is_some_and(|v| suppresses(&g.coexist, v));
            if g.other_panel == other {
                return;
            }
            g.other_panel = other;
        }
        self.shared.wake.notify_all();
        publish(&self.shared);
    }
    fn hover(&self, key: &str, on: bool) {
        if key == "osd" {
            let mut g = self.shared.lock();
            g.osd.hovered = on;
            // Hover holds the OSD open; leaving restarts the full hide delay.
            g.osd.deadline = (!on && g.osd.shown.is_some()).then(|| Instant::now() + OSD_HIDE);
            drop(g);
            self.shared.wake.notify_all();
            return;
        }
        let Some((id, rev)) = key
            .split_once('|')
            .and_then(|(id, rev)| Some((id.to_owned(), rev.parse::<u64>().ok()?)))
        else {
            return;
        };
        let call = {
            let mut g = self.shared.lock();
            if g.denied || !g.live {
                return;
            }
            let current = g.hover.clone();
            if on {
                let known = g.good.as_ref().is_some_and(|s| {
                    s.active
                        .items
                        .iter()
                        .any(|(k, n)| k.0 == id && n.revision == Some(rev))
                });
                if !known || current.as_ref() == Some(&(id.clone(), rev)) {
                    return;
                }
                g.hover = Some((id.clone(), rev));
            } else if current.as_ref() == Some(&(id.clone(), rev)) {
                g.hover = None;
            } else {
                return;
            }
            current
        };
        if on && let Some((old, old_rev)) = call {
            let _ = self.send(
                "notifications.set_hover",
                json!({"id": old, "revision": old_rev, "hovered": false}),
            );
        }
        let _ = self.send(
            "notifications.set_hover",
            json!({"id": id, "revision": rev, "hovered": on}),
        );
    }
    fn action(&self, name: &str, arg: &Value) -> Result<(), String> {
        if name == "notifications.osd" {
            return Ok(());
        }
        let planned = {
            let g = self.shared.lock();
            if !g.live {
                return Err(tr("bead.notifications.unavailable", &[]));
            }
            let state = g
                .good
                .as_ref()
                .ok_or_else(|| tr("bead.notifications.unavailable", &[]))?;
            plan(name, arg, state, self.action_on_click)
        };
        match planned {
            Ok(Plan::Toggle(id)) => {
                let mut g = self.shared.lock();
                if !g.expanded.remove(&id) {
                    g.expanded.insert(id);
                }
            }
            Ok(Plan::Call(method, params)) => {
                if let Err(message) = self.send(method, params) {
                    self.shared.lock().message = message.into();
                }
            }
            Err(message) => self.shared.lock().message = message.into(),
        }
        publish(&self.shared);
        Ok(())
    }
    fn audio_changed(&self, _: &bead::services::Audio) {}
    /// Normal feedback comes from daemon `osd` events (N0043); isolated previews use `preview`.
    fn feedback(&self, _: &[Json]) -> Result<Json, &'static str> {
        Err("the OSD shows daemon osd events only")
    }
    fn preview(&self, args: &[Json]) -> Result<Json, &'static str> {
        if !(2..=5).contains(&args.len()) {
            return Err("OSD preview needs kind, value, optional muted, label and device");
        }
        let kind = match args[0].as_str() {
            Some("volume") => OsdKind::Volume,
            Some("output") => OsdKind::Output,
            Some("microphone") => OsdKind::Microphone,
            Some("brightness") => OsdKind::Brightness,
            Some("keyboard") => OsdKind::Keyboard,
            Some("night_light") => OsdKind::NightLight,
            Some("power_profile") => OsdKind::PowerProfile,
            Some("capslock") => OsdKind::Capslock,
            _ => return Err("unknown OSD preview kind"),
        };
        let value = args[1]
            .as_f64()
            .filter(|n| n.is_finite() && (0. ..=1.).contains(n))
            .ok_or("OSD preview value must be within 0..1")?;
        let muted = args
            .get(2)
            .map_or(Some(false), Json::as_bool)
            .ok_or("OSD preview muted must be boolean")?;
        let label = args
            .get(3)
            .map_or(Some(""), Json::as_str)
            .ok_or("OSD preview label must be text")?;
        let device = args
            .get(4)
            .map_or(Some(""), Json::as_str)
            .ok_or("OSD preview device must be text")?;
        {
            let mut g = self.shared.lock();
            g.osd.shown = Some(Shown {
                kind,
                value,
                muted,
                label: label.into(),
                device: device.into(),
            });
            g.osd.deadline = (!g.osd.hovered).then(|| Instant::now() + OSD_HIDE);
        }
        self.shared.wake.notify_all();
        publish(&self.shared);
        Ok(Json::Null)
    }
    fn command(&self, call: &bead::services::ipc::Call) -> Result<Json, &'static str> {
        match call.method.as_str() {
            "clear" => self
                .action("notifications.clear_popups", &Value::Bool(true))
                .map(|_| Json::Null)
                .map_err(|_| "notifications unavailable"),
            "clearHistory" => self
                .action("notifications.clear", &Value::Bool(true))
                .map(|_| Json::Null)
                .map_err(|_| "notifications unavailable"),
            _ => Err("unknown notification command"),
        }
    }
}

#[cfg(test)]
#[path = "notice_service_tests.rs"]
mod tests;
