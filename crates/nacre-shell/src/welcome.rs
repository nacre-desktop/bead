// SPDX-License-Identifier: GPL-3.0-only
//! B0059 Welcome app adapter. The tour (pages, app tiles, help, links) is `shell/welcome/*.luau`
//! data and markup. This file supplies only what a view cannot know: the window size for the
//! layout, which apps this desktop can open, the live shortcut rows (the daemon's
//! `learn.shortcuts` menu route, the same bindings the launcher lists) and the effects of the
//! `welcome.do` host action. The first-run trigger and its once-only claim stay with Nacre
//! (`nacre-helper welcome`); Bead renders the window it is asked to open.
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{Stores, Value, source::Source};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

/// Node id of the footer's Next button (`shell/welcome/main.luau`): Enter activates it.
pub const NEXT: &str = "welcome.body.footer.next";
const QUERY: &str = "menu.query";
const SHORTCUTS_ROUTE: &str = "learn.shortcuts";
const AI_TOPIC: &str = "ai.chat";
const BRAIN_TOPIC: &str = "brain.recent";
/// Brain has no daemon route yet: its window is this command (QML ran it through `uwsm app`).
const BRAIN_COMMAND: &str = "nacre-brain";
const MAX_ROWS: usize = 200;
const MAX_TEXT: usize = 160;
/// Links the Help page can open. The view names one; the URL never comes from it.
const LINKS: [(&str, &str); 4] = [
    ("project", "https://github.com/nacre-desktop/nacre"),
    ("hyprland", "https://wiki.hypr.land/"),
    ("niri", "https://github.com/YaLTeR/niri/wiki"),
    ("cachyos", "https://wiki.cachyos.org/"),
];

#[derive(Default)]
struct State {
    /// Boot id the shortcut rows were last requested for: a restarted daemon asks again.
    asked: Option<String>,
    pending: Option<CallTicket>,
    rows: Vec<Json>,
    failed: bool,
}
pub struct Welcome {
    client: Option<Client>,
    state: RefCell<State>,
    wake: RefCell<Option<CallWake>>,
    viewport: std::cell::Cell<(u32, u32)>,
    /// The colours page demo (B0064): the Colors explorer's preview loop, never applying.
    demo: crate::colors::Colors,
}
/// What an action asks of the window.
#[derive(Default, Debug, PartialEq)]
pub struct Outcome {
    pub close: bool,
    /// Store changes of a demo action (`demo:<op>:<payload>`).
    pub changes: BTreeMap<String, Value>,
}

impl Welcome {
    pub fn new(client: Option<Client>) -> Self {
        Self {
            state: Default::default(),
            wake: Default::default(),
            viewport: std::cell::Cell::new((1020, 760)),
            demo: crate::colors::Colors::new(client.clone()),
            client,
        }
    }
    /// Gives the demo's image slots their pixels (`@icon:colors-*`).
    pub fn attach_images(&self, scene: &mut bead::render::Scene) {
        self.demo.attach_images(scene);
    }
    /// Shortcut rows from a fixed list (previews and tests): no daemon call is made.
    #[cfg(test)]
    pub fn with_shortcuts(self, rows: Vec<Json>) -> Self {
        *self.state.borrow_mut() = State {
            asked: Some("fixture".into()),
            rows,
            ..State::default()
        };
        self
    }
    pub fn set_viewport(&self, width: u32, height: u32) {
        self.viewport.set((width, height));
    }
    /// The reply wake: a finished daemon call projects again.
    pub fn subscribe(&self, wake: CallWake) {
        self.demo.subscribe(wake.clone());
        *self.wake.borrow_mut() = Some(wake);
    }
    fn ready(&self, method: &str) -> Option<(String, std::time::Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let method = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot?,
            std::time::Duration::from_millis(u64::from(method.timeout).clamp(1, 10_000)),
        ))
    }
    /// An optional component counts as installed while the daemon publishes its topic.
    fn published(&self, topic: &str) -> bool {
        self.client
            .as_ref()
            .is_some_and(|c| c.snapshot().connected && c.status().topics.contains_key(topic))
    }
    /// The Colors app is offered when this build has it (it shows its own unavailable view when the
    /// daemon lacks the colours methods).
    fn can_colors(&self) -> bool {
        crate::app::APPS.contains(&"colors")
    }
    fn ask(&self, state: &mut State) {
        let Some((boot, timeout)) = self.ready(QUERY) else {
            return;
        };
        let params = json!({"query": "", "mode": "apps", "route": SHORTCUTS_ROUTE});
        if let Some(client) = &self.client
            && let Ok(ticket) = client.begin_call(
                QUERY.into(),
                params,
                timeout,
                CallGuard {
                    boot: boot.clone(),
                    version: 1,
                },
                self.wake.borrow().clone(),
            )
        {
            state.pending = Some(ticket);
            state.asked = Some(boot);
        }
    }
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let on_shortcuts =
            matches!(stores.get("welcome.step"), Some(Value::Text(p)) if p == "shortcuts");
        let mut state = self.state.borrow_mut();
        if let Some(ticket) = &mut state.pending
            && let Some(reply) = ticket.try_take()
        {
            state.pending = None;
            match reply.map(|v| rows(&v)) {
                Ok(Some(found)) => (state.rows, state.failed) = (found, false),
                _ => state.failed = true,
            }
        }
        if on_shortcuts
            && state.pending.is_none()
            && let Some((boot, _)) = self.ready(QUERY)
            && state.asked.as_deref() != Some(boot.as_str())
        {
            state.failed = false;
            self.ask(&mut state);
        }
        let shown = if !state.rows.is_empty() {
            "ready"
        } else if state.pending.is_some()
            || (!state.failed && state.asked.is_none() && self.ready(QUERY).is_some())
        {
            "loading"
        } else if state.failed || state.asked.is_none() {
            "unavailable"
        } else {
            "empty"
        };
        let (width, height) = self.viewport.get();
        let flag = |name: &str, on: bool| (format!("welcome.can.{name}"), Value::Bool(on));
        BTreeMap::from([
            ("welcome.width".into(), Value::Number(f64::from(width))),
            ("welcome.height".into(), Value::Number(f64::from(height))),
            flag("settings", true),
            flag("colors", self.can_colors()),
            flag("ai", self.published(AI_TOPIC)),
            flag(
                "brain",
                self.published(BRAIN_TOPIC) && on_path(BRAIN_COMMAND),
            ),
            (
                "welcome.demo".into(),
                Value::list(vec![self.demo.demo(
                    matches!(stores.get("welcome.step"), Some(Value::Text(p)) if p == "colors"),
                )]),
            ),
            ("welcome.shortcuts".into(), Value::list(state.rows.clone())),
            ("welcome.shortcuts.state".into(), Value::Text(shown.into())),
        ])
    }
    /// The host action `welcome.do`, argument `op:payload`. The payload only selects from the
    /// fixed tables here; it never becomes a command, path or URL.
    pub fn act(&self, arg: &str, stores: &Stores) -> Outcome {
        let (op, payload) = arg.split_once(':').unwrap_or((arg, ""));
        match op {
            "demo" => Outcome {
                changes: self.demo.act(payload, stores),
                ..Outcome::default()
            },
            "close" => Outcome {
                close: true,
                ..Outcome::default()
            },
            "route" => Outcome {
                close: self.route(payload),
                ..Outcome::default()
            },
            "link" => {
                if let Some((_, url)) = LINKS.iter().find(|(name, _)| *name == payload) {
                    detach("xdg-open", &[url]);
                }
                Outcome::default()
            }
            _ => Outcome::default(),
        }
    }
    /// Open an app through the route its own launcher uses; true when something was started.
    fn route(&self, name: &str) -> bool {
        let app = |app: &str, method: &str, args: &[Json]| {
            crate::app::route(0, app, method, args).error.is_none()
        };
        match name {
            "settings" => app("settings", "open", &[]),
            "colors" => self.can_colors() && app("colors", "open", &[]),
            "ai" => self.published(AI_TOPIC) && app("chat", "toggle", &[]),
            "brain" => {
                self.published(BRAIN_TOPIC)
                    && on_path(BRAIN_COMMAND)
                    && detach(BRAIN_COMMAND, &["open"])
            }
            page => page
                .strip_prefix("settings:")
                .filter(|p| {
                    !p.is_empty()
                        && p.len() <= 64
                        && p.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'/'
                        })
                })
                .is_some_and(|page| app("settings", "open", &[json!(page)])),
        }
    }
}

/// `learn.shortcuts` rows: the action is the item name, the keys its description. `None` when the
/// reply is not a menu result.
fn rows(reply: &Json) -> Option<Vec<Json>> {
    let order = reply.get("order")?.as_array()?;
    let items = reply.get("items")?.as_object()?;
    let clip = |text: &str| text.chars().take(MAX_TEXT).collect::<String>();
    Some(
        order
            .iter()
            .filter_map(Json::as_str)
            .filter_map(|id| Some((id, items.get(id)?)))
            .filter_map(|(id, item)| {
                let title = clip(item.get("name")?.as_str()?);
                let key = clip(item.get("description").and_then(Json::as_str).unwrap_or(""));
                (!title.is_empty() && !key.is_empty())
                    .then(|| json!({"id": id, "title": title, "key": key}))
            })
            .take(MAX_ROWS)
            .collect(),
    )
}

/// True when `name` is an executable file in a PATH directory (Brain's published topic does not
/// say its app is installed).
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| in_dirs(name, &path))
}
pub(crate) fn in_dirs(name: &str, path: &std::ffi::OsStr) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path).any(|dir| {
        std::fs::metadata(dir.join(name))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// Start `program args` detached from this process (own group, no stdio); true when it started.
fn detach(program: &str, args: &[&str]) -> bool {
    use std::os::unix::process::CommandExt;
    let Ok(mut child) = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
    else {
        return false;
    };
    // Reap it when it exits; the window may outlive the child.
    let _ = std::thread::Builder::new()
        .name("bead-welcome-reap".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let _ = child.wait();
        });
    true
}

/// The Welcome window as a framework `WindowSpec`: one factory, one deep-link mapping.
pub fn window_spec(
    root: std::path::PathBuf,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    client: Option<Client>,
    trace: bool,
) -> Result<bead::wayland::WindowSpec, Box<dyn std::error::Error>> {
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.motion = None;
    fallback.primitives.clear();
    fallback.offset = (0., 0.);
    fallback.width = 1020.;
    fallback.bounds = bead::render::Rect::new(0., 0., 1020., 760.);
    let cold = fallback.clone();
    Ok(bead::wayland::WindowSpec {
        app_id: "nacre-welcome".into(),
        title: crate::i18n::tr("bead.welcome.title", &[]),
        size: (1020, 760),
        min_size: (720, 540),
        fallback,
        factory: Arc::new(move |_| {
            let session = bead::ui::Session::new(
                root.clone(),
                "welcome/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::new(session)
                .with_trace(trace)
                .with_welcome(Welcome::new(client.clone()))
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
        model: Arc::new(|args| {
            // `nacre welcome [home|colors|shortcuts|apps|ready|help]`; anything else shows the first page.
            let page = args
                .first()
                .and_then(Json::as_str)
                .filter(|p| ["colors", "shortcuts", "apps", "ready", "help"].contains(p))
                .unwrap_or("welcome");
            BTreeMap::from([
                ("welcome.step".into(), Value::Text(page.into())),
                ("welcome.guide".into(), Value::Text(String::new())),
            ])
        }),
    })
}
