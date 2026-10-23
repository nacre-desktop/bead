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
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
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

/// Topics the shared daemon link must subscribe for this service.
pub const TOPICS: [&str; 2] = ["notifications", "osd"];
/// Panel views the service offers (see `UiEvent::CompanionOffer`).
pub const VIEW_NOTICES: &str = "notices";
pub const VIEW_OSD: &str = "osd";

pub struct Options {
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
    json!({"id": id, "rev": rev, "key": format!("{id}|{rev}"),
        "app": n.app_name, "summary": n.summary, "body": n.body,
        "icon": n.icon.clone().unwrap_or_default(), "urgency": urgency(n),
        "expanded": expanded, "actions": actions, "default": default,
        "time_str": time_str(now.saturating_sub(n.created_ms)), "history": history})
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
    next: u64,
    /// Another view (launcher, dashboard, popout) is shown: popups are suppressed.
    other_panel: bool,
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
    let (values, offer, wakeups, driver) = {
        let mut g = shared.lock();
        let want = wanted(&g);
        let close = if want.is_none() {
            g.offered.take()
        } else {
            None
        };
        g.offered = want.or(g.offered);
        (
            models(&g),
            want.map(|v| (v, true)).or(close.map(|v| (v, false))),
            g.wakeups.values().cloned().collect::<Vec<_>>(),
            g.drivers.values().next().cloned(),
        )
    };
    for wakeup in wakeups {
        wakeup(UiEvent::Model(values.clone()));
    }
    if let (Some(driver), Some((view, open))) = (driver, offer) {
        driver(UiEvent::CompanionOffer {
            view: view.into(),
            output: None,
            open,
        });
    }
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
            g.drivers.insert(id, wakeup);
            id
        };
        publish(&self.shared);
        Box::new(Lease(id, Arc::downgrade(&self.shared)))
    }
    fn panel_changed(&self, view: Option<&str>) {
        let other = view.is_some_and(|v| v != VIEW_NOTICES && v != VIEW_OSD);
        {
            let mut g = self.shared.lock();
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
    /// The OSD is driven by the daemon's `osd` events alone (N0043); nothing else may show it.
    fn feedback(&self, _: &[Json]) -> Result<Json, &'static str> {
        Err("the OSD shows daemon osd events only")
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
