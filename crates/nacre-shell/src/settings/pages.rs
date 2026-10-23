// SPDX-License-Identifier: GPL-3.0-only
//! B0063: the registry's custom sections (`customType`) and action rows. This is the Nacre edge of
//! the Settings window: it knows which topics and methods the published contracts (N0042, N0046,
//! N0047, N0049) give each section and turns them into generic rows (`info`, `actions`, `device`,
//! `picker`, `help`) that `shell/settings/main.luau` draws with kit components. A control whose
//! contract is not published is a `help` row saying so; the precise ask is in spec 0063.
use super::*;

/// The controller's fixed presets (N0049) and their texts.
const PRESETS: [(&str, &str); 7] = [
    ("normal", "bead.settings.preset-normal"),
    ("focused", "bead.settings.preset-focused"),
    ("presentation", "bead.settings.preset-presentation"),
    ("minimal", "bead.settings.preset-minimal"),
    ("meeting", "bead.settings.preset-meeting"),
    ("music", "bead.settings.preset-music"),
    ("docked", "bead.settings.preset-docked"),
];
const ROLES: [&str; 6] = ["Browser", "Nacre AI", "Discord", "Spotify", "Mail", "Brain"];
const CLOCKS: [&str; 2] = ["12-hour", "24-hour"];
const MAX_ZONES: usize = 2048;
const ZONE_ROWS: usize = 8;
const HISTORY_ROWS: usize = 20;
const NODE_NAME_LEN: usize = 128;

/// The `(id, topic)` pairs a section reads besides the stores its settings name.
type Rows = (Vec<Json>, Vec<String>);

fn entities(node: &Json) -> impl Iterator<Item = (&str, &Json)> {
    node["order"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .filter_map(move |id| node["items"].get(id).map(|item| (id, item)))
}
fn healthy(topic: &Json) -> bool {
    topic["health"]["available"] == true
}
fn short(text: &str, chars: usize) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.chars().count() <= chars {
        clean.into()
    } else {
        format!("{}…", clean.chars().take(chars).collect::<String>())
    }
}
fn info(id: &str, label: &str, value: &str, description: &str) -> Json {
    json!({"id": id, "kind": "info", "label": label, "value": value, "description": description,
        "enabled": true, "own": true})
}
fn opt(arg: String, label: &str, selected: bool, enabled: bool) -> Json {
    json!({"id": arg, "label": label, "selected": selected, "enabled": enabled})
}
fn actions(id: &str, label: &str, description: &str, options: Vec<Json>, enabled: bool) -> Json {
    json!({"id": id, "kind": "actions", "label": label, "description": description,
        "options": options, "enabled": enabled, "own": true})
}
fn help(id: &str, label: &str) -> Json {
    json!({"id": id, "kind": "help", "label": label})
}
fn requires(id: &str, label: &str) -> Json {
    help(
        id,
        &format!("{label}: {}", tr("bead.settings.requires-contract", &[])),
    )
}
fn on_off(id: &str, label: &str, description: &str, on: bool, arg: &str, enabled: bool) -> Json {
    actions(
        id,
        label,
        description,
        vec![
            opt(
                format!("do:{arg}|on"),
                &tr("bead.settings.on", &[]),
                on,
                enabled,
            ),
            opt(
                format!("do:{arg}|off"),
                &tr("bead.settings.off", &[]),
                !on,
                enabled,
            ),
        ],
        enabled,
    )
}
/// A PipeWire node name the daemon accepts: `[A-Za-z0-9._:@+-]{1,128}`.
fn node_name(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= NODE_NAME_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:@+-".contains(&b))
}
fn ago(created_ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let minutes = now.saturating_sub(created_ms) / 60_000;
    let count = |n: u64| n.to_string();
    match minutes {
        0 => tr("bead.settings.ago-now", &[]),
        1..=59 => tr("bead.settings.ago-minutes", &[("count", &count(minutes))]),
        60..=1439 => tr(
            "bead.settings.ago-hours",
            &[("count", &count(minutes / 60))],
        ),
        _ => tr(
            "bead.settings.ago-days",
            &[("count", &count(minutes / 1440))],
        ),
    }
}

impl Settings {
    fn rev(&self, store: &str) -> u64 {
        self.document(store).0
    }

    // ---- rows ------------------------------------------------------------------------------

    /// The rows of a `customType` section and the topics they read; `None` when Bead has no
    /// builder for it (the page then says so).
    pub(super) fn custom_rows(
        &self,
        section: &Json,
        editable: bool,
        state: &State,
    ) -> Option<Rows> {
        let sid = section["id"].as_str().unwrap_or("");
        let topic = |name: &str| vec![name.to_owned()];
        Some(match sid {
            "date-time-language.clock" => (
                self.time_rows(editable, state),
                vec!["time".into(), "settings.store.desktop".into()],
            ),
            "notifications.history" => (self.history_rows(editable, state), topic("notifications")),
            "windows-workspaces/workflows.workflow" => (
                self.workflow_rows(editable, state),
                vec!["settings.desktop".into(), "settings.store.desktop".into()],
            ),
            "sound.devices" => (self.sound_rows(editable), topic("audio")),
            "network.devices" => (self.network_rows(editable, state), topic("network")),
            "bluetooth.devices" => (self.bluetooth_rows(editable), topic("bluetooth")),
            "system.health" => (
                vec![requires(
                    "system.health",
                    &tr("bead.settings.desktop-health", &[]),
                )],
                vec![],
            ),
            "keyboard-mouse.shortcuts" => (
                vec![requires("shortcuts", &tr("bead.settings.shortcuts", &[]))],
                vec![],
            ),
            "addons.optional" => (self.addon_rows(), vec![]),
            "nacre-ai.assistant" => (self.assistant_rows(), vec![]),
            _ => return None,
        })
    }

    /// Rows Bead adds to a registry section that no registry entry describes.
    pub(super) fn extra_rows(
        &self,
        page: &str,
        section: &str,
        editable: bool,
        _state: &State,
    ) -> Vec<(Json, Vec<String>)> {
        let mut out = Vec::new();
        if (page, section) == ("lock-login", "lock-login.widgets")
            && self.can("login.install_theme")
        {
            out.push((
                actions(
                    "login-theme",
                    &tr("bead.settings.login-theme", &[]),
                    &tr("bead.settings.login-theme-description", &[]),
                    vec![opt(
                        "do:install-login".into(),
                        &tr("bead.settings.login-install", &[]),
                        false,
                        editable,
                    )],
                    editable,
                ),
                vec![],
            ));
        }
        out
    }

    /// A registry action: a link to its page, a button, or the "requires" line.
    pub(super) fn action_row(&self, action: &Json, editable: bool, state: &State) -> Option<Json> {
        let registry = &self.registry;
        let id = action["id"].as_str().unwrap_or("");
        let label = registry.text(action, "labelId");
        let description = registry.text(action, "descriptionId");
        if let Some(route) = action["route"].as_str() {
            let page = route.split('#').next().unwrap_or("");
            // A child page already has its link at the top of the page.
            let child = registry
                .page(page)
                .is_some_and(|p| p["parent"] == action["page"]);
            return (!child && registry.page(page).is_some()).then(|| {
                json!({"id": format!("action:{id}"), "kind": "link", "page": page,
                    "label": label, "description": description})
            });
        }
        let mut row = match action["adapter"].as_str().unwrap_or("") {
            "lock-now" => actions(
                id,
                &label,
                &description,
                vec![opt(
                    "do:lock".into(),
                    &label,
                    false,
                    editable && self.can("session.request"),
                )],
                editable,
            ),
            "notifications-history" => {
                self.clear_history_row(id, &label, &description, state, editable)
            }
            // Bead previews its own lock; the idle policy and the font list have no Bead page.
            "lock-preview" | "idle-policy" | "fonts" => return None,
            _ => requires(id, &label),
        };
        row["id"] = json!(id);
        Some(row)
    }

    fn clear_history_row(
        &self,
        id: &str,
        label: &str,
        description: &str,
        state: &State,
        editable: bool,
    ) -> Json {
        let can = editable && self.can("notifications.clear");
        if state.confirm == "clear-history" {
            return actions(
                id,
                label,
                description,
                vec![
                    opt(
                        "do:clear-history".into(),
                        &tr("bead.settings.history-confirm", &[]),
                        true,
                        can,
                    ),
                    opt(
                        "do:cancel".into(),
                        &tr("bead.common.cancel", &[]),
                        false,
                        true,
                    ),
                ],
                can,
            );
        }
        actions(
            id,
            label,
            description,
            vec![opt("do:confirm|clear-history".into(), label, false, can)],
            can,
        )
    }

    fn time_rows(&self, editable: bool, state: &State) -> Vec<Json> {
        let Some(time) = self.topic("time").filter(healthy) else {
            return vec![help("time", &tr("bead.settings.time-unavailable", &[]))];
        };
        let zone = time["timezone"].as_str().unwrap_or("");
        let mut rows = vec![info(
            "time.zone",
            &tr("bead.settings.time-zone", &[]),
            zone,
            "",
        )];
        let ntp = &time["ntp"];
        let can_ntp = editable && ntp["supported"] == true && self.can("time.set_ntp");
        rows.push(on_off(
            "time.ntp",
            &tr("bead.settings.ntp", &[]),
            &tr(
                if ntp["supported"] == true {
                    "bead.settings.ntp-description"
                } else {
                    "bead.settings.ntp-unsupported"
                },
                &[],
            ),
            ntp["enabled"] == true,
            "ntp",
            can_ntp,
        ));
        let (_, store, _) = self.document("desktop");
        let clock = store["clockFormat"]
            .as_str()
            .filter(|c| CLOCKS.contains(c))
            .unwrap_or("24-hour");
        rows.push(actions(
            "time.clock",
            &tr("bead.settings.clock-format", &[]),
            "",
            CLOCKS
                .iter()
                .map(|c| {
                    let label = tr(
                        if *c == "12-hour" {
                            "bead.settings.clock-12"
                        } else {
                            "bead.settings.clock-24"
                        },
                        &[],
                    );
                    opt(format!("do:clock|{c}"), &label, clock == *c, editable)
                })
                .collect(),
            editable,
        ));
        // The zone picker: the daemon's list, filtered by what is typed.
        let tokens: Vec<String> = state
            .pick
            .split_whitespace()
            .map(|t| t.to_lowercase().replace('_', " "))
            .collect();
        let matching: Vec<&String> = state
            .zones
            .iter()
            .filter(|z| {
                let hay = z.to_lowercase().replace('_', " ");
                tokens.iter().all(|t| hay.contains(t))
            })
            .collect();
        let picked = if state.picked.is_empty() {
            zone
        } else {
            &state.picked
        };
        let shown: Vec<Json> = matching
            .iter()
            .take(ZONE_ROWS)
            .map(|z| {
                json!({"id": format!("do:zone-pick|{z}"), "label": z.replace('_', " "),
                    "selected": z.as_str() == picked})
            })
            .collect();
        let empty = if state.zones.is_empty() {
            tr(
                if self.can("time.zones") {
                    "bead.settings.zone-loading"
                } else {
                    "bead.settings.time-unavailable"
                },
                &[],
            )
        } else {
            tr("bead.settings.zone-none", &[])
        };
        let more = matching.len().saturating_sub(ZONE_ROWS);
        rows.push(json!({"id": "time.picker", "kind": "picker", "enabled": editable, "own": true,
            "label": tr("bead.settings.zone-pick", &[]),
            "placeholder": tr("bead.settings.zone-filter", &[]),
            "items": shown, "empty": empty,
            "more": if more > 0 { tr("bead.settings.zone-more", &[("count", &more.to_string())]) } else { String::new() }}));
        let can_set = editable
            && !state.picked.is_empty()
            && state.picked != zone
            && state.zones.contains(&state.picked)
            && self.can("time.set_timezone");
        rows.push(actions(
            "time.set",
            "",
            "",
            vec![opt(
                "do:zone-set".into(),
                &tr("bead.settings.zone-set", &[]),
                false,
                can_set,
            )],
            can_set,
        ));
        rows.push(help("time.auth", &tr("bead.settings.zone-auth", &[])));
        rows
    }

    fn history_rows(&self, editable: bool, state: &State) -> Vec<Json> {
        let Some(topic) = self.topic("notifications").filter(healthy) else {
            return vec![help("history", &tr("bead.settings.unavailable-now", &[]))];
        };
        let count = topic["history_count"].as_u64().unwrap_or_else(|| {
            topic["history"]["order"]
                .as_array()
                .map_or(0, |o| o.len() as u64)
        });
        let mut clear = self.clear_history_row(
            "history.clear",
            &tr("bead.settings.history-clear", &[]),
            "",
            state,
            editable,
        );
        // The count is the row's label; the button sits where a value would.
        clear["label"] = json!(tr(
            "bead.settings.history-count",
            &[("count", &count.to_string())]
        ));
        if count == 0 {
            clear["enabled"] = json!(false);
            clear["options"][0]["enabled"] = json!(false);
        }
        let mut rows = vec![clear];
        let mut shown = 0;
        for (id, notice) in entities(&topic["history"]).take(HISTORY_ROWS) {
            shown += 1;
            let summary = notice["summary"].as_str().unwrap_or("");
            let body = notice["body"].as_str().unwrap_or("");
            let app = notice["app_name"].as_str().unwrap_or("");
            let detail = if body.is_empty() {
                short(app, 40)
            } else {
                format!("{} · {}", short(app, 24), short(body, 80))
            };
            rows.push(info(
                &format!("history:{id}"),
                &short(summary, 60),
                &notice["created_ms"].as_u64().map(ago).unwrap_or_default(),
                &detail,
            ));
        }
        if shown == 0 {
            rows.push(help(
                "history.empty",
                &tr("bead.settings.history-empty", &[]),
            ));
        } else if topic["history_more"] == true || count > shown {
            rows.push(help("history.more", &tr("bead.settings.history-more", &[])));
        }
        rows
    }

    fn preset(&self) -> String {
        self.topic("settings.desktop")
            .and_then(|t| t["data"]["preset"].as_str().map(String::from))
            .filter(|p| PRESETS.iter().any(|(name, _)| name == p))
            .unwrap_or_else(|| "normal".into())
    }
    fn workflow_rows(&self, editable: bool, state: &State) -> Vec<Json> {
        let can = editable && self.can("settings.desktop");
        let preset = self.preset();
        let mut rows = vec![actions(
            "workflow.presets",
            &tr("bead.settings.presets", &[]),
            "",
            PRESETS
                .iter()
                .map(|(name, text)| {
                    opt(
                        format!("do:preset|{name}"),
                        &tr(text, &[]),
                        *name == preset,
                        can,
                    )
                })
                .collect(),
            can,
        )];
        rows.push(actions(
            "workflow.roles",
            &tr("bead.settings.workflow-setup", &[]),
            &tr("bead.settings.workflow-description", &[]),
            ROLES
                .iter()
                .map(|r| {
                    opt(
                        format!("do:role|{r}"),
                        r,
                        state.roles.iter().any(|x| x == r),
                        can,
                    )
                })
                .collect(),
            can,
        ));
        rows.push(actions(
            "workflow.save",
            "",
            "",
            vec![opt(
                "do:save-workflow".into(),
                &tr("bead.settings.workflow-save", &[]),
                false,
                can,
            )],
            can,
        ));
        if let Some(message) = self
            .topic("settings.desktop")
            .and_then(|t| t["message"].as_str().map(|m| short(m, 160)))
            .filter(|m| !m.is_empty())
        {
            rows.push(help("workflow.message", &message));
        }
        rows
    }

    fn addon_rows(&self) -> Vec<Json> {
        // QML offers an install button for each missing add-on; the installer is a host action.
        let flags = self.caps.borrow().clone();
        let missing = |flag: &str| {
            flags
                .as_ref()
                .is_some_and(|f| f.get(flag).and_then(Json::as_bool) != Some(true))
        };
        let mut rows = Vec::new();
        for (flag, label) in [
            ("ai", "bead.settings.addon-ai"),
            ("brain", "bead.settings.addon-brain"),
        ] {
            if missing(flag) {
                rows.push(requires(&format!("addon.{flag}"), &tr(label, &[])));
            }
        }
        rows
    }
    fn assistant_rows(&self) -> Vec<Json> {
        vec![
            json!({"id": "ai.sidebar", "kind": "link", "page": "panels/ai-sidebar",
                "label": tr("bead.settings.ai-sidebar", &[]), "description": ""}),
            requires("ai.open", &tr("bead.settings.ai-open", &[])),
            requires("ai.accounts", &tr("bead.settings.ai-accounts", &[])),
        ]
    }

    fn sound_rows(&self, editable: bool) -> Vec<Json> {
        let Some(audio) = self.topic("audio").filter(healthy) else {
            return vec![help("sound", &tr("bead.settings.unavailable-now", &[]))];
        };
        let mut rows = Vec::new();
        let groups = [
            (
                "output",
                "bead.settings.sound-output",
                "bead.settings.sound-none-devices",
            ),
            (
                "input",
                "bead.settings.sound-input",
                "bead.settings.sound-none-devices",
            ),
            (
                "stream",
                "bead.settings.sound-streams",
                "bead.settings.sound-none-streams",
            ),
        ];
        for (kind, title, none) in groups {
            rows.push(json!({"id": format!("sound:{kind}"), "kind": "section",
                "label": tr(title, &[]), "description": ""}));
            let mut any = false;
            for (id, node) in entities(&audio["nodes"]) {
                let node_kind = node["kind"].as_str().unwrap_or("");
                let stream = matches!(node_kind, "playback" | "capture");
                if (kind == "stream") != stream || (!stream && node_kind != kind) {
                    continue;
                }
                any = true;
                rows.push(self.node_row(&audio, id, node, stream, editable));
            }
            if !any {
                rows.push(help(&format!("sound:{kind}:none"), &tr(none, &[])));
            }
        }
        rows.push(requires(
            "sound.routing",
            &tr("bead.settings.sound-routing", &[]),
        ));
        rows
    }
    fn node_row(&self, audio: &Json, id: &str, node: &Json, stream: bool, editable: bool) -> Json {
        let ready = node["ready"] == true;
        let muted = node["readback"]["muted"]
            .as_bool()
            .unwrap_or(node["muted"] == true);
        let linear = node["readback"]["linear_volume"]
            .as_f64()
            .unwrap_or_else(|| node["volume"].as_f64().unwrap_or(0.));
        // The slider is perceptual: the cube root of the linear level.
        let percent = (linear.max(0.).cbrt() * 100.).round().clamp(0., 100.);
        let default = audio["default_output"] == id || audio["default_input"] == id;
        let writable = editable && ready && node_name(id);
        let mut buttons = Vec::new();
        if !stream {
            buttons.push(json!({"id": format!("do:default|{id}"),
                "label": tr("bead.settings.sound-default", &[]),
                "selected": default, "enabled": editable && ready && self.can("audio.set_default")}));
        }
        buttons.push(json!({"id": format!("do:mute|{id}"), "icon": if muted { "volume_off" } else { "volume_up" },
            "label": tr(if muted { "bead.settings.sound-unmute" } else { "bead.settings.sound-mute" }, &[]),
            "selected": muted, "enabled": writable && self.can("audio.set_mute")}));
        json!({"id": format!("node:{id}"), "kind": "device", "own": true, "enabled": writable,
            "title": node["name"].as_str().map(|n| short(n, 60)).unwrap_or_default(),
            "detail": "", "buttons": buttons,
            "slider": {"value": percent, "min": 0, "max": 100, "step": 5,
                "label": tr("bead.settings.sound-volume", &[]), "prefix": format!("do:vol|{id}|"),
                "text": if ready { format!("{percent}%") } else { "—".into() },
                "enabled": writable && self.can("audio.set_volume")}})
    }

    fn network_rows(&self, editable: bool, state: &State) -> Vec<Json> {
        let Some(net) = self.topic("network").filter(healthy) else {
            return vec![help("network", &tr("bead.settings.unavailable-now", &[]))];
        };
        let wifi = net["wireless_enabled"] == true;
        let mut rows = vec![on_off(
            "network.wifi",
            &tr("bead.settings.wifi", &[]),
            "",
            wifi,
            "wifi",
            editable && self.can("network.set_wifi"),
        )];
        if !wifi {
            rows.push(help("network.off", &tr("bead.settings.network-off", &[])));
            return rows;
        }
        rows.push(json!({"id": "network:available", "kind": "section",
            "label": tr("bead.settings.network-available", &[]), "description": ""}));
        let can_join = editable && self.can("network.connect");
        let mut any = false;
        for (id, ap) in entities(&net["access_points"]) {
            any = true;
            let active = ap["active"] == true;
            let name = ap["name"].as_str().unwrap_or("");
            let detail = if active {
                format!(
                    "{} · {}%",
                    tr("bead.settings.connected", &[]),
                    ap["strength"]
                )
            } else if ap["secured"] == true {
                format!("{} · {}%", tr("bead.settings.secured", &[]), ap["strength"])
            } else {
                format!("{}%", ap["strength"])
            };
            let connect = json!({"id": format!("do:net-connect|{id}"),
                "label": tr(if active { "bead.settings.connected" } else { "bead.settings.connect" }, &[]),
                "selected": active,
                "enabled": can_join && !active});
            rows.push(
                json!({"id": format!("ap:{id}"), "kind": "device", "own": true, "enabled": can_join,
                "title": short(name, 40), "detail": detail, "icon": "wifi", "buttons": [connect]}),
            );
            if state.joining == id {
                rows.push(json!({"id": "network.password", "kind": "secret", "own": true, "enabled": can_join,
                    "label": tr("bead.settings.network-password", &[("name", &short(name, 28))]),
                    "join": "do:net-join", "cancel": "do:cancel",
                    "join_label": tr("bead.settings.join", &[]),
                    "cancel_label": tr("bead.common.cancel", &[])}));
            }
        }
        if !any {
            rows.push(help("network.none", &tr("bead.settings.network-none", &[])));
        }
        let saved: Vec<(&str, &Json)> = entities(&net["connections"])
            .filter(|(_, c)| c["ssid"].is_string())
            .collect();
        if !saved.is_empty() {
            rows.push(json!({"id": "network:saved", "kind": "section",
                "label": tr("bead.settings.network-saved", &[]), "description": ""}));
            let can_forget = editable && self.can("network.forget");
            for (id, connection) in saved {
                let confirming = state.confirm == format!("forget:{id}");
                let button = if confirming {
                    json!({"id": format!("do:net-forget|{id}"),
                        "label": tr("bead.settings.forget-confirm", &[]), "selected": true, "enabled": can_forget})
                } else {
                    json!({"id": format!("do:confirm|forget:{id}"),
                        "label": tr("bead.settings.forget", &[]), "selected": false, "enabled": can_forget})
                };
                let mut buttons = vec![button];
                if confirming {
                    buttons.push(
                        json!({"id": "do:cancel", "label": tr("bead.common.cancel", &[]),
                        "selected": false, "enabled": true}),
                    );
                }
                rows.push(json!({"id": format!("saved:{id}"), "kind": "device", "own": true,
                    "enabled": can_forget, "title": short(connection["ssid"].as_str().unwrap_or(""), 40),
                    "detail": "", "icon": "wifi", "buttons": buttons}));
            }
        }
        rows.push(requires(
            "network.editor",
            &tr("bead.settings.network-editor", &[]),
        ));
        rows
    }

    fn bluetooth_rows(&self, editable: bool) -> Vec<Json> {
        let Some(bt) = self.topic("bluetooth").filter(healthy) else {
            return vec![help("bluetooth", &tr("bead.settings.unavailable-now", &[]))];
        };
        let mut rows = Vec::new();
        let can_power = editable && self.can("bluetooth.set_power");
        let can_scan = editable && self.can("bluetooth.set_discovery");
        for (id, adapter) in entities(&bt["adapters"]) {
            let on = adapter["powered"] == true;
            let name = adapter["name"].as_str().unwrap_or(id);
            rows.push(on_off(
                &format!("bt:{id}"),
                &short(name, 40),
                "",
                on,
                &format!("bt-power|{id}"),
                can_power,
            ));
            // `on_off` appends "|on"/"|off" to the op name; the adapter id sits before it.
            let scanning = adapter["discovering"] == true;
            rows.push(actions(
                &format!("bt-scan:{id}"),
                "",
                "",
                vec![opt(
                    format!("do:bt-scan|{id}|{}", if scanning { "off" } else { "on" }),
                    &tr(
                        if scanning {
                            "bead.settings.bt-stop"
                        } else {
                            "bead.settings.bt-scan"
                        },
                        &[],
                    ),
                    scanning,
                    can_scan && on,
                )],
                can_scan && on,
            ));
        }
        let mut any = false;
        for (id, device) in entities(&bt["devices"]) {
            any = true;
            let connected = device["connected"] == true;
            let paired = device["paired"] == true;
            let mut detail = Vec::new();
            if connected {
                detail.push(tr("bead.settings.connected", &[]));
            } else if paired {
                detail.push(tr("bead.settings.paired", &[]));
            }
            if let Some(battery) = device["battery"].as_u64() {
                detail.push(format!("{battery}%"));
            }
            let can = editable && paired && !(device["pairing"] == true);
            let (verb, label) = if connected {
                ("bt-disconnect", "bead.settings.disconnect")
            } else {
                ("bt-connect", "bead.settings.connect")
            };
            let method = if connected {
                "bluetooth.disconnect"
            } else {
                "bluetooth.connect"
            };
            rows.push(
                json!({"id": format!("btdev:{id}"), "kind": "device", "own": true,
                "enabled": can, "title": short(device["name"].as_str().unwrap_or(id), 48),
                "detail": detail.join(" · "), "icon": "bluetooth",
                "buttons": [{"id": format!("do:{verb}|{id}"), "label": tr(label, &[]),
                    "selected": connected, "enabled": can && self.can(method)}]}),
            );
        }
        if !any {
            rows.push(help("bluetooth.none", &tr("bead.settings.bt-none", &[])));
        }
        rows
    }

    // ---- calls -----------------------------------------------------------------------------

    /// Collect the reply of the call in flight and of the time zone list; start a queued level
    /// change. Returns the notice to show.
    pub(super) fn settle(&self) -> String {
        let mut queued = None;
        {
            let mut state = self.state.borrow_mut();
            if let Some(ticket) = &mut state.pending
                && let Some(reply) = ticket.try_take()
            {
                state.pending = None;
                match reply {
                    Ok(_) => {
                        state.message.clear();
                        state.notice = std::mem::take(&mut state.on_ok);
                    }
                    Err(e) => {
                        state.on_ok.clear();
                        state.message = match e.code.as_str() {
                            "stale_revision" => "bead.settings.changed-elsewhere",
                            "permission_denied" => "bead.settings.permission-denied",
                            _ if state.pending_write => "bead.settings.save-failed",
                            _ => "bead.settings.action-failed",
                        }
                        .into();
                    }
                }
                queued = state.queued.take();
            }
            if let Some(ticket) = &mut state.zones_fetch
                && let Some(reply) = ticket.try_take()
            {
                state.zones_fetch = None;
                if let Ok(reply) = reply {
                    state.zones = reply["zones"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Json::as_str)
                        .filter(|z| z.len() <= 100)
                        .take(MAX_ZONES)
                        .map(String::from)
                        .collect();
                }
            }
        }
        if let Some((method, params)) = queued
            && let Some(wake) = self.wake.borrow().clone()
        {
            self.call(&method, params, wake, "");
        }
        let state = self.state.borrow();
        if state.notice.is_empty() {
            String::new()
        } else {
            tr(&state.notice, &[])
        }
    }

    /// Start the one-time reads a page needs: the time zone list.
    pub(super) fn fetch(&self, page: &str) {
        if page != "date-time-language" {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.zones_asked || state.zones_fetch.is_some() {
            return;
        }
        let (Some((boot, timeout)), Some(client)) = (self.ready_for("time.zones"), &self.client)
        else {
            return;
        };
        state.zones_asked = true;
        if let Ok(ticket) = client.begin_call(
            "time.zones".into(),
            json!({}),
            timeout,
            CallGuard { boot, version: 1 },
            self.wake.borrow().clone(),
        ) {
            state.zones_fetch = Some(ticket);
        }
    }

    /// A level change: one call at a time, the newest target waits for the one in flight.
    fn level(&self, method: &str, params: Json, wake: CallWake) {
        if self.state.borrow().pending.is_some() {
            self.state.borrow_mut().queued = Some((method.into(), params));
        } else {
            self.call(method, params, wake, "");
        }
    }

    /// `do:<op>|<a>|<b>`: the buttons of the custom sections and action rows. Ids are
    /// re-checked against the published state, never trusted from the argument.
    pub(super) fn do_op(&self, payload: &str, wake: CallWake) -> BTreeMap<String, Value> {
        let mut parts = payload.splitn(3, '|');
        let (op, a, b) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        let mut changes = BTreeMap::new();
        match op {
            "confirm" => self.state.borrow_mut().confirm = a.into(),
            "cancel" => {
                let mut state = self.state.borrow_mut();
                state.confirm.clear();
                state.joining.clear();
                state.secret.clear();
                changes.insert("settings.secret".into(), Value::Text(String::new()));
            }
            "clear-history" => {
                let armed = std::mem::take(&mut self.state.borrow_mut().confirm) == "clear-history";
                if armed {
                    self.call("notifications.clear", json!({"scope": "history"}), wake, "");
                }
            }
            "lock" => self.call("session.request", json!({"action": "lock"}), wake, ""),
            "install-login" => {
                self.call(
                    "login.install_theme",
                    json!({}),
                    wake,
                    "bead.settings.login-installed",
                );
            }
            "ntp" if a == "on" || a == "off" => {
                self.call("time.set_ntp", json!({"enabled": a == "on"}), wake, "");
            }
            "zone-pick" if self.state.borrow().zones.iter().any(|z| z == a) => {
                self.state.borrow_mut().picked = a.into();
            }
            "zone-set" => {
                let zone = self.state.borrow().picked.clone();
                if self.state.borrow().zones.contains(&zone) {
                    self.call(
                        "time.set_timezone",
                        json!({"zone": zone}),
                        wake,
                        "bead.settings.zone-done",
                    );
                }
            }
            "clock" if CLOCKS.contains(&a) && self.document("desktop").2 => {
                let setting = json!({"store": {"id": "desktop", "key": ["clockFormat"]}});
                self.submit(&setting, json!(a), wake);
            }
            "preset" if PRESETS.iter().any(|(name, _)| *name == a) => {
                let rev = self.rev("desktop");
                self.call(
                    "settings.desktop",
                    json!({"arguments": ["preset", a], "rev": rev}),
                    wake,
                    "",
                );
            }
            "role" if ROLES.contains(&a) => {
                let mut state = self.state.borrow_mut();
                if let Some(at) = state.roles.iter().position(|r| r == a) {
                    state.roles.remove(at);
                } else {
                    state.roles.push(a.into());
                }
            }
            "save-workflow" => {
                let roles = serde_json::to_string(&self.state.borrow().roles).unwrap_or_default();
                let rev = self.rev("desktop");
                self.call(
                    "settings.desktop",
                    json!({"arguments": ["save-workflow", self.preset(), roles], "rev": rev}),
                    wake,
                    "",
                );
            }
            "vol" if node_name(a) => {
                if let Ok(percent) = b.parse::<f64>()
                    && percent.is_finite()
                {
                    let linear = (percent.clamp(0., 100.) / 100.).powi(3);
                    self.level(
                        "audio.set_volume",
                        self.node_target(a, json!({"value": linear})),
                        wake,
                    );
                }
            }
            "mute" if node_name(a) => {
                if let Some(muted) = self.topic("audio").and_then(|t| {
                    let node = t["nodes"]["items"].get(a)?.clone();
                    Some(
                        node["readback"]["muted"]
                            .as_bool()
                            .unwrap_or(node["muted"] == true),
                    )
                }) {
                    self.call(
                        "audio.set_mute",
                        self.node_target(a, json!({"muted": !muted})),
                        wake,
                        "",
                    );
                }
            }
            "default" if node_name(a) => {
                self.call("audio.set_default", json!({"node": a}), wake, "");
            }
            "wifi" if a == "on" || a == "off" => {
                self.call("network.set_wifi", json!({"enabled": a == "on"}), wake, "");
            }
            "net-connect" => self.net_connect(a, "", wake),
            "net-join" => {
                let joining = self.state.borrow().joining.clone();
                let secret = std::mem::take(&mut self.state.borrow_mut().secret);
                self.net_connect(&joining, &secret, wake);
                changes.insert("settings.secret".into(), Value::Text(String::new()));
                changes.insert("settings.reveal".into(), Value::Bool(false));
            }
            "reveal" => {
                let mut state = self.state.borrow_mut();
                state.reveal = !state.reveal;
                changes.insert("settings.reveal".into(), Value::Bool(state.reveal));
            }
            "net-forget" => {
                let armed =
                    std::mem::take(&mut self.state.borrow_mut().confirm) == format!("forget:{a}");
                if armed && self.known("network", "connections", a) {
                    self.call("network.forget", json!({"connection": a}), wake, "");
                }
            }
            "bt-power" if self.known("bluetooth", "adapters", a) && (b == "on" || b == "off") => {
                self.call(
                    "bluetooth.set_power",
                    json!({"adapter": a, "powered": b == "on"}),
                    wake,
                    "",
                );
            }
            "bt-scan" if self.known("bluetooth", "adapters", a) && (b == "on" || b == "off") => {
                self.call(
                    "bluetooth.set_discovery",
                    json!({"adapter": a, "discovering": b == "on"}),
                    wake,
                    "",
                );
            }
            "bt-connect" | "bt-disconnect" if self.known("bluetooth", "devices", a) => {
                let method = if op == "bt-connect" {
                    "bluetooth.connect"
                } else {
                    "bluetooth.disconnect"
                };
                self.call(method, json!({"device": a}), wake, "");
            }
            _ => {}
        }
        changes
    }

    /// Is `id` an entity of `topic.collection` right now.
    fn known(&self, topic: &str, collection: &str, id: &str) -> bool {
        self.topic(topic)
            .is_some_and(|t| t[collection]["items"].get(id).is_some())
    }

    /// The target of an audio call: the default device of the node's kind, else the node itself.
    fn node_target(&self, id: &str, mut params: Json) -> Json {
        let audio = self.topic("audio").unwrap_or(Json::Null);
        let target = if audio["default_output"] == id {
            json!({"target": "sink"})
        } else if audio["default_input"] == id {
            json!({"target": "source"})
        } else {
            json!({"target": "node", "node": id})
        };
        if let (Some(p), Some(t)) = (params.as_object_mut(), target.as_object()) {
            p.extend(t.clone());
        }
        params
    }

    /// Join an access point: open and saved ones at once, a secured new one asks for the password
    /// first. The password is sent once and never kept.
    fn net_connect(&self, id: &str, secret: &str, wake: CallWake) {
        let Some(net) = self.topic("network") else {
            return;
        };
        let Some(ap) = net["access_points"]["items"].get(id) else {
            return;
        };
        let device = ap["device"].clone();
        if !device.is_string() {
            return;
        }
        let saved = ap["connection"].is_string();
        if ap["secured"] == true && !saved && secret.is_empty() {
            let mut state = self.state.borrow_mut();
            state.joining = id.into();
            return;
        }
        let mut params = json!({"device": device, "target": {"kind": "access_point", "id": id}});
        if !secret.is_empty() {
            params["secret"] = json!(secret);
        }
        self.state.borrow_mut().joining.clear();
        self.call("network.connect", params, wake, "");
    }
}
