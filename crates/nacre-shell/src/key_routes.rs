// SPDX-License-Identifier: GPL-3.0-only
//! B0026 keybind routes. The compositor binds call `nacrectl ui TARGET METHOD ARGS`,
//! the daemon forwards `ui.dispatch` to the UI provider, and this module gives the
//! launcher-family routes and the brightness step keys their Bead meaning. Menu
//! interpretation and ranking stay in the daemon; the only client-side data here is
//! the route-name table that Nacre's QML menu model also applies to bind arguments.
use crate::nacre_source::{CallGuard, CallTicket, Client};
use bead::{
    core::{Value, source::Source},
    services::ipc::Call,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

/// What a launcher-family key asks the launcher view to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub query: String,
    /// Daemon menu route id; `None` is the plain application list.
    pub route: Option<String>,
    /// QML `openMode` closes an already-open identical request; `openMenu` does not.
    pub toggle: bool,
    /// Native shell surface requested instead of the launcher companion.
    pub target_view: Option<String>,
}
impl Launch {
    pub fn model(&self) -> BTreeMap<String, Value> {
        let text = |s: &str| Value::Text(s.into());
        BTreeMap::from([
            ("launcher.mode".into(), text("apps")),
            ("launcher.route".into(), text("apps")),
            (
                "launcher.engine_route".into(),
                // The QML opens the launcher on Home (`openMode("apps")` sets route "home"); a typed query searches everything.
                text(
                    self.route
                        .as_deref()
                        .unwrap_or(if self.query.trim().is_empty() {
                            "home"
                        } else {
                            ""
                        }),
                ),
            ),
            ("launcher.selection".into(), text("")),
            ("launcher.scroll.y".into(), Value::Number(0.)),
            ("launcher.search.text".into(), text(&self.query)),
        ])
    }
}

/// Nacre's menu route aliases (`nacre-menu` `route_alias`). The daemon resolves
/// them too, but echoes the resolved id and the launcher fences replies by the
/// requested route, so Bead sends the resolved id.
fn resolve(route: &str) -> Option<&'static str> {
    Some(match route {
        "favorites" => "home",
        "all" => "apps",
        "settings" => "setup",
        "system" => "power",
        "clipboard" => "actions.clipboard",
        "keys" => "learn.shortcuts",
        "overview" => "actions.windows",
        "palette" => "style",
        "wallpaper" => "style.wallpapers",
        _ => return None,
    })
}
fn menu_route(route: &str) -> Result<String, &'static str> {
    let route = if route.is_empty() { "home" } else { route };
    let valid = route.len() <= 128
        && route
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    if !valid {
        return Err("invalid menu route");
    }
    Ok(resolve(route).unwrap_or(route).to_owned())
}

/// `None` is not a launcher-family route; `Some(Err)` is one Bead refuses.
pub fn launcher(call: &Call) -> Option<Result<Launch, &'static str>> {
    if call.target != "nacre"
        || (call.method == "mode"
            && matches!(
                call.args.first().and_then(serde_json::Value::as_str),
                Some("overview" | "palette" | "keys")
            ))
    {
        return None;
    }
    let arg = |i: usize| call.args.get(i).and_then(serde_json::Value::as_str);
    let launch = |query: &str, route: Option<&str>, toggle| {
        Ok(Launch {
            query: query.into(),
            route: route.map(Into::into),
            toggle,
            target_view: None,
        })
    };
    Some(match call.method.as_str() {
        "launcher" => {
            let query = arg(0).unwrap_or_default();
            match query.strip_prefix(">wallpaper") {
                Some(rest) if rest.is_empty() || rest.starts_with(char::is_whitespace) => {
                    Ok(Launch {
                        query: rest.trim().into(),
                        route: None,
                        toggle: true,
                        target_view: Some("wallpaper".into()),
                    })
                }
                _ => launch(query, None, true),
            }
        }
        "menu" => menu_route(arg(0).unwrap_or_default())
            .and_then(|route| launch(arg(1).unwrap_or_default(), Some(&route), false)),
        "clipboard" => launch("", Some("actions.clipboard"), true),
        "mode" => match arg(0).unwrap_or_default() {
            "apps" => launch("", None, true),
            "legacy" => Err("legacy launcher mode is not implemented"),
            "wallpaper" => Ok(Launch {
                query: String::new(),
                route: None,
                toggle: true,
                target_view: Some("wallpaper".into()),
            }),
            "clipboard" => launch("", resolve("clipboard"), true),
            _ => Err("unknown launcher mode"),
        },
        _ => return None,
    })
}
const BRIGHTNESS: &str = "brightness.adjust";
const BRIGHTNESS_SET: &str = "brightness.set";
const KEYBOARD_LIGHT: &str = "keyboard_light.adjust";
/// QML `NacreBrightness.step` moves the focused display by five percentage points.
const BRIGHTNESS_STEP: i64 = 5;

/// `nacrectl ui brightness step|set` and `keyboardLight step` as one daemon call each
/// (N0031 `brightness.adjust`/`brightness.set`, N0036 `keyboard_light.adjust`). Enabled only
/// while the connected daemon advertises that method at version 1. One call at a time: the
/// daemon rejects concurrent brightness admission, so a held key never queues.
pub struct Brightness {
    client: Client,
    pending: Arc<Mutex<Option<CallTicket>>>,
}
impl Brightness {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            pending: Default::default(),
        }
    }
    pub fn step(&self, direction: &str, output: Option<&str>) -> Result<(), &'static str> {
        let delta = match direction {
            "up" => BRIGHTNESS_STEP,
            "down" => -BRIGHTNESS_STEP,
            _ => return Err("invalid brightness direction"),
        };
        let output = output.ok_or("no output is available")?;
        self.send(
            BRIGHTNESS,
            serde_json::json!({"output": output, "delta": delta}),
        )
    }
    /// `brightness set X`: X is the QML fraction 0..1 (what `desktop-actions brightness` sends).
    pub fn set(&self, fraction: f64, output: Option<&str>) -> Result<(), &'static str> {
        if !fraction.is_finite() {
            return Err("invalid brightness value");
        }
        let output = output.ok_or("no output is available")?;
        let percent = (fraction.clamp(0., 1.) * 100.).round() as u8;
        self.send(
            BRIGHTNESS_SET,
            serde_json::json!({"output": output, "percent": percent}),
        )
    }
    /// `keyboardLight step up|down`: one raw unit; the daemon wraps up and stops at zero.
    pub fn keyboard_step(&self, direction: &str) -> Result<(), &'static str> {
        if !matches!(direction, "up" | "down") {
            return Err("invalid keyboard light direction");
        }
        self.send(KEYBOARD_LIGHT, serde_json::json!({"step": direction}))
    }
    fn send(&self, method: &str, params: serde_json::Value) -> Result<(), &'static str> {
        let status = self.client.status();
        let advertised = status
            .methods
            .get(method)
            .filter(|m| m.v == 1)
            .ok_or("brightness control is unavailable")?;
        if !self.client.snapshot().connected {
            return Err("brightness control is unavailable");
        }
        let boot = status.boot.ok_or("brightness control is unavailable")?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "brightness state poisoned")?;
        if let Some(ticket) = pending.as_mut() {
            if ticket.try_take().is_none() {
                return Err("brightness change in progress");
            }
            *pending = None;
        }
        let timeout = Duration::from_millis(u64::from(advertised.timeout).clamp(1, 60_000));
        let ticket = self
            .client
            .begin_call(
                method.into(),
                params,
                timeout,
                CallGuard { boot, version: 1 },
                None,
            )
            .map_err(|_| "brightness request was not sent")?;
        *pending = Some(ticket);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn call(method: &str, args: &[&str]) -> Call {
        crate::commands::from_strings(
            "nacre".into(),
            method.into(),
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn routed(method: &str, args: &[&str]) -> Launch {
        launcher(&call(method, args)).unwrap().unwrap()
    }
    /// Every launcher-family command the owner's niri and Hyprland binds issue.
    #[test]
    fn owner_bind_commands_map_to_launcher_routes() {
        let l = routed("launcher", &[""]);
        assert_eq!((l.query.as_str(), l.route, l.toggle), ("", None, true));
        let l = routed("launcher", &[">wallpaper "]);
        assert_eq!(
            (
                l.query.as_str(),
                l.route.as_deref(),
                l.toggle,
                l.target_view.as_deref()
            ),
            ("", None, true, Some("wallpaper"))
        );
        assert_eq!(routed("launcher", &[">wallpaper forest"]).query, "forest");
        assert_eq!(routed("launcher", &[">wallpaperXYZ"]).target_view, None);
        let l = routed("clipboard", &[]);
        assert_eq!(l.route.as_deref(), Some("actions.clipboard"));
        for (route, id) in [
            ("actions.emoji", "actions.emoji"),
            ("actions.capture", "actions.capture"),
            ("learn.shortcuts", "learn.shortcuts"),
            ("power", "power"),
        ] {
            let l = routed("menu", &[route, ""]);
            assert_eq!((l.route.as_deref(), l.toggle), (Some(id), false));
        }
        assert_eq!(
            routed("mode", &["clipboard"]).route.as_deref(),
            Some("actions.clipboard")
        );
        assert_eq!(
            routed("mode", &["wallpaper"]).target_view.as_deref(),
            Some("wallpaper")
        );
        for mode in ["overview", "palette", "keys"] {
            assert!(launcher(&call("mode", &[mode])).is_none());
        }
        assert_eq!(routed("mode", &["apps"]).route, None);
    }
    #[test]
    fn refusals_and_unrelated_routes_are_explicit() {
        assert!(launcher(&call("mode", &["legacy"])).unwrap().is_err());
        assert!(launcher(&call("mode", &["nonsense"])).unwrap().is_err());
        assert!(
            launcher(&call("menu", &["Bad Route!", ""]))
                .unwrap()
                .is_err()
        );
        assert_eq!(routed("menu", &["", "q"]).route.as_deref(), Some("home"));
        assert_eq!(routed("menu", &["", "q"]).query, "q");
        assert!(launcher(&call("hide", &[])).is_none());
        let other = Call {
            version: 1,
            id: 1,
            target: "drawers".into(),
            method: "launcher".into(),
            args: vec![json!("x")],
            output: None,
        };
        assert!(launcher(&other).is_none());
    }
    #[test]
    fn launch_model_replaces_route_and_query_state() {
        let model = routed("menu", &["actions.emoji", "smile"]).model();
        assert_eq!(
            model["launcher.engine_route"],
            Value::Text("actions.emoji".into())
        );
        assert_eq!(model["launcher.search.text"], Value::Text("smile".into()));
        assert_eq!(model["launcher.mode"], Value::Text("apps".into()));
        let plain = routed("launcher", &["a"]).model();
        assert_eq!(plain["launcher.engine_route"], Value::Text(String::new()));
        // No query and no route opens on Home, as the QML does.
        let home = routed("launcher", &[""]).model();
        assert_eq!(home["launcher.engine_route"], Value::Text("home".into()));
    }
}
