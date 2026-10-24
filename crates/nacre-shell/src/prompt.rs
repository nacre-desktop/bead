// SPDX-License-Identifier: GPL-3.0-only
//! B0093 selector prompt: the edge adapter for `menuPrompt open ID` (`nacre-menu pick|input`, the Wi-Fi
//! chooser, the several-windows chooser). Layout, filtering, ranking and keyboard selection live in
//! shell/prompt. This file reads the request through `nacre-helper menu-cli read-request` and answers
//! through `menu-cli respond`, the same two calls the QML shell makes, with the same response object
//! `{value, cancelled, error}`. It runs inside the on-demand `--app prompt ID` process (B0052), so the
//! view, its Luau VM and this state exist only while a prompt is open. Typed text is only ever passed to
//! the responder; it is never logged or kept.
use crate::i18n::tr;
use bead::core::{Stores, Value};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The responder refuses longer values (menu_cli.rs `respond`).
const MAX_VALUE: usize = 16384;
const MAX_ROWS: usize = 2000;

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    pub value: String,
    pub description: String,
    pub icon: String,
    pub disabled: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub input: bool,
    pub title: String,
    pub initial: String,
    pub choices: Vec<Choice>,
    pub expires: f64,
}

pub fn is_id(ident: &str) -> bool {
    ident.len() == 32
        && ident
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64())
}
fn text(stores: &Stores, key: &str) -> String {
    match stores.get(key) {
        Some(Value::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

impl Request {
    /// The helper's `read-request` output: already validated (owner, mode, version, expiry, limits).
    pub fn parse(json: &str) -> Result<Self, String> {
        let data: Json = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let input = match data["kind"].as_str() {
            Some("pick") => false,
            Some("input") => true,
            _ => return Err("unknown selector kind".into()),
        };
        let mut choices = Vec::new();
        for row in data["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .take(MAX_ROWS)
        {
            let label = row["label"].as_str().ok_or("choice without a label")?;
            choices.push(Choice {
                label: label.into(),
                value: row["value"].as_str().unwrap_or(label).into(),
                description: row["description"].as_str().unwrap_or_default().into(),
                icon: row["icon"].as_str().unwrap_or("chevron_right").into(),
                disabled: row["disabled"] == true,
            });
        }
        Ok(Self {
            input,
            title: data["title"].as_str().unwrap_or_default().into(),
            initial: data["initial"].as_str().unwrap_or_default().into(),
            choices,
            expires: data["expires"].as_f64().ok_or("no expiry")?,
        })
    }
    /// `nacre-helper menu-cli read-request ID`.
    pub fn read(helper: &Path, ident: &str) -> Result<Self, String> {
        if !is_id(ident) {
            return Err("invalid selector id".into());
        }
        let out = bead::services::process::command(helper)
            .args(["menu-cli", "read-request", ident])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("the helper refused the request".into());
        }
        Self::parse(&String::from_utf8_lossy(&out.stdout))
    }
}

/// `nacre-helper menu-cli respond ID` with one JSON line on stdin (the QML `NacreMenuPrompt` writer).
pub fn respond(
    helper: &Path,
    ident: &str,
    value: &str,
    cancelled: bool,
    error: &str,
) -> Result<(), String> {
    let mut child = bead::services::process::command(helper)
        .args(["menu-cli", "respond", ident])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let line = json!({"value": value, "cancelled": cancelled, "error": error});
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    child.wait().map(drop).map_err(|e| e.to_string())
}

/// Where the helper lives: the installed tools directory unless `NACRE_HELPER` names another.
pub fn helper_binary() -> PathBuf {
    std::env::var_os("NACRE_HELPER")
        .map(Into::into)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            Path::new(&home).join(".local/share/nacre/shell/tools/nacre-helper")
        })
}

pub struct Prompt {
    helper: PathBuf,
    ident: String,
    request: Request,
    answered: Cell<bool>,
    last_query: RefCell<String>,
}
impl Prompt {
    pub fn new(helper: PathBuf, ident: String, request: Request) -> Self {
        Self {
            helper,
            ident,
            request,
            answered: Cell::new(false),
            last_query: RefCell::default(),
        }
    }
    /// Stores that exist from the first frame: the request as the view shows it. Row ids are positions
    /// ("c0", "c1", ...); values never reach the view.
    pub fn stores(&self) -> BTreeMap<String, Value> {
        let rows = self.request.choices.iter().enumerate().map(|(index, c)| {
            json!({"id": format!("c{index}"), "name": c.label, "description": c.description, "icon": c.icon, "disabled": c.disabled})
        });
        BTreeMap::from([
            (
                "prompt.kind".into(),
                Value::Text(if self.request.input { "input" } else { "pick" }.into()),
            ),
            (
                "prompt.title".into(),
                Value::Text(self.request.title.clone()),
            ),
            ("prompt.rows".into(), Value::list(rows.collect())),
            (
                "prompt.input".into(),
                Value::Text(self.request.initial.clone()),
            ),
        ])
    }
    /// Ends the wait at the request's deadline even when nothing else wakes the panel.
    pub fn watch(&self, wake: crate::nacre_source::CallWake) {
        let left = (self.request.expires - now()).max(0.);
        let _ = std::thread::Builder::new()
            .name("bead-prompt-expiry".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                std::thread::sleep(Duration::from_secs_f64(left + 0.05));
                wake();
            });
    }
    fn finish(&self, value: &str, cancelled: bool, error: &str) {
        if self.answered.replace(true) {
            return;
        }
        if let Err(reason) = respond(&self.helper, &self.ident, value, cancelled, error) {
            eprintln!("MENU_PROMPT_RESPOND_FAILED {reason}");
        }
    }
    pub fn answered(&self) -> bool {
        self.answered.get()
    }
    /// Store changes after any update: a new filter restarts the selection and the scroll, and the
    /// deadline closes the prompt as the QML timer does.
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        let query = text(stores, "prompt.query");
        if *self.last_query.borrow() != query {
            *self.last_query.borrow_mut() = query;
            out.insert("prompt.selected".into(), Value::Text(String::new()));
            out.insert("prompt.scroll".into(), Value::Number(0.));
        }
        if !self.answered() && now() >= self.request.expires {
            self.finish("", true, &tr("bead.prompt.timeout", &[]));
        }
        if self.answered() {
            out.insert("panel.open".into(), Value::Bool(false));
        }
        out
    }
    /// A view action: "choose:ID", "submit" or "cancel". Anything else, an unknown or disabled row, or a
    /// request of the wrong kind is ignored. Returns the store changes (the panel closes when answered).
    pub fn act(&self, request: &str, stores: &Stores) -> BTreeMap<String, Value> {
        if let Some(id) = request.strip_prefix("choose:") {
            let choice = id
                .strip_prefix('c')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| self.request.choices.get(n))
                .filter(|c| !c.disabled && !self.request.input);
            if let Some(choice) = choice {
                self.finish(&choice.value, false, "");
            }
        } else if request == "submit" && self.request.input {
            let value: String = text(stores, "prompt.input")
                .chars()
                .take(MAX_VALUE)
                .collect();
            self.finish(&value, false, "");
        } else if request == "cancel" {
            self.finish("", true, "");
        }
        self.project(stores)
    }
}
impl Drop for Prompt {
    /// Closing by any other route answers "cancelled" so the waiting tool does not run into its timeout.
    fn drop(&mut self) {
        self.finish("", true, "");
    }
}

/// The panel: the launcher's size, bottom attachment and frame, with the prompt view. The factory builds the
/// request state, so a closed prompt holds nothing (and the process ends with its panel).
pub fn panel(
    palette: PathBuf,
    colors: bead::render::Palette,
    helper: PathBuf,
    ident: String,
    request: Request,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    use bead::render::{Primitive, Rect};
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 1100.;
    fallback.bounds = Rect::new(-32., -32., 1164., 654.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 1100., 590.),
        radius: 12.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "prompt".into();
        motion.edge = bead::core::Edge::Bottom;
    }
    let cold = fallback.clone();
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::Attached(bead::core::Edge::Bottom),
        )),
        engages_decorations: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "prompt/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let prompt = Prompt::new(helper.clone(), ident.clone(), request.clone());
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(
                    crate::placement::Attachment::Attached(bead::core::Edge::Bottom),
                    &colors,
                )
                .and_then(|runtime| runtime.with_prompt(prompt))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

#[cfg(test)]
pub(crate) mod tests;
