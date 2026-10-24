// SPDX-License-Identifier: GPL-3.0-only
//! B0077 edge adapter. Layout, filtering and navigation live in shell/extras.
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use nacre_protocol::menu::{MenuActivated, MenuQueryResult};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Overview,
    Palette,
    Keybindings,
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Overview, Self::Palette, Self::Keybindings];
    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Palette => "palette",
            Self::Keybindings => "keybindings",
        }
    }
    fn topic(self) -> Option<&'static str> {
        match self {
            Self::Overview => Some("compositor"),
            Self::Keybindings => Some("shortcuts"),
            Self::Palette => None,
        }
    }
}
struct Query {
    text: String,
    route: String,
    boot: String,
    ticket: CallTicket,
}
#[derive(Default)]
struct State {
    query: Option<Query>,
    wanted: Option<(String, String, String)>,
    result: Option<(String, MenuQueryResult)>,
    action: Option<CallTicket>,
    refreshed: Option<String>,
    message: String,
    close: bool,
    route: Option<String>,
}
pub struct Extras {
    pub kind: Kind,
    client: Client,
    topic: Option<Arc<dyn Source>>,
    state: RefCell<State>,
}
impl Extras {
    pub fn new(kind: Kind, client: Client) -> bead::core::source::Result<Self> {
        let topic = kind
            .topic()
            .map(|name| {
                client
                    .lease_topic(name)
                    .map(|t| Arc::new(t) as Arc<dyn Source>)
            })
            .transpose()?;
        Ok(Self {
            kind,
            client,
            topic,
            state: Default::default(),
        })
    }
    pub fn subscribe(&self, wake: CallWake) -> bead::core::source::Result<Vec<Subscription>> {
        let w = wake.clone();
        let mut subscriptions = vec![self.client.subscribe(Arc::new(move |_| w()))?];
        if let Some(topic) = &self.topic {
            subscriptions.push(topic.subscribe(Arc::new(move |_| wake()))?);
        }
        Ok(subscriptions)
    }
    fn begin(&self, method: &str, params: Json, wake: Option<CallWake>) -> Option<CallTicket> {
        let status = self.client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !self.client.snapshot().connected {
            return None;
        }
        self.client
            .begin_call(
                method.into(),
                params,
                Duration::from_millis(u64::from(advertised.timeout).clamp(1, 30_000)),
                CallGuard {
                    boot: status.boot?,
                    version: 1,
                },
                wake,
            )
            .ok()
    }
    pub fn project(&self, stores: &Stores, wake: Option<CallWake>) -> BTreeMap<String, Value> {
        let mut state = self.state.borrow_mut();
        let boot = self
            .client
            .status()
            .boot
            .filter(|_| self.client.snapshot().connected);
        let mut rows = Vec::new();
        let mut windows = Vec::new();
        let mut workspaces = Vec::new();
        let mut available = false;
        let mut per_output = false;
        if let Some(topic) = &self.topic {
            let snapshot = topic.snapshot();
            let data = &snapshot.data;
            available = snapshot.connected
                && match self.kind {
                    Kind::Overview => data["available"] == true,
                    _ => data["health"]["available"] == true && data["health"]["stale"] != true,
                };
            if available {
                if self.kind == Kind::Overview {
                    per_output = data["capabilities"]["perOutputWorkspaces"] == true;
                    (windows, workspaces) = compositor_rows(data);
                } else {
                    rows = ordered(&data["items"]["items"], &data["items"]["order"])
                        .map(|(id, row)| json!({"id":id,"key":row["key"].as_str().unwrap_or_default(),"description":row["description"].as_str().unwrap_or_default()})).collect();
                }
            }
        }
        if self.kind == Kind::Keybindings && boot.is_some() && state.refreshed != boot {
            state.action = self.begin("shortcuts.refresh", json!({}), wake.clone());
            if state.action.is_some() {
                state.refreshed = boot.clone();
            }
        }
        let text = match stores.get("extras.query") {
            Some(Value::Text(s)) => s.as_str(),
            _ => "",
        };
        if self.kind == Kind::Palette {
            let route = state.route.clone().unwrap_or_else(|| {
                if text.trim().is_empty() {
                    "actions"
                } else {
                    ""
                }
                .into()
            });
            let wanted = boot
                .as_ref()
                .map(|boot| (text.to_owned(), route.clone(), boot.clone()));
            if state.wanted != wanted
                || (state.query.is_none() && state.result.is_none() && state.message.is_empty())
            {
                state.query = None; // cancellation: no obsolete activation can survive a new query/boot
                state.result = None;
                state.wanted = wanted.clone();
                state.message.clear();
                if let Some((text, route, boot)) = wanted {
                    state.query = self
                        .begin(
                            "menu.query",
                            json!({"query":text,"mode":"apps","route":route}),
                            wake.clone(),
                        )
                        .map(|ticket| Query {
                            text,
                            route,
                            boot,
                            ticket,
                        });
                }
            }
            if let Some(query) = &mut state.query
                && let Some(reply) = query.ticket.try_take()
            {
                let expected = (query.text.clone(), query.route.clone(), query.boot.clone());
                let result = reply
                    .ok()
                    .and_then(|reply| serde_json::from_value::<MenuQueryResult>(reply).ok())
                    .filter(|r| {
                        r.validate().is_ok()
                            && r.query == expected.0
                            && r.route == expected.1
                            && boot.as_ref() == Some(&expected.2)
                    });
                state.query = None;
                state.result = result.map(|r| (expected.2, r));
                if state.result.is_none() {
                    state.message = "Commands unavailable".into();
                }
            }
            if let Some((stamp, result)) = &state.result
                && Some(stamp) == boot.as_ref()
            {
                available = true;
                rows = result.order.iter().filter_map(|id| result.items.get(id).map(|row|
                    json!({"id":id,"name":row.name,"description":row.description,"icon":row.icon,"kind":row.kind,"disabled":row.disabled}))).collect();
            }
        }
        if let Some(ticket) = &mut state.action
            && let Some(reply) = ticket.try_take()
        {
            state.action = None;
            match reply {
                Err(_) => state.message = "Action unavailable; refresh and try again".into(),
                Ok(reply) if self.kind == Kind::Palette => {
                    if let Ok(result) = serde_json::from_value::<MenuActivated>(reply)
                        && result.validate().is_ok()
                    {
                        state.close = !result.keep_open;
                        if let Some(view) = result.view {
                            state.route = view.route;
                            state.wanted = None;
                        }
                    } else {
                        state.message = "Invalid command response".into();
                    }
                }
                Ok(_) => {}
            }
        }
        let mut values = BTreeMap::from([
            ("extras.kind".into(), Value::Text(self.kind.name().into())),
            ("extras.available".into(), Value::Bool(available)),
            ("extras.per_output".into(), Value::Bool(per_output)),
            ("extras.message".into(), Value::Text(state.message.clone())),
            ("extras.rows".into(), Value::list(rows)),
            ("extras.windows".into(), Value::list(windows)),
            ("extras.workspaces".into(), Value::list(workspaces)),
        ]);
        if state.close {
            values.insert("panel.open".into(), Value::Bool(false));
        }
        values
    }
    pub fn act(&self, request: &str, stores: &Stores, wake: CallWake) {
        let mut state = self.state.borrow_mut();
        if state.action.is_some() {
            return;
        }
        let (method, params) = if request == "refresh" {
            state.wanted = None;
            state.refreshed = None;
            return;
        } else if let Some(id) = request.strip_prefix("focus:") {
            let Some(topic) = &self.topic else {
                return;
            };
            let snapshot = topic.snapshot();
            if !snapshot.connected
                || snapshot.data["available"] != true
                || snapshot.data["windows"].get(id).is_none()
            {
                return;
            }
            ("window.focus", json!({"window":id}))
        } else if let Some(index) = request
            .strip_prefix("workspace:")
            .and_then(|s| s.parse::<u16>().ok())
        {
            let Some(topic) = &self.topic else {
                return;
            };
            let snapshot = topic.snapshot();
            if !snapshot.connected || snapshot.data["available"] != true || index == 0 {
                return;
            }
            if snapshot.data["capabilities"]["perOutputWorkspaces"] == true
                && !snapshot.data["workspaces"].as_object().is_some_and(|rows| {
                    rows.values().any(|row| {
                        row["index"].as_u64() == Some(u64::from(index))
                            && row["outputId"] == snapshot.data["focus"]["outputId"]
                    })
                })
            {
                return;
            }
            let Some(output) = snapshot.data["focus"]["outputId"]
                .as_str()
                .filter(|_| snapshot.connected)
            else {
                return;
            };
            (
                "workspace.activate",
                json!({"output":output,"workspace":index}),
            )
        } else if let Some(id) = request.strip_prefix("activate:") {
            let Some((boot, result)) = &state.result else {
                return;
            };
            let query = match stores.get("extras.query") {
                Some(Value::Text(s)) => s.as_str(),
                _ => "",
            };
            if self.client.status().boot.as_ref() != Some(boot)
                || result.query != query
                || !result.order.iter().any(|i| i == id)
            {
                return;
            }
            let Some(item) = result.items.get(id).filter(|i| !i.disabled) else {
                return;
            };
            let Some(token) = &item.activation else {
                return;
            };
            (
                "menu.activate",
                json!({"id":id,"activation":token,"catalog_rev":result.catalog_rev}),
            )
        } else {
            return;
        };
        state.action = self.begin(method, params, Some(wake));
        if state.action.is_none() {
            state.message = "Action unavailable".into();
        } else if method == "window.focus" || method == "workspace.activate" {
            state.close = true;
        }
    }
}
fn ordered<'a>(items: &'a Json, order: &'a Json) -> impl Iterator<Item = (&'a str, &'a Json)> {
    order
        .as_array()
        .into_iter()
        .flatten()
        .take(2048)
        .filter_map(move |id| Some((id.as_str()?, items.get(id.as_str()?)?)))
}
fn compositor_rows(data: &Json) -> (Vec<Json>, Vec<Json>) {
    let local = data["capabilities"]["perOutputWorkspaces"] == true;
    let output = &data["focus"]["outputId"];
    let workspaces = ordered(&data["workspaces"], &data["workspaceOrder"])
        .filter(|(_, row)| !local || &row["outputId"] == output)
        .map(|(id, row)| json!({"id":id,"index":row["index"].as_i64().or_else(|| row["id"].as_i64()).or_else(|| id.parse().ok()).unwrap_or(0),"name":row["name"].as_str().unwrap_or_default(),"output":row["outputId"]})).collect();
    let windows = ordered(&data["windows"], &data["windowOrder"])
        .filter(|(_, row)| row["hidden"] != true && (!local || &row["outputId"] == output))
        .map(|(id, row)| json!({"id":id,"title":row["title"].as_str().unwrap_or_default(),"app_id":row["wmClass"].as_str().or_else(|| row["appId"].as_str()).unwrap_or_default(),"workspace":row["workspaceId"],"icon":row["wmClass"].as_str().unwrap_or("application-x-executable")})).collect();
    (windows, workspaces)
}

/// The factory retains no view state or topic lease. Worker retirement drops
/// its runtime, subscriptions, pending calls, Luau VM and assets together.
pub fn panel(
    kind: Kind,
    client: Client,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    let cold = crate::sound::fallback(colors.get("surface")?);
    Ok(bead::wayland::PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::Attached(bead::core::Edge::Top),
        )),
        engages_decorations: true,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback: cold.clone(),
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "extras/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(
                    crate::placement::Attachment::Attached(bead::core::Edge::Top),
                    &colors,
                )
                .and_then(|runtime| runtime.with_extras(Extras::new(kind, client.clone())?))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|error| error.to_string())
        }),
    })
}

#[cfg(test)]
pub(crate) mod tests;
