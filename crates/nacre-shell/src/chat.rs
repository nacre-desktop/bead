// SPDX-License-Identifier: GPL-3.0-only
//! B0043: the AI sidebar as a Bead panel over the published `ai.chat` topic and its
//! methods (N0032). The daemon owns the provider workers; this module only reads
//! the bounded snapshot of the edge channel (`""`), turns it into windowed cells
//! for the Luau view, and sends the published calls. Closed, it holds nothing:
//! the topic lease, the layout caches and the heartbeat belong to the open panel.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use bead::ui::{
    markdown,
    transcript::{Entry, Meter, Scale, Style, Transcript},
};
use nacre_protocol::ai_chat::{AiChannel, Provider, Role};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Default panel width; callers may pass another to `Chat::new`.
pub const WIDTH: f64 = 470.;
const SANS: &str = "IBM Plex Sans";
const MONO: &str = "JetBrainsMono Nerd Font";
const HEARTBEAT: Duration = Duration::from_secs(10);
const SAVED_REFRESH: Duration = Duration::from_secs(60);
const OPEN: &str = "ai.chat.open";
const SAVED: &str = "ai.chat.saved";
const MAX_QUESTIONS: usize = 4;

type Notify = Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>;

// ---- geometry ----------------------------------------------------------------

/// Panel geometry as multiples of the shared size scale; nothing here names a pixel.
struct Metrics {
    edge: f64,
    gap: f64,
    button: f64,
    tabs: f64,
    top: f64,
    tools: f64,
    inner: f32,
    body: f32,
    info: f32,
    composer_min: f64,
    composer_max: f64,
    question_min: f64,
    question_max: f64,
}
impl Metrics {
    fn new(width: f64, scale: &Scale) -> Self {
        let (xs, sm, md) = (
            f64::from(scale.xs),
            f64::from(scale.sm),
            f64::from(scale.md),
        );
        let edge = md + xs;
        let button = 2. * md + 2.5 * xs;
        let tabs = edge * 3.875;
        let top = tabs + button + md + 0.5 * xs;
        Self {
            edge,
            gap: sm,
            button,
            tabs,
            top,
            tools: button + sm - 1. + f64::from(scale.caption) * 1.4,
            inner: (width - 2. * edge) as f32,
            body: scale.body,
            info: scale.caption,
            composer_min: 25. * xs,
            composer_max: 40. * xs,
            question_min: 20. * xs,
            question_max: 60. * xs,
        }
    }
}

// ---- state -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Open,
    Send,
    Plain,
    Saved,
}
struct Pending {
    call: Call,
    ticket: CallTicket,
}
struct Parsed {
    key: (String, u64),
    available: bool,
    channel: Option<Arc<AiChannel>>,
}
#[derive(Default)]
struct St {
    pending: Vec<Pending>,
    opened_at: Option<Instant>,
    message: String,
    follow: bool,
    written: f64,
    viewport: (f32, f32),
    parsed: Option<Parsed>,
    thread: String,
    question: String,
    saved: Option<(Instant, Json)>,
    saved_asked: Option<Instant>,
}
struct Ticker {
    _stop: std::sync::mpsc::Sender<()>,
}
pub struct Chat {
    source: Arc<dyn Source>,
    client: Option<Client>,
    transcript: RefCell<Transcript>,
    metrics: Metrics,
    st: RefCell<St>,
    notify: RefCell<Option<Notify>>,
    ticker: RefCell<Option<Ticker>>,
    heartbeat: Duration,
    /// Message text by ordinal for the copy actions of the projected transcript.
    texts: RefCell<BTreeMap<String, String>>,
    memory: Option<crate::popouts::ChatMemory>,
}

/// What a view action asks the host to do besides updating stores.
#[derive(Default)]
pub struct Outcome {
    pub stores: BTreeMap<String, Value>,
    pub copy: Option<String>,
}

// The daemon publishes `send_failed` as an event, not a patch. The shell's one
// daemon link hands every event to `route_event`; the open panel drains the queue.
static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WAKER: Mutex<Option<Notify>> = Mutex::new(None);
pub fn route_event(message: &nacre_protocol::Message) {
    use nacre_protocol::{Message, ai_chat};
    let Message::Ev {
        topic, name, data, ..
    } = message
    else {
        return;
    };
    if topic != ai_chat::TOPIC || name != ai_chat::EVENT_SEND_FAILED {
        return;
    }
    let Ok(failed) = serde_json::from_value::<ai_chat::AiSendFailed>(data.clone()) else {
        return;
    };
    if failed.validate().is_err() || !failed.channel.is_empty() {
        return;
    }
    {
        let mut queue = FAILED.lock().unwrap_or_else(|p| p.into_inner());
        if queue.len() < 8 {
            queue.push(failed.text);
        }
    }
    let waker = WAKER.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(waker) = waker {
        waker(bead::wayland::UiEvent::Model(BTreeMap::new()));
    }
}

/// The shell's font policy for measuring: Fontconfig, as for painting.
pub fn system_meter() -> Meter {
    Meter::new(Arc::new(|scene| {
        crate::font_sources::select(scene).unwrap_or_default()
    }))
}
fn number(value: f64) -> Value {
    Value::Number(value)
}
fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}
fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "Codex",
        Provider::Claude => "Claude",
    }
}

impl Chat {
    pub fn new(client: Option<Client>, source: Arc<dyn Source>, meter: Meter) -> Self {
        let style = Style::new(WIDTH as f32 - 32., SANS, MONO);
        let metrics = Metrics::new(WIDTH, &style.scale);
        Self {
            source,
            client,
            transcript: RefCell::new(Transcript::new(style, meter)),
            metrics,
            st: RefCell::new(St {
                follow: true,
                viewport: (1200., 800.),
                ..Default::default()
            }),
            notify: Default::default(),
            ticker: Default::default(),
            heartbeat: HEARTBEAT,
            texts: Default::default(),
            memory: None,
        }
    }
    /// Feed an unbounded synthetic channel into projection tests without weakening the
    /// published `ai.chat` validator used by production sources.
    #[cfg(test)]
    pub(crate) fn set_test_channel(&self, channel: AiChannel) {
        let snapshot = self.source.snapshot();
        self.st.borrow_mut().parsed = Some(Parsed {
            key: (snapshot.epoch.clone(), snapshot.revision),
            available: true,
            channel: Some(Arc::new(channel)),
        });
    }
    /// Process-local keep of the draft and section while the panel is closed.
    pub fn with_memory(mut self, memory: crate::popouts::ChatMemory) -> Self {
        self.memory = Some(memory);
        self
    }
    pub fn remembered(&self) -> BTreeMap<String, Value> {
        self.memory
            .as_ref()
            .and_then(|memory| memory.lock().ok().map(|saved| saved.clone()))
            .unwrap_or_default()
    }
    fn remember(&self, key: &str, value: &str) {
        if let Some(memory) = &self.memory
            && let Ok(mut saved) = memory.lock()
            && value.len() <= 65536
            && saved.get(key) != Some(&Value::Text(value.into()))
        {
            saved.insert(key.into(), Value::Text(value.into()));
        }
    }
    /// Heartbeat period (tests shorten it).
    #[cfg(test)]
    pub fn with_heartbeat(mut self, heartbeat: Duration) -> Self {
        self.heartbeat = heartbeat;
        self
    }
    pub fn set_viewport(&self, _width: u32, height: u32) {
        self.st.borrow_mut().viewport.1 = height as f32;
    }
    pub fn subscribe(&self, notify: Notify) -> bead::core::source::Result<Vec<Subscription>> {
        *self.notify.borrow_mut() = Some(notify.clone());
        *WAKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(notify.clone());
        let callback: bead::core::source::Callback = {
            let notify = notify.clone();
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())))
        };
        let mut subscriptions = vec![self.source.subscribe(callback.clone())?];
        if let Some(client) = &self.client {
            subscriptions.push(client.subscribe(callback)?);
        }
        Ok(subscriptions)
    }

    fn ready(&self, method: &str) -> Option<(String, Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot.clone()?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 10_000)),
        ))
    }
    fn begin(&self, call: Call, method: &str, params: Json, wake: CallWake) -> bool {
        let mut st = self.st.borrow_mut();
        let Some(client) = &self.client else {
            st.message = "bead.panel.chat-unavailable".into();
            return false;
        };
        let Some((boot, timeout)) = self.ready(method) else {
            st.message = "bead.panel.chat-unavailable".into();
            return false;
        };
        if st.pending.len() >= 8 {
            return false;
        }
        match client.begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        ) {
            Ok(ticket) => {
                st.pending.push(Pending { call, ticket });
                // The heartbeat must not wipe an error the reader has not seen yet.
                if call != Call::Open {
                    st.message.clear();
                }
                true
            }
            Err(_) => {
                st.message = "bead.common.request-failed".into();
                false
            }
        }
    }
    fn waker(&self) -> CallWake {
        let notify = self.notify.borrow().clone();
        Arc::new(move || {
            if let Some(notify) = &notify {
                notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }
        })
    }
    /// The open heartbeat: one wake per interval while the panel is shown.
    fn tick(&self, live: bool) {
        let mut ticker = self.ticker.borrow_mut();
        if !live {
            *ticker = None;
            return;
        }
        let Some(notify) = self.notify.borrow().clone().filter(|_| ticker.is_none()) else {
            return;
        };
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let period = self.heartbeat;
        let spawned = std::thread::Builder::new()
            .name("chat-heartbeat".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(period)
                {
                    notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
                }
            });
        if spawned.is_ok() {
            *ticker = Some(Ticker { _stop: stop });
        }
    }

    fn poll(&self) {
        let mut st = self.st.borrow_mut();
        let mut index = 0;
        while index < st.pending.len() {
            let Some(reply) = st.pending[index].ticket.try_take() else {
                index += 1;
                continue;
            };
            let call = st.pending.remove(index).call;
            match (call, reply) {
                (Call::Saved, Ok(value)) => st.saved = Some((Instant::now(), value)),
                (Call::Open, Ok(_)) => st.opened_at = Some(Instant::now()),
                (_, Ok(_)) => {}
                // Retry at the next heartbeat, not in a loop.
                (Call::Open, Err(_)) => {
                    st.opened_at = Some(Instant::now());
                    st.message = "bead.common.request-failed".into();
                }
                (_, Err(e)) if e.code == "stale_revision" => {
                    st.message = "bead.common.changed-elsewhere".into();
                }
                (_, Err(_)) => st.message = "bead.common.request-failed".into(),
            }
        }
    }

    fn parsed(&self, st: &mut St) {
        let snapshot = self.source.snapshot();
        let key = (snapshot.epoch.clone(), snapshot.revision);
        if st.parsed.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        let available = snapshot.connected && snapshot.data["available"].as_bool() == Some(true);
        let channel = snapshot
            .data
            .get("channels")
            .and_then(|channels| channels.get(""))
            .and_then(|value| serde_json::from_value::<AiChannel>(value.clone()).ok())
            .filter(|channel| channel.validate().is_ok());
        let channel = channel.map(Arc::new);
        // An invalid or missing update keeps the last good channel while disconnected.
        let channel = channel.or_else(|| st.parsed.take().and_then(|p| p.channel));
        st.parsed = Some(Parsed {
            key,
            available,
            channel,
        });
    }

    /// Project the open panel. `stores` supplies the composer text, the scroll offset
    /// and the answer fields; a hidden panel drops its caches and projects nothing.
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let open = stores.get("panel.open") != Some(&Value::Bool(false));
        self.tick(open);
        if !open {
            let mut st = self.st.borrow_mut();
            st.parsed = None;
            st.saved = None;
            self.transcript.borrow_mut().release();
            return BTreeMap::new();
        }
        self.poll();
        let section = match stores.get("chat.section") {
            Some(Value::Text(s)) => s.clone(),
            _ => "chat".into(),
        };
        let draft = match stores.get("chat.draft") {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let wake = self.waker();
        let mut out = BTreeMap::new();
        self.remember("chat.section", &section);
        // Keep the worker alive and recover after a reconnect (QML: open on start, every 10 s).
        let stale = {
            let st = self.st.borrow();
            st.opened_at.is_none_or(|at| at.elapsed() >= self.heartbeat)
                && !st.pending.iter().any(|p| p.call == Call::Open)
        };
        if stale && self.ready(OPEN).is_some() {
            self.begin(Call::Open, OPEN, json!({"channel": ""}), wake.clone());
        }
        if self.ready(OPEN).is_none() {
            self.st.borrow_mut().opened_at = None;
        }
        let labels = ["bead.panel.chat-tab-chat", "bead.panel.chat-tab-chats", "bead.panel.chat-tab-brain", "bead.panel.chat-new", "bead.panel.chat-workspace", "bead.panel.chat-send", "bead.panel.chat-attach", "bead.panel.chat-screenshot", "bead.panel.chat-latest", "bead.panel.chat-refresh", "bead.panel.chat-codex", "bead.panel.chat-claude"]
            .into_iter().map(|id| {
                let label = tr(id, &[]);
                let transcript = self.transcript.borrow();
                let meter = transcript.meter();
                json!({"text": label,"body": meter.width(&label,SANS,self.metrics.body),"caption": meter.width(&label,SANS,self.metrics.info)})
            }).collect();
        out.insert("chat.labels".into(), Value::list(labels));
        let mut st = self.st.borrow_mut();
        self.parsed(&mut st);
        let connected = st.parsed.as_ref().is_some_and(|p| p.available);
        let channel = st.parsed.as_ref().and_then(|p| p.channel.clone());
        let state = channel.as_ref().map(|c| c.state.clone());
        let in_workspace = state.as_ref().is_some_and(|s| s.in_workspace);
        let busy = state.as_ref().is_some_and(|s| s.busy);
        // A refused submission puts its text back in front of what was typed since.
        let failed: Vec<String> =
            std::mem::take(&mut *FAILED.lock().unwrap_or_else(|p| p.into_inner()));
        let restored = failed.into_iter().fold(draft.clone(), |current, lost| {
            if current.is_empty() {
                lost
            } else {
                format!("{lost}\n\n{current}")
            }
        });
        if restored != draft {
            out.insert("chat.draft".into(), text(restored.clone()));
        }
        let draft = restored;
        self.remember("chat.draft", &draft);

        // Geometry (QML LeftDrawer: 470 wide, at most 810 high and 150 short of the screen).
        let m = &self.metrics;
        let transcript = self.transcript.borrow();
        let meter = transcript.meter();
        let scale = transcript.style().scale.clone();
        let unit = f64::from(scale.xs);
        let height = (f64::from(st.viewport.1) - 150.).clamp(105. * unit, 202.5 * unit);
        let bottom = height - m.edge;
        let composer_h = meter.height(&draft, SANS, m.body, m.inner - 2. * scale.md) as f64
            + 2. * f64::from(scale.md);
        let composer = composer_h.clamp(m.composer_min, m.composer_max);
        let actions_top = bottom - m.button;
        let composer_top = actions_top - m.gap - composer;
        let mut floor = composer_top - m.gap;

        // Approval / question card.
        let question = state.as_ref().and_then(|s| s.question.clone());
        let qid = question
            .as_ref()
            .and_then(|q| q["id"].as_str())
            .unwrap_or("")
            .to_owned();
        if qid != st.question {
            st.question = qid.clone();
            for i in 0..MAX_QUESTIONS {
                out.insert(format!("chat.answer.{i}"), text(""));
            }
        }
        let answer = |i: usize| -> String {
            match out
                .get(&format!("chat.answer.{i}"))
                .or_else(|| stores.get(&format!("chat.answer.{i}")))
            {
                Some(Value::Text(s)) => s.clone(),
                _ => String::new(),
            }
        };
        let mut rows = Vec::new();
        let mut qextent = 0f32;
        let mut all_answered = question.is_some();
        if let Some(question) = &question {
            let items = question["questions"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (i, item) in items.iter().take(MAX_QUESTIONS).enumerate() {
                let prompt = item["question"].as_str().unwrap_or("");
                let h = meter
                    .height(prompt, SANS, m.body, m.inner - 2. * scale.md)
                    .max(m.body * 1.4);
                rows.push(json!({"id": format!("q{i}"), "k": "text", "i": i, "t": prompt, "h": h, "sel": false, "a": ""}));
                qextent += h + 6.;
                let current = answer(i);
                for (j, option) in item["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(8)
                    .enumerate()
                {
                    let label = option["label"].as_str().unwrap_or("").to_owned();
                    rows.push(json!({"id": format!("q{i}o{j}"), "k": "option", "i": i, "t": label, "h": 32, "sel": current.trim() == label && !label.is_empty(), "a": format!("option:{i}:{j}")}));
                    qextent += 37.;
                }
                rows.push(json!({"id": format!("q{i}i"), "k": "input", "i": i, "t": "", "h": 35, "sel": false, "a": ""}));
                qextent += 45.;
                all_answered &= !current.trim().is_empty();
            }
            if items.is_empty() || items.len() > MAX_QUESTIONS {
                all_answered = false;
            }
            rows.push(json!({"id": "reply", "k": "reply", "i": 0, "t": "", "h": 36, "sel": all_answered && !in_workspace, "a": "reply"}));
            qextent += 36.;
        }
        let question_h = if question.is_some() {
            f64::from(qextent)
                .clamp(m.question_min, m.question_max)
                .min(height * 0.35)
                .max(m.question_min)
        } else {
            0.
        };
        let question_top = floor - question_h;
        if question.is_some() {
            floor = question_top - 8.;
        }

        // Status or error line.
        let status_text = match &state {
            Some(s) if !s.error.is_empty() => s.error.clone(),
            Some(s) if !s.status.is_empty() => s.status.clone(),
            _ if !st.message.is_empty() => tr(&st.message, &[]),
            None if !connected => tr("bead.panel.chat-connecting", &[]),
            _ => String::new(),
        };
        let is_error =
            state.as_ref().is_some_and(|s| !s.error.is_empty()) || !st.message.is_empty();
        let status_h = if status_text.is_empty() {
            0.
        } else {
            f64::from(meter.height(&status_text, SANS, m.info, m.inner))
                .max(f64::from(m.info) * 1.4)
        };
        let status_top = floor - status_h;
        if status_h > 0. {
            floor = status_top - 8.;
        }
        let transcript_top = m.top + m.tools + 1.5 * f64::from(scale.sm);
        let transcript_h = (floor + m.gap - 10. - transcript_top).max(60.);
        drop(transcript);

        // Messages: the reusable transcript lays them out and windows them.
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        let mut entries = Vec::new();
        if let Some(channel) = &channel {
            for (ordinal, message) in &channel.messages {
                let user = message.role == Role::User;
                texts.insert(ordinal.clone(), message.text.clone());
                entries.push(Entry {
                    key: ordinal.clone(),
                    label: tr(
                        if user {
                            "bead.panel.chat-you"
                        } else {
                            "bead.panel.chat-title"
                        },
                        &[],
                    ),
                    fill: if user {
                        "secondaryContainer"
                    } else {
                        "surfaceContainer"
                    }
                    .into(),
                    text: message.text.clone(),
                    markdown: !user,
                    actions: vec![(tr("bead.panel.chat-copy", &[]), format!("copy:{ordinal}"))],
                    code_action: Some((tr("bead.panel.chat-copy-code", &[]), "code".into())),
                });
            }
        }
        let extent = self.transcript.borrow_mut().layout(&entries);

        // Follow the newest text until the reader scrolls away; clamp to the content.
        let max_scroll = f64::from((extent - transcript_h as f32).max(0.));
        let current = match stores.get("chat.scroll") {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        };
        let thread = state
            .as_ref()
            .map_or(String::new(), |s| s.thread_id.clone());
        if thread != st.thread {
            st.thread = thread;
            st.follow = true;
        } else if current != st.written {
            st.follow = current >= max_scroll;
        }
        let scroll = if st.follow {
            max_scroll
        } else {
            current.clamp(0., max_scroll)
        };
        st.written = scroll;
        out.insert("chat.scroll".into(), number(scroll));

        // Only the cells that can be seen are handed to the view.
        let visible =
            self.transcript
                .borrow_mut()
                .window(scroll as f32, transcript_h as f32, &entries);
        let cells_empty = entries.is_empty();

        // Header and footer state.
        let provider = state
            .as_ref()
            .map_or(String::new(), |s| provider_name(s.provider).to_owned());
        let title = state
            .as_ref()
            .map_or_else(|| tr("bead.panel.chat-new", &[]), |s| s.title.clone());
        let heading = format!(
            "{} · {}",
            if provider.is_empty() {
                tr("bead.panel.chat-assistant", &[])
            } else {
                provider
            },
            title
        );
        let trimmed = draft.trim();
        let placeholder = if in_workspace {
            "bead.panel.chat-placeholder-workspace"
        } else if busy {
            "bead.panel.chat-placeholder-busy"
        } else {
            "bead.panel.chat-placeholder"
        };
        let methods_ok = self.ready("ai.chat.send").is_some();
        let defaults = state
            .as_ref()
            .map(|s| provider_name(s.default_provider))
            .unwrap_or("");
        let saved_ok = self.ready(SAVED).is_some();
        if section == "chats" && saved_ok {
            let due = st
                .saved_asked
                .is_none_or(|at| at.elapsed() >= SAVED_REFRESH);
            if due && !st.pending.iter().any(|p| p.call == Call::Saved) {
                st.saved_asked = Some(Instant::now());
                drop(st);
                self.begin(Call::Saved, SAVED, json!({"channel": ""}), wake);
                st = self.st.borrow_mut();
            }
        }
        let saved_rows: Vec<Json> = st
            .saved
            .as_ref()
            .and_then(|(_, value)| value["chats"].as_array())
            .into_iter()
            .flatten()
            .take(30)
            .filter_map(|chat| {
                let key = chat["key"].as_str().filter(|k| !k.is_empty() && k.len() <= 128)?;
                let detail = format!(
                    "{} · {} · {}",
                    chat["agent"].as_str().unwrap_or(""),
                    chat["account"].as_str().unwrap_or(""),
                    chat["state"].as_str().unwrap_or("")
                );
                Some(json!({"id": key, "t": chat["title"].as_str().unwrap_or(""), "d": detail, "a": format!("resume:{key}")}))
            })
            .collect();
        let saved_state = if !saved_ok {
            "bead.panel.chat-saved-unavailable"
        } else if saved_rows.is_empty() {
            "bead.panel.chat-saved-empty"
        } else {
            ""
        };
        let _ = defaults;
        let mut set = |key: &str, value: Value| {
            out.insert(key.into(), value);
        };
        set("chat.height", number(height));
        set("chat.transcript_top", number(transcript_top));
        set("chat.transcript_height", number(transcript_h));
        set("chat.extent", number(f64::from(extent)));
        set("chat.cells", Value::list(visible));
        set("chat.edge", number(m.edge));
        set("chat.button", number(m.button));
        set("chat.tabs", number(m.tabs));
        set("chat.top", number(m.top));
        set("chat.inner", number(f64::from(m.inner)));
        set("chat.empty", Value::Bool(cells_empty));
        set("chat.heading", text(heading));
        set("chat.status", text(status_text));
        set("chat.status_error", Value::Bool(is_error));
        set("chat.status_top", number(status_top));
        set("chat.status_height", number(status_h));
        set("chat.question_top", number(question_top));
        set("chat.question_height", number(question_h));
        set("chat.question_rows", Value::list(rows));
        set("chat.question_extent", number(f64::from(qextent)));
        set("chat.composer_top", number(composer_top));
        set("chat.composer_height", number(composer));
        set("chat.actions_top", number(actions_top));
        set("chat.busy", Value::Bool(busy));
        set("chat.workspace", Value::Bool(in_workspace));
        set("chat.can_new", Value::Bool(methods_ok && !busy));
        set(
            "chat.can_workspace",
            Value::Bool(
                methods_ok && !busy && state.as_ref().is_some_and(|s| !s.thread_id.is_empty()),
            ),
        );
        set(
            "chat.can_send",
            Value::Bool(methods_ok && !in_workspace && !trimmed.is_empty()),
        );
        set(
            "chat.can_reply",
            Value::Bool(methods_ok && !in_workspace && all_answered),
        );
        set("chat.can_stop", Value::Bool(methods_ok && busy));
        set("chat.placeholder", text(tr(placeholder, &[])));
        set(
            "chat.provider",
            text(state.as_ref().map_or("", |s| match s.default_provider {
                Provider::Codex => "codex",
                Provider::Claude => "claude",
            })),
        );
        set("chat.has_question", Value::Bool(question.is_some()));
        set("chat.saved", Value::list(saved_rows));
        set(
            "chat.saved_state",
            text(if saved_state.is_empty() {
                String::new()
            } else {
                tr(saved_state, &[])
            }),
        );
        drop(st);
        *self.texts.borrow_mut() = texts;
        out
    }
}

impl Chat {
    fn state(&self) -> Option<nacre_protocol::ai_chat::AiState> {
        let mut st = self.st.borrow_mut();
        self.parsed(&mut st);
        st.parsed
            .as_ref()?
            .channel
            .as_ref()
            .map(|channel| channel.state.clone())
    }
    fn plain(&self, method: &str, extra: Json) {
        let mut params = json!({"channel": ""});
        if let (Some(base), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            base.extend(extra.clone());
        }
        self.begin(Call::Plain, method, params, self.waker());
    }
    /// Run one view action. Stores to change and text to copy are returned; every
    /// daemon call is the published method, sent once and never retried.
    pub fn act(&self, arg: &str, stores: &Stores) -> Outcome {
        let mut out = Outcome::default();
        let value = |key: &str| match stores.get(key) {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let state = self.state();
        let busy = state.as_ref().is_some_and(|s| s.busy);
        let (verb, rest) = arg.split_once(':').unwrap_or((arg, ""));
        match verb {
            "pin" => {
                out.stores.insert(
                    "chat.pinned".into(),
                    Value::Bool(stores.get("chat.pinned") != Some(&Value::Bool(true))),
                );
            }
            "send" => {
                let draft = value("chat.draft");
                if draft.trim().is_empty() || state.as_ref().is_none_or(|s| s.in_workspace) {
                    return out;
                }
                if draft.chars().count() > nacre_protocol::ai_chat::MAX_SEND_CHARS {
                    self.st.borrow_mut().message = "bead.panel.chat-too-long".into();
                    return out;
                }
                let params = json!({"channel": "", "text": draft, "attachments": []});
                if self.begin(Call::Send, "ai.chat.send", params, self.waker()) {
                    out.stores.insert("chat.draft".into(), text(""));
                    self.st.borrow_mut().follow = true;
                }
            }
            "stop" => self.plain("ai.chat.cancel", json!({})),
            "new" if !busy => {
                self.plain("ai.chat.new", json!({}));
                out.stores.insert("chat.section".into(), text("chat"));
            }
            "workspace" if !busy => self.plain("ai.chat.workspace", json!({})),
            "provider" if matches!(rest, "codex" | "claude") => {
                self.plain("ai.chat.provider", json!({"provider": rest}));
            }
            "resume" if !busy && !rest.is_empty() => {
                self.plain("ai.chat.resume", json!({"key": rest}));
                out.stores.insert("chat.section".into(), text("chat"));
                self.st.borrow_mut().follow = true;
            }
            "reply" => {
                let Some(question) = state.and_then(|s| s.question) else {
                    return out;
                };
                let items = question["questions"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut answers = serde_json::Map::new();
                for (i, item) in items.iter().take(MAX_QUESTIONS).enumerate() {
                    let given = value(&format!("chat.answer.{i}"));
                    let id = item["id"].as_str().unwrap_or("");
                    if given.trim().is_empty() || id.is_empty() {
                        return out;
                    }
                    answers.insert(id.into(), json!({"answers": [given]}));
                }
                if items.is_empty() || items.len() > MAX_QUESTIONS {
                    return out;
                }
                let id = question["id"].as_str().unwrap_or("");
                self.plain("ai.chat.approve", json!({"id": id, "answers": answers}));
            }
            "option" => {
                let mut parts = rest.split(':').filter_map(|p| p.parse::<usize>().ok());
                if let (Some(i), Some(j), Some(question)) =
                    (parts.next(), parts.next(), state.and_then(|s| s.question))
                    && let Some(label) = question["questions"][i]["options"][j]["label"].as_str()
                    && i < MAX_QUESTIONS
                {
                    out.stores.insert(format!("chat.answer.{i}"), text(label));
                }
            }
            "copy" => out.copy = self.texts.borrow().get(rest).cloned(),
            "code" => {
                let (ordinal, index) = rest.split_once(':').unwrap_or((rest, "0"));
                out.copy = self.texts.borrow().get(ordinal).and_then(|message| {
                    markdown::code_blocks(message)
                        .into_iter()
                        .nth(index.parse().ok()?)
                });
            }
            "latest" => self.st.borrow_mut().follow = true,
            "section" if matches!(rest, "chat" | "chats" | "settings") => {
                out.stores.insert("chat.section".into(), text(rest));
            }
            "refresh" => self.st.borrow_mut().saved_asked = None,
            _ => {}
        }
        out
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
pub(crate) mod tests;
