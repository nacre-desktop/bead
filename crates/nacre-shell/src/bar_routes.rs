// SPDX-License-Identifier: GPL-3.0-only
//! B0022 bar actions as published daemon calls (Nacre N0031).
//!
//! With a daemon client every state-changing bar action is one semantic,
//! output-scoped method call. A method is usable only while the connected
//! daemon advertises it at version 1; otherwise the control is disabled and
//! nothing is guessed or sent. Calls ride `Client::begin_call` tickets, so a
//! queued call is dropped on a transport-generation or boot change and is never
//! replayed. Success `{changed:true}` is not state readback: the bar renders
//! whatever the compositor/service topics publish next.
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::{core::Value, core::source::Source, services::TrayItem};
use std::{cell::RefCell, collections::BTreeMap, time::Duration};

const MAX_PENDING: usize = 8;
const MAX_TRAY: usize = 64;
/// QML moves volume by 5 percent and brightness by 5 points per wheel event.
const VOLUME_STEP: f64 = 0.05;
const BRIGHTNESS_STEP: i8 = 5;
/// One conventional SNI wheel notch.
const TRAY_NOTCH: i16 = 120;

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    WorkspaceActivate {
        slot: u16,
    },
    WorkspaceScroll {
        forward: bool,
    },
    /// A window glyph on the bar (N0040: the output is optional, the window's own is used).
    WindowFocus {
        window: String,
    },
    Volume {
        up: bool,
    },
    Mute,
    Brightness {
        up: bool,
    },
    Tray {
        method: &'static str,
        item: String,
        x: i16,
        y: i16,
    },
    TrayScroll {
        item: String,
        horizontal: bool,
        up: bool,
    },
}
impl Route {
    pub fn method(&self) -> &'static str {
        match self {
            Self::WorkspaceActivate { .. } => "workspace.activate",
            Self::WorkspaceScroll { .. } => "workspace.scroll",
            Self::WindowFocus { .. } => "window.focus",
            Self::Volume { .. } => "audio.adjust_volume",
            Self::Mute => "audio.toggle_mute",
            Self::Brightness { .. } => "brightness.adjust",
            Self::Tray { method, .. } => method,
            Self::TrayScroll { .. } => "tray.scroll",
        }
    }
    /// Relative or toggling calls must never queue behind themselves.
    fn exclusive(&self) -> bool {
        !matches!(
            self,
            Self::WorkspaceActivate { .. } | Self::WindowFocus { .. } | Self::Tray { .. }
        )
    }
    /// `None` when the route needs an output and none is known yet.
    fn params(&self, output: Option<&str>) -> Option<serde_json::Value> {
        use serde_json::json;
        Some(match self {
            Self::WorkspaceActivate { slot } => {
                json!({"output": output?, "workspace": slot})
            }
            Self::WorkspaceScroll { forward } => {
                json!({"output": output?, "direction": if *forward { "next" } else { "prev" }})
            }
            Self::WindowFocus { window } => json!({"window": window}),
            Self::Volume { up } => {
                json!({"delta": if *up { VOLUME_STEP } else { -VOLUME_STEP }})
            }
            Self::Mute => json!({}),
            Self::Brightness { up } => {
                json!({"output": output?, "delta": if *up { BRIGHTNESS_STEP } else { -BRIGHTNESS_STEP }})
            }
            Self::Tray { item, x, y, .. } => json!({"item": item, "x": x, "y": y}),
            Self::TrayScroll {
                item,
                horizontal,
                up,
            } => json!({
                "item": item,
                "axis": if *horizontal { "horizontal" } else { "vertical" },
                // SNI: positive is down, so wheel-up is negative.
                "delta": if *up { -TRAY_NOTCH } else { TRAY_NOTCH },
            }),
        })
    }
}

/// Boot id and deadline when the exact method is advertised at v1 on a live link.
fn ready(client: &Client, method: &str) -> Option<(String, Duration)> {
    let status = client.status();
    let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
    if !client.snapshot().connected {
        return None;
    }
    Some((
        status.boot?,
        Duration::from_millis(u64::from(advertised.timeout).clamp(1, 30_000)),
    ))
}
/// Host-owned stores that disable the controls the daemon does not offer.
fn stores(client: &Client) -> BTreeMap<String, Value> {
    let on = |method| Value::Bool(ready(client, method).is_some());
    BTreeMap::from([
        ("bar.workspace.enabled".into(), on("workspace.activate")),
        ("bar.tray.enabled".into(), on("tray.activate")),
        ("bar.window.focus.enabled".into(), on("window.focus")),
    ])
}

pub struct Routes {
    client: Client,
    pending: RefCell<Vec<(&'static str, CallTicket)>>,
}
impl Routes {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            pending: RefCell::new(Vec::new()),
        }
    }
    /// Re-publish the capability stores whenever the link or its method registry changes.
    pub fn watch(
        &self,
        publish: impl Fn(BTreeMap<String, Value>) + Send + Sync + 'static,
    ) -> bead::core::source::Result<bead::core::source::Subscription> {
        let client = self.client.clone();
        self.client
            .subscribe(std::sync::Arc::new(move |_| publish(stores(&client))))
    }
    pub fn stores(&self) -> BTreeMap<String, Value> {
        stores(&self.client)
    }
    /// Drop finished calls; a failed call is reported by code only, never retried.
    fn reap(&self) {
        self.pending
            .borrow_mut()
            .retain_mut(|(method, ticket)| match ticket.try_take() {
                None => true,
                Some(reply) => {
                    if let Err(error) = reply {
                        eprintln!("BAR_ACTION_FAILED method={method} code={}", error.code);
                    }
                    false
                }
            });
    }
    pub fn send(
        &self,
        route: &Route,
        output: Option<&str>,
        wake: CallWake,
    ) -> Result<(), &'static str> {
        self.reap();
        let method = route.method();
        let (boot, timeout) = ready(&self.client, method).ok_or("bar control unavailable")?;
        let params = route.params(output).ok_or("output unknown")?;
        let mut pending = self.pending.borrow_mut();
        if pending.len() >= MAX_PENDING
            || (route.exclusive() && pending.iter().any(|(m, _)| *m == method))
        {
            return Err("bar control busy");
        }
        let ticket = self
            .client
            .begin_call(
                method.into(),
                params,
                timeout,
                CallGuard { boot, version: 1 },
                Some(wake),
            )
            .map_err(|_| "bar control unavailable")?;
        pending.push((method, ticket));
        Ok(())
    }
}

/// Daemon `tray` topic rows as the bar's existing tray model. Rows without a
/// valid ID, hidden rows and rows past the cap are dropped; the daemon's
/// decoded icon file (absolute path) wins over the theme icon name.
pub fn tray_items(data: &serde_json::Value) -> Vec<TrayItem> {
    let entities = &data["items"];
    let Some(order) = entities["order"].as_array() else {
        return Vec::new();
    };
    order
        .iter()
        .filter_map(|id| {
            let id = id.as_str().filter(|id| {
                !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
            })?;
            let row = entities["items"].get(id)?;
            if row["visible"] != true {
                return None;
            }
            let title = row["title"].as_str().unwrap_or_default().to_owned();
            let icon = row["icon_path"]
                .as_str()
                .filter(|p| p.starts_with('/'))
                .or_else(|| row["icon"].as_str())
                .unwrap_or_default()
                .to_owned();
            Some(TrayItem {
                id: id.to_owned(),
                tooltip_title: title.clone(),
                title,
                visible: true,
                item_is_menu: row["item_is_menu"] == true,
                icon_name: icon,
                attention: row["attention"] == true,
                ..TrayItem::default()
            })
        })
        .take(MAX_TRAY)
        .collect()
}

/// The bar's `bead.desktop.tray` rows (same shape as `desktop::values`).
pub fn tray_rows(items: &[TrayItem]) -> Vec<serde_json::Value> {
    items
        .iter()
        .map(|t| serde_json::json!({"id": t.id, "title": t.title, "icon": format!("tray:{}", t.id), "attention": t.attention}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn params_follow_the_published_schemas() {
        let p = |route: Route| route.params(Some("DP-1")).unwrap();
        assert_eq!(
            p(Route::WorkspaceActivate { slot: 3 }),
            json!({"output":"DP-1","workspace":3})
        );
        assert_eq!(
            p(Route::WorkspaceScroll { forward: false }),
            json!({"output":"DP-1","direction":"prev"})
        );
        assert_eq!(
            p(Route::WindowFocus {
                window: "niri-window:7".into()
            }),
            json!({"window":"niri-window:7"})
        );
        assert_eq!(p(Route::Volume { up: false }), json!({"delta":-0.05}));
        assert_eq!(p(Route::Mute), json!({}));
        assert_eq!(
            p(Route::Brightness { up: true }),
            json!({"output":"DP-1","delta":5})
        );
        assert_eq!(
            p(Route::TrayScroll {
                item: "sni:a".into(),
                horizontal: true,
                up: true
            }),
            json!({"item":"sni:a","axis":"horizontal","delta":-120})
        );
        assert_eq!(
            p(Route::Tray {
                method: "tray.context_menu",
                item: "sni:a".into(),
                x: 15,
                y: 9
            }),
            json!({"item":"sni:a","x":15,"y":9})
        );
        assert!(
            Route::WorkspaceScroll { forward: true }
                .params(None)
                .is_none()
        );
        assert!(Route::Brightness { up: true }.params(None).is_none());
        assert!(Route::Mute.params(None).is_some());
    }

    #[test]
    fn tray_topic_rows_become_bar_items() {
        let data = json!({"health":{"available":true,"stale":false},"items":{"items":{
            "sni:a":{"title":"A","visible":true,"item_is_menu":true,"attention":true,
                     "icon":"theme-name","icon_path":"/tmp/a.png","menu":{"items":{},"order":[]}},
            "sni:b":{"title":"B","visible":false,"item_is_menu":false,"attention":false,"menu":{"items":{},"order":[]}},
            "sni:c":{"title":"C","visible":true,"item_is_menu":false,"attention":false,"icon":"relative.png","icon_path":"rel.png","menu":{"items":{},"order":[]}},
        },"order":["sni:a","sni:b","sni:missing","sni:c"]}});
        let items = tray_items(&data);
        let ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["sni:a", "sni:c"]);
        assert_eq!(items[0].icon_name, "/tmp/a.png");
        assert!(items[0].item_is_menu && items[0].attention);
        assert_eq!(items[1].icon_name, "relative.png");
        assert!(tray_items(&json!(null)).is_empty());
        assert!(tray_items(&json!({"items":{"items":{},"order":"x"}})).is_empty());
    }
}
