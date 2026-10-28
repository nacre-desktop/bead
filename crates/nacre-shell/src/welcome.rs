// SPDX-License-Identifier: GPL-3.0-only
//! B0059 Welcome app adapter. The tour (pages, app tiles, help, links) is `shell/welcome/*.luau`
//! data and markup. This file supplies only what a view cannot know: the window size for the
//! layout, which apps this desktop can open, the live shortcut rows (the daemon's
//! `shortcuts` topic, the same bindings Settings lists) and the effects of the
//! `welcome.do` host action. The first-run trigger and its once-only claim stay with Nacre
//! (`nacre-helper welcome`); Bead renders the window it is asked to open.
use crate::nacre_source::{CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

/// Node id of the footer's Next button (`shell/welcome/main.luau`): Enter activates it.
pub const NEXT: &str = "welcome.body.footer.next";
pub const SHORTCUTS: &str = "shortcuts";
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

pub struct Welcome {
    client: Option<Client>,
    shortcuts: Option<Arc<dyn Source>>,
    subscription: RefCell<Option<Subscription>>,
    #[cfg(test)]
    fixture: Option<Vec<Json>>,
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
            shortcuts: client
                .as_ref()
                .and_then(|c| c.topic(SHORTCUTS).ok())
                .map(|t| Arc::new(t) as Arc<dyn Source>),
            subscription: Default::default(),
            #[cfg(test)]
            fixture: None,
            viewport: std::cell::Cell::new((1100, 800)),
            demo: crate::colors::Colors::new(client.clone()),
            client,
        }
    }
    fn with_demo(mut self, root: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        self.demo = crate::colors::Colors::bundled_demo(root)?;
        Ok(self)
    }
    /// Gives the demo's image slots their pixels (`@icon:colors-*`).
    pub fn attach_images(&self, scene: &mut bead::render::Scene) {
        self.demo.attach_images(scene);
    }
    /// Shortcut rows from a fixed list (previews and tests): no daemon call is made.
    #[cfg(test)]
    pub fn with_shortcuts(mut self, rows: Vec<Json>) -> Self {
        self.fixture = Some(rows);
        self
    }
    #[cfg(test)]
    pub fn with_source(mut self, source: Arc<dyn Source>) -> Self {
        self.shortcuts = Some(source);
        self
    }
    pub fn set_viewport(&self, width: u32, height: u32) {
        self.viewport.set((width, height));
    }
    /// The reply wake: a finished daemon call projects again.
    pub fn subscribe(&self, wake: CallWake) {
        if let Some(source) = &self.shortcuts {
            let changed = wake.clone();
            *self.subscription.borrow_mut() = source.subscribe(Arc::new(move |_| changed())).ok();
        }
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
    pub fn project(&self, _stores: &Stores) -> BTreeMap<String, Value> {
        let snapshot = self.shortcuts.as_ref().map(|s| s.snapshot());
        let mut found = snapshot
            .as_ref()
            .map_or_else(Vec::new, |s| rows(&s.data).unwrap_or_default());
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            found = fixture.clone();
        }
        let available = snapshot
            .as_ref()
            .is_some_and(|s| s.connected && s.data["health"]["available"] == true);
        #[cfg(test)]
        let available = available || self.fixture.is_some();
        let shown = if !found.is_empty() {
            "ready"
        } else if available {
            "empty"
        } else {
            "unavailable"
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
            ("welcome.demo".into(), Value::list(vec![self.demo.demo()])),
            (
                "welcome.shortcuts".into(),
                Value::list(std::mem::take(&mut found)),
            ),
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

/// Published shortcuts topic: preserve stable IDs, order and compositor submap headings.
fn rows(reply: &Json) -> Option<Vec<Json>> {
    let entities = reply.get("items")?;
    let order = entities.get("order")?.as_array()?;
    let items = entities.get("items")?.as_object()?;
    let clip = |text: &str| text.chars().take(MAX_TEXT).collect::<String>();
    Some(
        order
            .iter()
            .filter_map(Json::as_str)
            .filter_map(|id| Some((id, items.get(id)?)))
            .filter_map(|(id, item)| {
                let title = clip(item.get("description")?.as_str()?);
                let key = clip(item.get("key").and_then(Json::as_str).unwrap_or(""));
                (!title.is_empty() && !key.is_empty()).then(|| {
                    json!({"id": id, "title": title, "key": key,
                        "group": clip(item["submap"].as_str().unwrap_or("")),
                        "menu": item["menu"].is_string()})
                })
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
pub(crate) fn detach(program: &str, args: &[&str]) -> bool {
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
    fallback.width = 1100.;
    fallback.bounds = bead::render::Rect::new(0., 0., 1100., 800.);
    let cold = fallback.clone();
    Ok(bead::wayland::WindowSpec {
        command_targets: Vec::new(),
        app_id: "nacre-welcome".into(),
        title: crate::i18n::tr("bead.welcome.title", &[]),
        size: (1100, 800),
        size_for_output: None,
        on_resize: None,
        min_size: (1100, 800),
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
                .with_welcome(
                    Welcome::new(client.clone())
                        .with_demo(&root)
                        .map_err(|e| e.to_string())?,
                )
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
