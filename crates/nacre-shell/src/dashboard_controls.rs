// SPDX-License-Identifier: GPL-3.0-only
//! B0027: the daemon-hosted extras (Focus, Clipboard, Screenshots, AI usage,
//! Brain), tab customization and their calls. Every control is enabled only
//! while the connected daemon advertises the exact method at version 1; a
//! call is never retried and the topic stays the only source of truth.
use super::*;
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use nacre_protocol::core_services::{DashboardSystemState, EntityId, Player, PlayerStatus};
use std::{cell::RefCell, time::Duration};

#[derive(Default)]
struct State {
    pending: Option<CallTicket>,
    message: String,
    confirm: bool,
}
pub struct Controls {
    client: Client,
    state: RefCell<State>,
}
impl Controls {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Default::default(),
        }
    }
    /// Connected daemon, known boot and the exact method at version 1.
    fn ready(&self, method: &str) -> Option<(String, Duration)> {
        let status = self.client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !self.client.snapshot().connected {
            return None;
        }
        let timeout = Duration::from_millis(u64::from(advertised.timeout).clamp(1, 10_000));
        Some((status.boot?, timeout))
    }
    fn submit(&self, method: &str, params: Json, wake: CallWake) {
        let mut state = self.state.borrow_mut();
        if state.pending.is_some() {
            return;
        }
        state.message.clear();
        state.confirm = false;
        let Some((boot, timeout)) = self.ready(method) else {
            state.message = "bead.dashboard.unavailable-now".into();
            return;
        };
        match self.client.begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        ) {
            Ok(ticket) => state.pending = Some(ticket),
            Err(_) => state.message = "bead.common.request-failed".into(),
        }
    }
    fn say(&self, message: &str) {
        self.state.borrow_mut().message = message.into();
    }
    /// Polls a finished call; the daemon's own message is never shown.
    fn poll(&self) -> (bool, String, bool) {
        let mut state = self.state.borrow_mut();
        if let Some(ticket) = &mut state.pending
            && let Some(reply) = ticket.try_take()
        {
            state.pending = None;
            state.message = match reply {
                Ok(_) => String::new(),
                Err(e) if e.code == "stale_revision" => "bead.common.changed-elsewhere".into(),
                Err(_) => "bead.common.request-failed".into(),
            };
        }
        (
            state.pending.is_some(),
            if state.message.is_empty() {
                String::new()
            } else {
                crate::i18n::tr(&state.message, &[])
            },
            state.confirm,
        )
    }
}

impl Dashboard {
    /// One host action `dashboard.do`, argument `op[:id]`. Row ids are re-resolved
    /// against the current topic; callers never construct them.
    pub fn act(&self, arg: &str, wake: CallWake) {
        let Some(c) = &self.controls else { return };
        let (op, id) = arg.split_once(':').unwrap_or((arg, ""));
        let page = self.page;
        match (page, op) {
            (Page::Focus, "focus.start") => {
                if let Ok(minutes @ 1..=180) = id.parse::<u32>() {
                    c.submit(op, json!({"minutes": minutes}), wake);
                }
            }
            (Page::Focus, "focus.pause" | "focus.resume" | "focus.stop") => {
                c.submit(op, json!({}), wake)
            }
            (Page::Clipboard, "clip.copy") if self.holds(id) => {
                c.submit("clipboard.copy", json!({"id": id}), wake)
            }
            (Page::Clipboard, "clip.ask") if c.ready("clipboard.clear").is_some() => {
                c.state.borrow_mut().confirm = true
            }
            (Page::Clipboard, "clip.cancel") => c.state.borrow_mut().confirm = false,
            (Page::Clipboard, "clip.clear") if c.state.borrow().confirm => {
                c.submit("clipboard.clear", json!({}), wake)
            }
            (Page::Screenshots, "shot.open") if self.holds(id) => {
                c.submit("screenshots.open", json!({"id": id}), wake)
            }
            (
                Page::Media | Page::Today,
                "player.play_pause" | "player.next" | "player.previous",
            ) => {
                // The topic's own `can_*` flags gate each control.
                if let Some((id, p)) = self.active_player()
                    && match op {
                        "player.next" => p.can_next,
                        "player.previous" => p.can_previous,
                        _ => p.can_play_pause,
                    }
                {
                    c.submit(op, json!({"player": id.0}), wake)
                }
            }
            (Page::Media, "player.seek") => {
                // Relative jump from the extrapolated position, clamped to the track.
                let Ok(step @ -60..=60) = id.parse::<i64>() else {
                    return;
                };
                if let Some((player, p)) = self.active_player()
                    && p.can_seek
                    && let Some(at) = self.position(&p)
                {
                    let target =
                        (at as i64 + step * 1000).clamp(0, p.length_ms.unwrap_or(0) as i64);
                    c.submit(op, json!({"player": player.0, "position_ms": target}), wake)
                }
            }
            (
                Page::Notifications,
                "notif.dismiss" | "notif.forget" | "notif.dnd" | "notif.clear",
            ) => self.notice_call(op, id, wake),
            (Page::AiUsage, "usage.refresh") => c.submit("ai.usage.refresh", json!({}), wake),
            (Page::Customize, "tab.toggle" | "tab.up" | "tab.down") => self.customize(op, id, wake),
            _ => {}
        }
    }
    /// Notification controls go through the same revision-checked plan as the
    /// resident stack; the snapshot on screen decides what may be sent.
    fn notice_call(&self, op: &str, id: &str, wake: CallWake) {
        let Some(c) = &self.controls else { return };
        let Ok(CoreSnapshot::Notifications(state)) =
            checked("notifications", &self.source.snapshot().data)
        else {
            return;
        };
        let (name, arg) = match op {
            "notif.dnd" => ("notifications.dnd", "toggle"),
            "notif.clear" => ("notifications.clear", ""),
            "notif.forget" => ("notifications.forget", id),
            _ => ("notifications.dismiss", id),
        };
        match crate::notice_service::plan(name, &Value::Text(arg.into()), &state, false) {
            Ok(crate::notice_service::Plan::Call(method, params)) => c.submit(method, params, wake),
            Ok(_) => {}
            Err(message) => c.say(message),
        }
    }
    /// The daemon's current player and its state, if the topic is fresh and valid.
    fn active_player(&self) -> Option<(EntityId, Player)> {
        let source = if self.page == Page::Media {
            self.secondary.as_ref()
        } else {
            self.player.as_ref()
        };
        let snapshot = source?.snapshot();
        let Ok(CoreSnapshot::Player(s)) = checked("dashboard.player", &snapshot.data) else {
            return None;
        };
        let id = s.active?;
        let player = s.players.items.get(&id)?.clone();
        (s.health.available && snapshot.connected).then_some((id, player))
    }
    fn holds(&self, id: &str) -> bool {
        let snapshot = self.source.snapshot();
        match extra(&self.page.topic(), &snapshot.data) {
            Some(Extra::Clipboard(s)) => s.entries.items.keys().any(|k| k.0 == id),
            Some(Extra::Screenshots(s)) => s.items.items.keys().any(|k| k.0 == id),
            _ => false,
        }
    }
    fn store(&self) -> Option<(SettingsStore, bool)> {
        let source = if self.page == Page::Customize {
            Some(&self.source)
        } else {
            self.settings.as_ref()
        }?;
        let snapshot = source.snapshot();
        let store: SettingsStore = serde_json::from_value((*snapshot.data).clone()).ok()?;
        Some((store, snapshot.connected))
    }
    /// Enabled pages and the full order, both limited to the known registry.
    fn layout(store: Option<&SettingsStore>) -> (Vec<Page>, Vec<Page>) {
        let read = |key: &str| -> Vec<Page> {
            let mut out: Vec<Page> = Vec::new();
            let names = store
                .and_then(|s| s.data.get(key))
                .and_then(Json::as_array)
                .into_iter()
                .flatten();
            for page in names
                .filter_map(Json::as_str)
                .filter_map(|n| Page::REGISTRY.into_iter().find(|p| p.name() == n))
            {
                if !out.contains(&page) {
                    out.push(page);
                }
            }
            out
        };
        let mut order = read("order");
        let rest: Vec<Page> = Page::REGISTRY
            .into_iter()
            .filter(|p| !order.contains(p))
            .collect();
        order.extend(rest);
        let enabled = read("enabled");
        (enabled, order)
    }
    fn customize(&self, op: &str, id: &str, wake: CallWake) {
        let (Some(c), Some((store, connected))) = (&self.controls, self.store()) else {
            return;
        };
        if !connected || store.error.is_some() {
            c.say("bead.dashboard.settings-unavailable");
            return;
        }
        let Some(page) = Page::REGISTRY.into_iter().find(|p| p.name() == id) else {
            return;
        };
        let (enabled, mut order) = Self::layout(Some(&store));
        let (key, names): (&str, Vec<Page>) = if op == "tab.toggle" {
            let mut on = enabled;
            match on.iter().position(|p| *p == page) {
                Some(at) => {
                    on.remove(at);
                }
                None => on.push(page),
            }
            (
                "enabled",
                order.into_iter().filter(|p| on.contains(p)).collect(),
            )
        } else {
            let at = order.iter().position(|p| *p == page).unwrap_or(0);
            let to = if op == "tab.up" {
                at.saturating_sub(1)
            } else {
                (at + 1).min(order.len() - 1)
            };
            if to == at {
                return;
            }
            order.swap(at, to);
            ("order", order)
        };
        let value: Vec<&str> = names.iter().map(|p| p.name()).collect();
        // The revision fence: a stale revision is refused and nothing is replayed.
        c.submit(
            "settings.set",
            json!({"store":"dashboard-extras","key":[key],"value":value,"rev":store.rev}),
            wake,
        );
    }
    fn offered(&self, page: Page) -> bool {
        match &self.client {
            Some(client) => client.status().topics.get(&page.topic()) == Some(&1),
            None => self.preview,
        }
    }
    fn tabs(&self) -> Vec<Json> {
        let store = self.store();
        let (enabled, order) = Self::layout(store.as_ref().map(|(s, _)| s));
        let mut pages = Page::ALL.to_vec();
        pages.extend(
            order
                .into_iter()
                .filter(|p| enabled.contains(p) && self.offered(*p)),
        );
        if !pages.contains(&self.page) {
            pages.push(self.page);
        }
        pages
            .into_iter()
            .map(|p| json!({"id":p.name(),"title":p.title(),"icon":p.icon()}))
            .collect()
    }
    /// Stores shared by every page: navigation and call state.
    pub(super) fn chrome(&self, out: &mut BTreeMap<String, Value>) {
        let (busy, message, confirm) = self
            .controls
            .as_ref()
            .map_or_else(Default::default, |c| c.poll());
        let method = match self.page {
            Page::Focus => "focus.start",
            Page::Clipboard => "clipboard.copy",
            Page::Screenshots => "screenshots.open",
            Page::AiUsage => "ai.usage.refresh",
            Page::Customize => "settings.set",
            Page::Media | Page::Today => "player.play_pause",
            Page::Notifications => "notifications.set_dnd",
            _ => "",
        };
        let can = |m: &str| self.controls.as_ref().is_some_and(|c| c.ready(m).is_some());
        let customize = self.page != Page::Customize
            && can("settings.set")
            && self.store().is_some()
            && self.offered(Page::Customize);
        out.insert("dashboard.tabs".into(), Value::list(self.tabs()));
        out.insert(
            "dashboard.control".into(),
            Value::Bool(!busy && can(method)),
        );
        out.insert("dashboard.busy".into(), Value::Bool(busy));
        out.insert("dashboard.message".into(), Value::Text(line(&message, 64)));
        out.insert("dashboard.confirm".into(), Value::Bool(confirm));
        out.insert("dashboard.customize".into(), Value::Bool(customize));
        if matches!(self.page, Page::Media | Page::Today) {
            for (key, value) in [
                ("track", "bead.dashboard.playback-unavailable"),
                ("track_detail", "bead.dashboard.playback-unavailable-detail"),
            ] {
                out.entry(format!("dashboard.{key}"))
                    .or_insert_with(|| Value::Text(tr(value, &[])));
            }
            for key in ["can_previous", "can_toggle", "can_next", "can_seek", "art"] {
                out.entry(format!("dashboard.{key}"))
                    .or_insert(Value::Bool(false));
            }
        }
        out.entry("dashboard.state".into())
            .or_insert(Value::Text("unavailable".into()));
        out.entry("dashboard.progress".into())
            .or_insert(Value::Number(0.));
    }
    /// Countdown wake: one thread per visible running session, none otherwise.
    pub(super) fn tick(&self, live: bool) {
        let mut ticker = self.ticker.borrow_mut();
        if !live {
            *ticker = None;
            return;
        }
        let Some(notify) = self.notify.borrow().clone().filter(|_| ticker.is_none()) else {
            return;
        };
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new()
            .name("focus-tick".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(Duration::from_secs(1))
                {
                    notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
                }
            });
        if spawned.is_ok() {
            *ticker = Some(Ticker { _stop: stop });
        }
    }
    /// Media playback from `dashboard.player`; the lyrics stay on `dashboard.media`.
    /// Position is extrapolated locally while playing, never per daemon frame.
    pub(super) fn project_player(&self, out: &mut BTreeMap<String, Value>) {
        let Some((_, p)) = self.active_player() else {
            return;
        };
        let clock = |ms: u64| format!("{}:{:02}", ms / 60_000, ms / 1000 % 60);
        let position = self.position(&p);
        let state = match p.status {
            PlayerStatus::Playing => "playing",
            PlayerStatus::Paused => "paused",
            PlayerStatus::Stopped => "stopped",
        };
        text(out, "state", state, 12);
        text(
            out,
            "track",
            &if p.title.is_empty() {
                tr("bead.dashboard.media-unknown-track", &[])
            } else {
                p.title.clone()
            },
            64,
        );
        let who = [p.artist.as_str(), p.album.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        text(
            out,
            "track_detail",
            &if who.is_empty() {
                p.identity.clone()
            } else {
                who
            },
            96,
        );
        text(
            out,
            "metric",
            &match (position, p.length_ms) {
                (Some(at), Some(len)) => format!("{} / {}", clock(at), clock(len)),
                (Some(at), None) => clock(at),
                _ => String::new(),
            },
            16,
        );
        let progress = position
            .zip(p.length_ms)
            .map_or(0., |(at, len)| at as f64 / len as f64);
        out.insert("dashboard.progress".into(), Value::Number(progress));
        out.insert("dashboard.can_previous".into(), Value::Bool(p.can_previous));
        out.insert("dashboard.can_toggle".into(), Value::Bool(p.can_play_pause));
        out.insert("dashboard.can_next".into(), Value::Bool(p.can_next));
        let seek = p.can_seek
            && self
                .controls
                .as_ref()
                .is_some_and(|c| c.ready("player.seek").is_some());
        out.insert("dashboard.can_seek".into(), Value::Bool(seek));
        out.insert(
            "dashboard.art".into(),
            Value::Bool(self.art_for(p.art_path.as_deref())),
        );
    }
    /// Reading at `position_updated_ms` plus the time since, only while playing.
    fn position(&self, p: &Player) -> Option<u64> {
        let moved = if p.status == PlayerStatus::Playing {
            (self.now)().saturating_sub(p.position_updated_ms.unwrap_or(0))
        } else {
            0
        };
        p.position_ms
            .map(|at| (at + moved).min(p.length_ms.unwrap_or(u64::MAX)))
    }
    /// True when a thumbnail for `path` is held; decodes once per path change and
    /// remembers a failure so a bad file is not re-read on every projection.
    fn art_for(&self, path: Option<&str>) -> bool {
        let mut art = self.art.borrow_mut();
        let Some(path) = path else {
            *art = None;
            return false;
        };
        if art.as_ref().is_none_or(|(held, _)| held != path) {
            *art = Some((path.into(), crate::player_art::thumbnail(path)));
        }
        art.as_ref().is_some_and(|(_, thumb)| thumb.is_some())
    }
    /// Gives the card's `art` image its pixels: the placeholder icon name in the
    /// scene is swapped for the held thumbnail, which the scene then owns.
    pub fn attach_art(&self, scene: &mut bead::render::Scene) {
        let held = self.art.borrow();
        let thumb = held.as_ref().and_then(|(_, t)| t.clone());
        for draw in &mut scene.primitives {
            if let bead::render::Primitive::Image { path, .. } = draw
                && path.to_str() == Some("@icon:player-art")
                && let Some(thumb) = &thumb
            {
                let key = std::path::PathBuf::from("@player-art/thumb.png");
                scene.asset_data.insert(key.clone(), thumb.clone());
                *path = key;
            }
        }
    }
    pub(super) fn project_more(&self, out: &mut BTreeMap<String, Value>, snapshot: &Snapshot) {
        if self.page == Page::Customize {
            let Some((store, connected)) = self.store() else {
                return;
            };
            out.insert("dashboard.ready".into(), Value::Bool(true));
            out.insert(
                "dashboard.stale".into(),
                Value::Bool(!connected || store.error.is_some()),
            );
            text(
                out,
                "heading",
                &tr("bead.dashboard.customize-heading", &[]),
                48,
            );
            text(
                out,
                "summary",
                &tr("bead.dashboard.customize-hint", &[]),
                64,
            );
            let (enabled, order) = Self::layout(Some(&store));
            let rows = order.iter().map(|p| {
                let name = p.name();
                json!({"id":name,"label":p.title(),"value":tr(if enabled.contains(p) {"bead.common.on"} else {"bead.common.off"}, &[]),
                    "detail":"","toggle":format!("tab.toggle:{name}"),
                    "up":format!("tab.up:{name}"),"down":format!("tab.down:{name}")})
            });
            out.insert("dashboard.rows".into(), Value::list(rows.collect()));
            return;
        }
        let Some(data) = extra(&self.page.topic(), &snapshot.data) else {
            return;
        };
        let health = match &data {
            Extra::Focus(s) => &s.health,
            Extra::Clipboard(s) => &s.health,
            Extra::Screenshots(s) => &s.health,
            Extra::AiUsage(s) => &s.health,
            Extra::BrainRecent(s) => &s.health,
        };
        out.insert("dashboard.ready".into(), Value::Bool(health.available));
        out.insert(
            "dashboard.stale".into(),
            Value::Bool(health.stale || !snapshot.connected),
        );
        if !health.available {
            return;
        }
        let mut rows: Vec<Json> = Vec::new();
        let mut secondary: Vec<Json> = Vec::new();
        match data {
            Extra::Focus(s) => {
                let remaining = match s.status {
                    FocusStatus::Running => s
                        .deadline_ms
                        .map(|d| d.saturating_sub((self.now)()).div_ceil(1000).min(10_800) as u32),
                    FocusStatus::Paused => s.remaining_s,
                    _ => None,
                };
                let (state, heading) = match s.status {
                    FocusStatus::Idle => ("idle", "bead.dashboard.focus-idle"),
                    FocusStatus::Running => ("running", "bead.dashboard.focus-running"),
                    FocusStatus::Paused => ("paused", "bead.dashboard.focus-paused"),
                    FocusStatus::Done => ("done", "bead.dashboard.focus-done"),
                };
                text(out, "state", state, 12);
                text(out, "heading", &tr(heading, &[]), 32);
                text(
                    out,
                    "metric",
                    &remaining.map_or("—".into(), |r| format!("{:02}:{:02}", r / 60, r % 60)),
                    8,
                );
                text(
                    out,
                    "summary",
                    &if s.owns_dnd {
                        tr("bead.dashboard.focus-dnd", &[])
                    } else {
                        String::new()
                    },
                    64,
                );
                let progress = match remaining {
                    Some(r) if s.total_s > 0 => {
                        1. - f64::from(r.min(s.total_s)) / f64::from(s.total_s)
                    }
                    _ => 0.,
                };
                out.insert("dashboard.progress".into(), Value::Number(progress));
            }
            Extra::Clipboard(s) => {
                text(
                    out,
                    "heading",
                    &tr("bead.dashboard.clipboard-heading", &[]),
                    48,
                );
                text(
                    out,
                    "detail",
                    &if s.entries.order.is_empty() {
                        tr("bead.dashboard.clipboard-empty", &[])
                    } else {
                        String::new()
                    },
                    48,
                );
                rows = s.entries.order.iter().take(7).filter_map(|id| s.entries.items.get(id).map(|e| {
                    json!({"id":id.0,"label":line(&e.preview,72),
                        "value":tr(if e.kind == nacre_protocol::dashboard_extras::ClipKind::Image {"bead.dashboard.clipboard-image"} else {"bead.dashboard.clipboard-text"}, &[]),
                        "detail":"","act":format!("clip.copy:{}", id.0)})
                })).collect();
            }
            Extra::Screenshots(s) => {
                text(
                    out,
                    "heading",
                    &tr("bead.dashboard.screenshots-heading", &[]),
                    48,
                );
                text(
                    out,
                    "detail",
                    &if s.items.order.is_empty() {
                        tr("bead.dashboard.screenshots-empty", &[])
                    } else {
                        String::new()
                    },
                    64,
                );
                // Only the file name is shown; the absolute path never reaches the view.
                rows = s.items.order.iter().take(6).filter_map(|id| s.items.items.get(id).map(|e| {
                    let name = e.path.rsplit('/').next().unwrap_or("");
                    json!({"id":id.0,"label":line(name,56),"value":tr("bead.dashboard.screenshots-open", &[]),"detail":"","act":format!("shot.open:{}", id.0)})
                })).collect();
            }
            Extra::AiUsage(s) => {
                text(out, "heading", &Page::AiUsage.title(), 48);
                text(
                    out,
                    "detail",
                    &if s.providers.order.is_empty() {
                        tr("bead.dashboard.ai-usage-empty", &[])
                    } else {
                        String::new()
                    },
                    48,
                );
                for id in &s.providers.order {
                    let Some(p) = s.providers.items.get(id) else {
                        continue;
                    };
                    rows.push(json!({"id":id.0,"label":line(&p.name,32),"value":"","detail":""}));
                    for (n, l) in p.lines.iter().enumerate() {
                        rows.push(json!({"id":format!("{}-{n}", id.0),"label":line(l,96),"value":"","detail":""}));
                    }
                }
                rows.truncate(14);
            }
            Extra::BrainRecent(s) => {
                text(out, "heading", &tr("bead.dashboard.brain-heading", &[]), 48);
                text(
                    out,
                    "detail",
                    &if s.notes.order.is_empty() {
                        tr("bead.dashboard.brain-empty", &[])
                    } else {
                        String::new()
                    },
                    48,
                );
                rows = s
                    .notes
                    .order
                    .iter()
                    .filter_map(|id| {
                        s.notes.items.get(id).map(
                            |n| json!({"id":id.0,"label":line(&n.title,56),"value":"","detail":""}),
                        )
                    })
                    .collect();
                secondary = s
                    .tasks
                    .order
                    .iter()
                    .filter_map(|id| {
                        s.tasks.items.get(id).map(
                            |t| json!({"id":id.0,"label":line(&t.text,72),"value":"","detail":""}),
                        )
                    })
                    .collect();
            }
        }
        out.insert("dashboard.rows".into(), Value::list(rows));
        out.insert("dashboard.secondary".into(), Value::list(secondary));
    }
}

/// CPU, memory, GPU and drive cards. Unknown readings stay "—", never zero.
pub(super) fn gauges(s: &DashboardSystemState) -> Vec<Json> {
    let percent = |v: Option<f64>| {
        v.filter(|v| v.is_finite())
            .map_or("—".into(), |v| format!("{v:.0}%"))
    };
    let unavailable = || tr("bead.dashboard.gauge-unavailable", &[]);
    let mut gpu_detail = unavailable();
    if let Some((used, total)) = s
        .gpu
        .as_ref()
        .and_then(|g| g.memory_used.zip(g.memory_total))
    {
        gpu_detail = format!("{} / {}", bytes(used), bytes(total));
    }
    let memory = s.memory.as_ref();
    vec![
        json!({"id":"cpu","label":tr("bead.dashboard.gauge-cpu", &[]),"icon":"developer_board","value":percent(s.cpu_percent),
            "detail":if s.cpu_percent.is_some() {tr("bead.dashboard.gauge-cpu-detail", &[])} else {unavailable()}}),
        json!({"id":"memory","label":tr("bead.dashboard.gauge-memory", &[]),"icon":"memory",
            "value":percent(memory.map(|m| m.used as f64 * 100. / m.total as f64)),
            "detail":memory.map_or_else(unavailable, |m| format!("{} / {}", bytes(m.used), bytes(m.total)))}),
        json!({"id":"gpu","label":tr("bead.dashboard.gauge-gpu", &[]),"icon":"view_in_ar","value":percent(s.gpu.as_ref().and_then(|g| g.percent)),
            "detail":gpu_detail}),
    ]
}

/// Down/up throughput bars and rates from `dashboard.system.network` (bytes per
/// second, oldest first). Bar heights share one scale so the two lines compare.
pub(super) fn traffic(s: &DashboardSystemState, out: &mut BTreeMap<String, Value>) {
    const BAR: u64 = 40;
    let rate = |v: u64| {
        if v >= 1024 * 1024 {
            format!("{:.1} MiB/s", v as f64 / (1024. * 1024.))
        } else {
            format!("{:.0} KiB/s", v as f64 / 1024.)
        }
    };
    let peak = s
        .network
        .iter()
        .map(|n| n.down_bps.max(n.up_bps))
        .max()
        .unwrap_or(0);
    let scale = peak.max(1024) as u128;
    let bars = |pick: fn(&nacre_protocol::core_services::NetworkSample) -> u64| {
        s.network
            .iter()
            .enumerate()
            .map(|(i, n)| json!({"id": i.to_string(), "h": (u128::from(BAR) * u128::from(pick(n)) / scale).max(2) as u64}))
            .collect::<Vec<_>>()
    };
    let last = s.network.last();
    out.insert(
        "dashboard.net_down".into(),
        Value::list(bars(|n| n.down_bps)),
    );
    out.insert("dashboard.net_up".into(), Value::list(bars(|n| n.up_bps)));
    for (key, text) in [
        (
            "net_rate",
            last.map_or("—".into(), |n| {
                tr(
                    "bead.dashboard.network-down",
                    &[("rate", &rate(n.down_bps))],
                )
            }),
        ),
        (
            "net_up_rate",
            last.map_or("—".into(), |n| {
                tr("bead.dashboard.network-up", &[("rate", &rate(n.up_bps))])
            }),
        ),
        (
            "net_peak",
            if last.is_some() {
                tr("bead.dashboard.network-peak", &[("rate", &rate(peak))])
            } else {
                String::new()
            },
        ),
    ] {
        out.insert(format!("dashboard.{key}"), Value::Text(text));
    }
}
