// SPDX-License-Identifier: GPL-3.0-only
//! B0063: the registry's custom sections (`customType`) and action rows. This is the Nacre edge of
//! the Settings window: it knows which topics and methods the published contracts (N0042, N0046,
//! N0047, N0049) give each section and turns them into generic rows (`info`, `actions`, `device`,
//! `picker`, `help`) that `shell/settings/main.luau` draws with kit components. B0070: the
//! contracts of N0051 (history clears, input store, shortcuts, status, tools, launches) replace
//! the "requires a Nacre contract" lines; a control with no contract left is still a `help` row.
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
const SHORTCUT_ROWS: usize = 150;
const SERVICE_ROWS: usize = 16;
const LAYOUT_ROWS: usize = 8;
const MAX_LAYOUTS: usize = 1024;
const LAYOUT_LIST: &str = "/usr/share/X11/xkb/rules/base.lst";
const MAX_LAYOUT_FILE: u64 = 4 << 20;
const MAX_LAYOUT_TEXT: usize = 160;

/// A history the Privacy page clears: the confirm/op name, the method and the texts.
struct History {
    op: &'static str,
    method: &'static str,
    params: fn() -> Json,
    confirm: &'static str,
    done: &'static str,
}
const HISTORIES: [History; 3] = [
    History {
        op: "clear-history",
        method: "notifications.clear",
        params: || json!({"scope": "history"}),
        confirm: "bead.settings.history-confirm",
        done: "",
    },
    History {
        op: "clear-clipboard",
        method: "clipboard.clear_history",
        params: || json!({}),
        confirm: "bead.settings.clipboard-confirm",
        done: "bead.settings.history-cleared",
    },
    History {
        op: "clear-launcher",
        method: "launcher.clear_history",
        params: || json!({}),
        confirm: "bead.settings.launcher-confirm",
        done: "bead.settings.history-cleared",
    },
];
/// A launch button: the closed set of what a Settings control may start, by key. The first is
/// the daemon method and its parameters, the second what to ask instead when the daemon
/// answers `unavailable` (the power service fix without `powerprofilesctl` opens the packages menu).
type Launch = (&'static str, Json, Option<(&'static str, Json)>);
fn launch(key: &str) -> Option<Launch> {
    let dispatch = |target: &str, method: &str, args: Json| json!({"target": target, "method": method, "args": args});
    Some(match key {
        "sidebar" => ("ui.dispatch", dispatch("nacre", "left", json!([])), None),
        "welcome" => (
            "ui.dispatch",
            dispatch("welcomeApp", "open", json!([])),
            None,
        ),
        "colors" => (
            "ui.dispatch",
            dispatch("colorsApp", "open", json!([])),
            None,
        ),
        "brain" => ("brain.open", json!({}), None),
        "power-service" => (
            "tools.launch",
            json!({"tool": key}),
            Some((
                "ui.dispatch",
                dispatch(
                    "nacre",
                    "menu",
                    json!(["software.packages", "power-profiles-daemon"]),
                ),
            )),
        ),
        "audio-manager" | "connection-editor" | "setup" | "ai-workspace" | "aur-review-policy" => {
            ("tools.launch", json!({"tool": key}), None)
        }
        _ => return None,
    })
}
/// The ids of the `! layout` section of an XKB `base.lst`, with their names.
fn parse_layouts(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(section) = line.strip_prefix('!') {
            inside = section.trim() == "layout";
        } else if inside && let Some((id, name)) = line.trim().split_once(char::is_whitespace) {
            let ok = (1..=16).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            if ok && out.len() < MAX_LAYOUTS {
                out.push((id.to_owned(), short(name, 60)));
            }
        }
    }
    out
}

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
pub(super) fn healthy(topic: &Json) -> bool {
    topic["health"]["available"] == true
}
pub(super) fn short(text: &str, chars: usize) -> String {
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
pub(super) fn opt(arg: String, label: &str, selected: bool, enabled: bool) -> Json {
    json!({"id": arg, "label": label, "selected": selected, "enabled": enabled})
}
pub(super) fn actions(
    id: &str,
    label: &str,
    description: &str,
    options: Vec<Json>,
    enabled: bool,
) -> Json {
    json!({"id": id, "kind": "actions", "label": label, "description": description,
        "options": options, "enabled": enabled, "own": true})
}
/// A drop-down row (`NacreSelect`): `options` are `opt` entries, `text` shows the chosen one.
pub(super) fn select(id: &str, label: &str, options: Vec<Json>, enabled: bool) -> Json {
    let text = options
        .iter()
        .find(|o| o["selected"] == true)
        .map_or(Json::from(""), |o| o["label"].clone());
    json!({"id": id, "kind": "select", "label": label, "description": "", "text": text,
        "options": options, "enabled": enabled, "own": true})
}
pub(super) fn help(id: &str, label: &str) -> Json {
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
            "displays.devices" => (self.display_rows(state), topic("displays")),
            "sound.devices" => (self.sound_rows(editable), topic("audio")),
            "network.devices" => (self.network_rows(editable, state), topic("network")),
            "bluetooth.devices" => (self.bluetooth_rows(editable), topic("bluetooth")),
            "system.health" => (self.status_rows(), vec![]),
            "keyboard-mouse.shortcuts" => (self.shortcut_rows(), topic("shortcuts")),
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
                self.clear_history_row(&HISTORIES[0], id, &label, &description, state, editable)
            }
            "clipboard-history" => {
                self.clear_history_row(&HISTORIES[1], id, &label, &description, state, editable)
            }
            "launcher-history" => {
                self.clear_history_row(&HISTORIES[2], id, &label, &description, state, editable)
            }
            "power-refresh" => actions(
                id,
                &label,
                &description,
                vec![opt(
                    "do:refresh-caps".into(),
                    &label,
                    false,
                    self.can("capabilities.refresh"),
                )],
                self.can("capabilities.refresh"),
            ),
            "power-fix" => self.launch_row(id, &label, &description, "power-service"),
            "setup" => self.launch_row(id, &label, &description, "setup"),
            "brain" => self.launch_row(id, &label, &description, "brain"),
            "welcome" => self.launch_row(id, &label, &description, "welcome"),
            "colors-studio" => self.launch_row(id, &label, &description, "colors"),
            "aur-review-policy" => self.aur_row(id, &label),
            // Bead has no updates view (B0068): the route is refused, so there is no button.
            "updates" => return None,
            // Bead previews its own lock; the idle policy and the font list have no Bead page.
            "lock-preview" | "idle-policy" | "fonts" => return None,
            _ => requires(id, &label),
        };
        row["id"] = json!(id);
        Some(row)
    }

    /// A button that starts one of the closed set of `launch` targets; the daemon method must be
    /// advertised.
    fn launch_row(&self, id: &str, label: &str, description: &str, key: &str) -> Json {
        let can = launch(key).is_some_and(|(method, ..)| self.can(method));
        let mut row = actions(
            id,
            "",
            description,
            vec![opt(format!("do:open|{key}"), label, false, can)],
            can,
        );
        row["own"] = json!(true);
        row
    }
    /// The AUR review policy: the daemon's provenance when `system.status` has it, and the
    /// button that opens the policy view.
    fn aur_row(&self, id: &str, label: &str) -> Json {
        let policy = self
            .state
            .borrow()
            .status
            .as_ref()
            .map(|s| s["aur_review"].clone())
            .filter(Json::is_object)
            .map(|p| {
                format!(
                    "{} · {} · {}",
                    short(p["mode"].as_str().unwrap_or(""), 32),
                    short(p["source"].as_str().unwrap_or(""), 64),
                    short(p["profile"].as_str().unwrap_or(""), 64)
                )
            })
            .unwrap_or_default();
        self.launch_row(id, label, &policy, "aur-review-policy")
    }

    fn clear_history_row(
        &self,
        history: &History,
        id: &str,
        label: &str,
        description: &str,
        state: &State,
        editable: bool,
    ) -> Json {
        let can = editable && self.can(history.method);
        if state.confirm == history.op {
            return actions(
                id,
                label,
                description,
                vec![
                    opt(
                        format!("do:{}", history.op),
                        &tr(history.confirm, &[]),
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
            vec![opt(
                format!("do:confirm|{}", history.op),
                &tr("bead.settings.history-clear", &[]),
                false,
                can,
            )],
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
            &HISTORIES[0],
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
        // QML offers an install button for each missing add-on; `nacre setup` runs in a terminal.
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
                rows.push(self.launch_row(&format!("addon.{flag}"), &tr(label, &[]), "", "setup"));
            }
        }
        rows
    }
    fn assistant_rows(&self) -> Vec<Json> {
        vec![
            json!({"id": "ai.sidebar", "kind": "link", "page": "panels/ai-sidebar",
                "label": tr("bead.settings.ai-sidebar", &[]), "description": ""}),
            self.launch_row("ai.open", &tr("bead.settings.ai-open", &[]), "", "sidebar"),
            self.launch_row(
                "ai.accounts",
                &tr("bead.settings.ai-accounts", &[]),
                "",
                "ai-workspace",
            ),
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
        rows.push(self.launch_row(
            "sound.routing",
            &tr("bead.settings.sound-routing", &[]),
            "",
            "audio-manager",
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
        rows.push(self.launch_row(
            "network.editor",
            &tr("bead.settings.network-editor", &[]),
            "",
            "connection-editor",
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
            let trusted = device["trusted"] == true;
            if let Some(battery) = device["battery"].as_u64() {
                detail.push(format!("{battery}%"));
            }
            let idle = editable && !(device["pairing"] == true);
            let can = idle && paired;
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
                "enabled": idle, "title": short(device["name"].as_str().unwrap_or(id), 48),
                "detail": detail.join(" · "), "icon": "bluetooth",
                "buttons": [{"id": format!("do:{verb}|{id}"), "label": tr(label, &[]),
                    "selected": connected, "enabled": can && self.can(method)},
                    {"id": format!("do:bt-trust|{id}|{}", if trusted { "off" } else { "on" }),
                    "label": tr(if trusted { "bead.settings.untrust" } else { "bead.settings.trust" }, &[]),
                    "selected": trusted, "enabled": idle && self.can("bluetooth.set_trusted")}]}),
            );
        }
        if !any {
            rows.push(help("bluetooth.none", &tr("bead.settings.bt-none", &[])));
        }
        rows
    }

    // ---- N0051 pages: shortcuts, status, input store ------------------------------------------

    /// `software-input` settings with an adapter in the registry that `settings.set` now writes.
    pub(super) fn input_adapter(setting: &Json) -> bool {
        setting["store"]["id"] == "software-input"
            && matches!(
                setting["adapter"].as_str(),
                Some("keyboard-layout" | "touchpad-enabled")
            )
    }

    fn shortcut_rows(&self) -> Vec<Json> {
        let Some(topic) = self.topic("shortcuts").filter(healthy) else {
            return vec![help("shortcuts", &tr("bead.settings.shortcuts-none", &[]))];
        };
        let mut rows: Vec<Json> = entities(&topic["items"])
            .take(SHORTCUT_ROWS)
            .map(|(id, item)| {
                let key = short(item["key"].as_str().unwrap_or(""), 64);
                let description = short(item["description"].as_str().unwrap_or(""), 120);
                let label = if description.is_empty() {
                    key.clone()
                } else {
                    description
                };
                info(&format!("key:{id}"), &label, &key, "")
            })
            .collect();
        let total = topic["items"]["order"].as_array().map_or(0, Vec::len);
        if rows.is_empty() {
            rows.push(help("shortcuts", &tr("bead.settings.shortcuts-none", &[])));
        } else if total > rows.len() {
            rows.push(help(
                "shortcuts.more",
                &tr("bead.settings.shortcuts-more", &[]),
            ));
        }
        rows
    }

    /// The System page's read-only status (`system.status`, read when the page opens).
    fn status_rows(&self) -> Vec<Json> {
        let state = self.state.borrow();
        let Some(status) = &state.status else {
            let text = if self.can("system.status") {
                "bead.settings.status-loading"
            } else {
                "bead.settings.status-unavailable"
            };
            return vec![help("system.health", &tr(text, &[]))];
        };
        let text = |v: &Json, max| short(v.as_str().unwrap_or(""), max);
        let line = |id: &str, label: &str, value: String, detail: &str| {
            info(&format!("status.{id}"), &tr(label, &[]), &value, detail)
        };
        let mut rows = vec![line(
            "release",
            "bead.settings.status-release",
            status["release"]
                .as_str()
                .map(|r| short(&r.chars().take(12).collect::<String>(), 12))
                .unwrap_or_else(|| tr("bead.settings.status-unknown", &[])),
            "",
        )];
        if let Some(services) = status["services"].as_object() {
            let mut names: Vec<&String> = services.keys().collect();
            names.sort();
            for name in names.into_iter().take(SERVICE_ROWS) {
                let unit = short(name.trim_end_matches(".service"), 48);
                rows.push(info(
                    &format!("status.service:{unit}"),
                    &unit,
                    &text(&services[name], 32),
                    "",
                ));
            }
        }
        let errors = text(&status["config_errors"], 200);
        rows.push(line(
            "config",
            "bead.settings.status-config",
            if errors.is_empty() {
                tr("bead.settings.status-config-ok", &[])
            } else {
                errors
            },
            "",
        ));
        let drift: Vec<&str> = status["drift"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .collect();
        rows.push(line(
            "drift",
            "bead.settings.status-drift",
            drift.len().to_string(),
            &drift
                .iter()
                .take(3)
                .map(|d| short(d, 60))
                .collect::<Vec<_>>()
                .join(", "),
        ));
        rows.push(line(
            "sync",
            "bead.settings.status-sync",
            status["brain_sync"]
                .as_str()
                .map(|s| short(s, 120))
                .unwrap_or_else(|| tr("bead.settings.status-none", &[])),
            "",
        ));
        if let Some(event) = status["session_event"].as_str() {
            rows.push(line(
                "event",
                "bead.settings.status-event",
                short(event, 120),
                "",
            ));
        }
        if let Some(sample) = status["performance"].as_object() {
            let number = |k: &str| sample.get(k).and_then(Json::as_f64).unwrap_or(0.);
            rows.push(line(
                "sample",
                "bead.settings.status-sample",
                tr(
                    "bead.settings.status-sample-value",
                    &[
                        ("cpu", &format!("{:.0}", number("cpu_core_percent"))),
                        ("memory", &format!("{:.0}", number("memory_mib"))),
                    ],
                ),
                "",
            ));
        }
        if let Some(age) = status["updates_age_seconds"].as_u64() {
            rows.push(line(
                "updates",
                "bead.settings.status-updates",
                ago(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64)
                    .saturating_sub(age.saturating_mul(1000))),
                "",
            ));
        }
        rows
    }

    /// Rows that follow a registry setting: the keyboard layout picker.
    pub(super) fn after_row(&self, setting: &Json, editable: bool, state: &State) -> Vec<Json> {
        if setting["adapter"] != "keyboard-layout" || !Self::input_adapter(setting) {
            return Vec::new();
        }
        let can = editable && self.document("software-input").2 && self.can(super::METHOD);
        let current = self.layout_list();
        let tokens: Vec<String> = state
            .pick
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let matching: Vec<&(String, String)> = state
            .layouts
            .iter()
            .filter(|(id, name)| {
                let hay = format!("{id} {}", name.to_lowercase());
                tokens.iter().all(|t| hay.contains(t))
            })
            .collect();
        let shown: Vec<Json> = matching
            .iter()
            .take(LAYOUT_ROWS)
            .map(|(id, name)| {
                json!({"id": format!("do:layout|{id}"), "label": format!("{name} ({id})"),
                    "selected": current.contains(id)})
            })
            .collect();
        let more = matching.len().saturating_sub(LAYOUT_ROWS);
        let empty = if state.layouts.is_empty() {
            tr("bead.settings.layout-unavailable", &[])
        } else {
            tr("bead.settings.layout-none", &[])
        };
        vec![
            json!({"id": "layout.picker", "kind": "picker", "enabled": can,
                "label": tr("bead.settings.layout-pick", &[]),
                "placeholder": tr("bead.settings.layout-filter", &[]),
                "items": shown, "empty": empty,
                "more": if more > 0 { tr("bead.settings.zone-more", &[("count", &more.to_string())]) } else { String::new() }}),
            actions(
                "layout.reset",
                "",
                "",
                vec![opt(
                    "do:layout-reset".into(),
                    &tr("bead.settings.layout-reset", &[]),
                    false,
                    can && !current.is_empty(),
                )],
                can && !current.is_empty(),
            ),
        ]
    }
    /// The info row of the layout setting: the chosen ids, or the system default.
    pub(super) fn layout_row(&self, setting: &Json, mut row: Json) -> Json {
        let list = self.layout_list().join(", ");
        row["kind"] = "info".into();
        row["value"] = if list.is_empty() {
            tr("bead.settings.default-value", &[])
        } else {
            list
        }
        .into();
        row["own"] = json!(true);
        row["id"] = setting["id"].clone();
        row
    }
    /// The layout ids of the `software-input` store, in order.
    fn layout_list(&self) -> Vec<String> {
        let (_, data, _) = self.document("software-input");
        data["keyboardLayouts"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    }
    /// Add or remove one known layout; an empty list is the system default.
    fn toggle_layout(&self, id: &str, wake: CallWake) {
        if !self.state.borrow().layouts.iter().any(|(l, _)| l == id) {
            return;
        }
        let mut list = self.layout_list();
        if let Some(at) = list.iter().position(|l| l == id) {
            list.remove(at);
        } else {
            list.push(id.into());
        }
        let joined = list.join(",");
        if joined.len() <= MAX_LAYOUT_TEXT {
            self.set_layouts(&joined, wake);
        } else {
            self.state.borrow_mut().message = "bead.settings.layout-too-long".into();
        }
    }
    fn set_layouts(&self, value: &str, wake: CallWake) {
        let Some(setting) = self
            .registry
            .setting("keyboard-mouse.keyboard.keyboardLayouts")
        else {
            return;
        };
        if self.registry.editable("keyboard-mouse") && self.document("software-input").2 {
            self.submit(setting, json!(value), wake);
        }
    }

    // ---- theme targets (B0064) ---------------------------------------------------------------

    /// A registry row Bead can drive with `colors.set_target`: the `app-theme` adapter, with the
    /// method advertised. (`settings.set colors appThemes.<id>` only writes the store; it does not
    /// re-theme the target.)
    pub(super) fn app_theme(&self, setting: &Json) -> bool {
        setting["adapter"] == "app-theme" && self.can("colors.set_target")
    }
    /// One target: On / Off buttons and the published status (`current`, `not-published`,
    /// `tool-missing`, `paused`; unknown until `colors.targets` answers).
    pub(super) fn target_row(&self, setting: &Json, mut row: Json) -> Json {
        let key = setting["key"].as_str().unwrap_or("");
        let status = self
            .state
            .borrow()
            .targets
            .iter()
            .find(|(id, _)| id == key)
            .map(|(_, status)| status.clone());
        let text = match status.as_deref() {
            Some("current") => "bead.settings.target-current",
            Some("not-published") => "bead.settings.target-not-published",
            Some("tool-missing") => "bead.settings.target-tool-missing",
            Some("paused") => "bead.settings.target-paused",
            _ => "bead.settings.target-unknown",
        };
        let enabled = row["enabled"] == true && status.is_some();
        let on = self.current(setting) == json!(true);
        let mut out = on_off(
            row["id"].as_str().unwrap_or(""),
            row["label"].as_str().unwrap_or(""),
            &tr(text, &[]),
            on,
            &format!("app-theme|{key}"),
            enabled,
        );
        out["own"] = json!(false);
        if let (Some(r), Some(o)) = (row.as_object_mut(), out.as_object()) {
            r.extend(o.clone());
        }
        row
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
                if std::mem::take(&mut state.retarget) {
                    state.targets_asked = false;
                }
                let fallback = state.fallback.take();
                match reply {
                    Ok(_) => {
                        state.message.clear();
                        state.notice = std::mem::take(&mut state.on_ok);
                    }
                    Err(e) if e.code == "unavailable" && fallback.is_some() => {
                        state.on_ok.clear();
                        state.queued = fallback;
                    }
                    Err(e) => {
                        state.on_ok.clear();
                        state.message = match e.code.as_str() {
                            "stale_revision" => "bead.settings.changed-elsewhere",
                            "permission_denied" => "bead.settings.permission-denied",
                            "busy" => "bead.settings.displays-busy",
                            _ if state.pending_write => "bead.settings.save-failed",
                            _ => "bead.settings.action-failed",
                        }
                        .into();
                    }
                }
                queued = state.queued.take();
            }
            if let Some(ticket) = &mut state.targets_fetch
                && let Some(reply) = ticket.try_take()
            {
                state.targets_fetch = None;
                if let Ok(reply) = reply {
                    state.targets = reply["targets"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(64)
                        .filter_map(|t| {
                            Some((
                                t["id"].as_str()?.to_owned(),
                                t["status"].as_str()?.to_owned(),
                            ))
                        })
                        .collect();
                }
            }
            if let Some(ticket) = &mut state.status_fetch
                && let Some(reply) = ticket.try_take()
            {
                state.status_fetch = None;
                if let Ok(reply) = reply {
                    state.status = Some(reply).filter(Json::is_object);
                }
            }
            if let Some(ticket) = &mut state.shortcuts_fetch
                && ticket.try_take().is_some()
            {
                state.shortcuts_fetch = None;
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

    /// Start the reads a page needs: the time zone list, the theme target statuses, the system
    /// status, the shortcuts refresh and the keyboard layout list.
    pub(super) fn fetch(&self, page: &str) {
        match page {
            "colors/app-theming" => self.fetch_targets(),
            "date-time-language" => {
                let mut state = self.state.borrow_mut();
                if !state.zones_asked && state.zones_fetch.is_none() {
                    state.zones_fetch = self.begin_read("time.zones");
                    state.zones_asked = state.zones_fetch.is_some();
                }
            }
            "system" => {
                let mut state = self.state.borrow_mut();
                if !state.status_asked && state.status_fetch.is_none() {
                    state.status_fetch = self.begin_read("system.status");
                    state.status_asked = state.status_fetch.is_some();
                }
            }
            "keyboard-mouse" => {
                let mut state = self.state.borrow_mut();
                // The compositor config is not watched: the list is asked for when the page opens.
                if !state.shortcuts_asked && state.shortcuts_fetch.is_none() {
                    state.shortcuts_fetch = self.begin_read("shortcuts.refresh");
                    state.shortcuts_asked = state.shortcuts_fetch.is_some();
                }
                if !state.layouts_asked {
                    state.layouts_asked = true;
                    state.layouts = std::fs::File::open(LAYOUT_LIST)
                        .and_then(|f| {
                            let mut text = String::new();
                            std::io::Read::read_to_string(
                                &mut std::io::Read::take(f, MAX_LAYOUT_FILE),
                                &mut text,
                            )
                            .map(|_| parse_layouts(&text))
                        })
                        .unwrap_or_default();
                }
            }
            _ => {}
        }
    }

    /// Begin a read-only call with no parameters, if the daemon advertises it.
    fn begin_read(&self, method: &str) -> Option<CallTicket> {
        let ((boot, timeout), client) = (self.ready_for(method)?, self.client.as_ref()?);
        client
            .begin_call(
                method.into(),
                json!({}),
                timeout,
                CallGuard { boot, version: 1 },
                self.wake.borrow().clone(),
            )
            .ok()
    }

    fn fetch_targets(&self) {
        let mut state = self.state.borrow_mut();
        if state.targets_asked || state.targets_fetch.is_some() {
            return;
        }
        state.targets_fetch = self.begin_read("colors.targets");
        state.targets_asked = state.targets_fetch.is_some();
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
        if let Some(history) = HISTORIES.iter().find(|h| h.op == op) {
            let armed = std::mem::take(&mut self.state.borrow_mut().confirm) == op;
            if armed {
                self.call(history.method, (history.params)(), wake, history.done);
            }
            return changes;
        }
        match op {
            "confirm" => self.state.borrow_mut().confirm = a.into(),
            "cancel" => {
                let mut state = self.state.borrow_mut();
                state.confirm.clear();
                state.joining.clear();
                state.secret.clear();
                changes.insert("settings.secret".into(), Value::Text(String::new()));
            }
            "open" if self.state.borrow().pending.is_none() => {
                if let Some((method, params, fallback)) = launch(a) {
                    self.call(method, params, wake, "");
                    let mut state = self.state.borrow_mut();
                    state.fallback = fallback
                        .filter(|_| state.pending.is_some())
                        .map(|(m, p)| (m.to_owned(), p));
                }
            }
            "disp-primary" | "disp-arrange" | "disp-scale" | "disp-mode" | "disp-keep"
            | "disp-revert" => self.display_op(op, a, b, wake),
            "refresh-caps" => self.call("capabilities.refresh", json!({}), wake, ""),
            "layout" => self.toggle_layout(a, wake),
            "layout-reset" => self.set_layouts("", wake),
            "bt-trust" if self.known("bluetooth", "devices", a) && (b == "on" || b == "off") => {
                self.call(
                    "bluetooth.set_trusted",
                    json!({"device": a, "trusted": b == "on"}),
                    wake,
                    "",
                );
            }
            "app-theme"
                if (b == "on" || b == "off")
                    && self.state.borrow().targets.iter().any(|(id, _)| id == a) =>
            {
                self.call(
                    "colors.set_target",
                    json!({"id": a, "enabled": b == "on"}),
                    wake,
                    "",
                );
                let mut state = self.state.borrow_mut();
                state.retarget = state.pending.is_some();
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

#[cfg(test)]
mod tests {
    #[test]
    fn layout_list_is_the_layout_section_of_base_lst() {
        let text = "! model\n  pc105  Generic\n\n! layout\n  us   English (US)\n  no   Norwegian\n  BAD! x\n\n! variant\n  nodeadkeys no: Norwegian (no dead keys)\n";
        assert_eq!(
            super::parse_layouts(text),
            [
                ("us".to_string(), "English (US)".to_string()),
                ("no".into(), "Norwegian".into())
            ]
        );
    }
}
