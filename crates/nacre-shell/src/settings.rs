// SPDX-License-Identifier: GPL-3.0-only
//! B0045 Settings window adapter. Nacre's settings registry (data) plus the published
//! store contract (`settings.store.<id>` topics, `settings.set {store,key,value,rev}`)
//! become generic form rows; `shell/settings/*.luau` and the bead-ui form kit draw them.
//! A new page or widget is registry data or Luau, never Rust: this file only knows
//! value types (boolean, number, enum, string, array). Nothing is resident until the
//! window opens, and the registry, store leases and subscriptions go with it.
mod pages;
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

/// Registry files are read from the shell bundle when the window opens and dropped on close,
/// so they cost nothing in the binary or while the window is closed.
const REGISTRY_DIR: &str = "settings/registry";
const MAX_REGISTRY_FILE: u64 = 1 << 20;
pub const METHOD: &str = "settings.set";
const MAX_TEXT: usize = 200;
const MAX_ITEMS: usize = 64;
const MAX_RESULTS: usize = 40;
/// Followed while the window is open: the registry's capability flags, and the two topics that
/// answer the `bluetooth` and `battery` conditions the flags topic does not carry.
const ALWAYS: &[&str] = &["capabilities", "bluetooth", "battery"];

pub struct Registry {
    navigation: Json,
    settings: Json,
    english: BTreeMap<String, String>,
    editable: Vec<String>,
    /// Array settings shown as a reorderable list (also lifts `editorOwned`).
    ordered: Vec<String>,
    default_page: String,
}
impl Registry {
    pub fn load(root: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let read = |name: &str| -> Result<Vec<u8>, String> {
            let path = root.join(REGISTRY_DIR).join(name);
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .and_then(|f| f.take(MAX_REGISTRY_FILE + 1).read_to_end(&mut bytes))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if bytes.len() as u64 > MAX_REGISTRY_FILE {
                return Err(format!("{}: too large", path.display()));
            }
            Ok(bytes)
        };
        let parse = |name: &str| {
            serde_json::from_slice::<Json>(&read(name)?).map_err(|e| format!("{name}: {e}"))
        };
        let overlay = parse("bead.json")?;
        Ok(Self {
            navigation: parse("navigation.json")?,
            settings: parse("settings.json")?,
            english: serde_json::from_slice(&read("en.json")?).map_err(|e| e.to_string())?,
            editable: overlay["editable"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            ordered: overlay["orderedLists"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            default_page: overlay["default"].as_str().unwrap_or("").into(),
        })
    }
    fn label(&self, id: &str) -> String {
        self.english.get(id).cloned().unwrap_or_else(|| tr(id, &[]))
    }
    fn text(&self, node: &Json, key: &str) -> String {
        node[key]
            .as_str()
            .map(|id| self.label(id))
            .unwrap_or_default()
    }
    fn array<'a>(&'a self, node: &'a Json, key: &str) -> impl Iterator<Item = &'a Json> {
        node[key].as_array().into_iter().flatten()
    }
    fn order(node: &Json) -> i64 {
        node["order"].as_i64().unwrap_or(0)
    }
    /// Top-level pages by group, in registry order.
    fn groups(&self) -> Vec<(&Json, Vec<&Json>)> {
        let mut groups: Vec<&Json> = self.array(&self.navigation, "groups").collect();
        groups.sort_by_key(|g| Self::order(g));
        groups
            .into_iter()
            .map(|group| {
                let mut pages: Vec<&Json> = self
                    .array(&self.navigation, "pages")
                    .filter(|p| p["group"] == group["id"] && p.get("parent").is_none())
                    .collect();
                pages.sort_by_key(|p| Self::order(p));
                (group, pages)
            })
            .collect()
    }
    fn page(&self, id: &str) -> Option<&Json> {
        self.array(&self.navigation, "pages")
            .find(|p| p["id"] == id)
    }
    fn first_page(&self) -> String {
        if self.page(&self.default_page).is_some() {
            return self.default_page.clone();
        }
        self.groups()
            .iter()
            .find_map(|(_, pages)| pages.first().and_then(|p| p["id"].as_str()))
            .unwrap_or("")
            .into()
    }
    fn setting(&self, id: &str) -> Option<&Json> {
        self.array(&self.settings, "settings")
            .find(|s| s["id"] == id)
    }
    /// A page is editable only when the Bead overlay lists it (or its top-level parent).
    fn editable(&self, page: &str) -> bool {
        self.editable
            .iter()
            .any(|p| page == p || page.starts_with(&format!("{p}/")))
    }
}

/// Dotted-path read of a store document.
fn dig<'a>(data: &'a Json, key: &[&str]) -> Option<&'a Json> {
    key.iter().try_fold(data, |at, k| at.get(*k))
}
fn key_of(setting: &Json) -> Vec<&str> {
    setting["store"]["key"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .collect()
}
fn number_step(setting: &Json) -> (f64, f64, f64) {
    let range = &setting["range"];
    (
        range["min"].as_f64().unwrap_or(0.),
        range["max"].as_f64().unwrap_or(100.),
        range["step"].as_f64().filter(|s| *s > 0.).unwrap_or(1.),
    )
}
fn hex_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 255 },
    ])
}
fn clean_text(text: &str) -> Option<String> {
    let text = text.trim();
    (text.chars().count() <= MAX_TEXT && !text.chars().any(char::is_control)).then(|| text.into())
}

#[derive(Default)]
struct State {
    pending: Option<CallTicket>,
    /// The pending call is a `settings.set` write (decides the failure wording).
    pending_write: bool,
    message: String,
    editing: String,
    /// A value for a setting that needs an explicit apply.
    staged: Option<(String, Json)>,
    /// Page-local choices of the custom sections (`pages.rs`): armed destructive action, picked
    /// time zone, chosen workflow roles, the time zone list and the call that fetches it.
    confirm: String,
    picked: String,
    roles: Vec<String>,
    zones: Vec<String>,
    zones_fetch: Option<CallTicket>,
    zones_asked: bool,
    /// The text of the picker's filter field, as of the last projection.
    pick: String,
    /// What the last finished call wants said (not an error), cleared by the next action.
    notice: String,
    /// The notice id the call in flight leaves when it succeeds.
    on_ok: String,
    /// The newest level change that waited for the call in flight.
    queued: Option<(String, Json)>,
    /// The access point whose password is being asked, the typed password (never traced or kept
    /// past the join) and whether it is shown.
    joining: String,
    secret: String,
    reveal: bool,
}
type Wake = Arc<dyn Fn() + Send + Sync>;
type Opened = BTreeMap<String, (Arc<dyn Source>, Option<Subscription>)>;
pub struct Settings {
    client: Option<Client>,
    /// Fixed topic sources (by topic name) for previews and tests; they replace the daemon topics.
    fixtures: BTreeMap<String, Arc<dyn Source>>,
    registry: Registry,
    open: RefCell<Opened>,
    wake: RefCell<Option<Wake>>,
    state: RefCell<State>,
    /// The `capabilities` flags of the last projection (`None`: the daemon does not publish them).
    caps: RefCell<Option<Json>>,
}

impl Settings {
    pub fn new(client: Option<Client>, root: &std::path::Path) -> Result<Self, String> {
        Ok(Self {
            client,
            fixtures: Default::default(),
            registry: Registry::load(root)?,
            open: Default::default(),
            wake: Default::default(),
            state: Default::default(),
            caps: Default::default(),
        })
    }
    /// The time zone list the daemon would have returned (previews and tests).
    #[cfg(test)]
    pub fn with_zones(self, zones: &[&str]) -> Self {
        {
            let mut state = self.state.borrow_mut();
            state.zones = zones.iter().map(|z| z.to_string()).collect();
            state.zones_asked = true;
        }
        self
    }
    #[cfg(test)]
    pub fn with_sources(mut self, fixtures: BTreeMap<String, Arc<dyn Source>>) -> Self {
        self.fixtures = fixtures;
        self
    }
    /// The password field's text for the next action; it lives in the `settings.secret` store.
    pub fn secret(&self, text: &str) {
        let mut state = self.state.borrow_mut();
        if state.secret != text {
            state.secret = text.into();
        }
    }
    /// Store changes wake the window; sources opened later subscribe with the same wake.
    pub fn subscribe(&self, wake: Wake) {
        for (source, subscription) in self.open.borrow_mut().values_mut() {
            *subscription = Self::follow(source, &wake);
        }
        *self.wake.borrow_mut() = Some(wake);
    }
    fn follow(source: &Arc<dyn Source>, wake: &Wake) -> Option<Subscription> {
        let wake = wake.clone();
        source.subscribe(Arc::new(move |_| wake())).ok()
    }
    fn ready(&self) -> Option<(String, Duration)> {
        self.ready_for(METHOD)
    }
    /// The daemon's advertised method at v1 on a live link, with the boot it was seen on and
    /// its deadline (a polkit prompt may take up to the protocol's 120 s).
    fn ready_for(&self, method: &str) -> Option<(String, Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 120_000)),
        ))
    }
    fn can(&self, method: &str) -> bool {
        self.ready_for(method).is_some()
    }
    /// Keep exactly the topics the shown rows read; everything else is released.
    fn retain(&self, wanted: &[String]) {
        let mut open = self.open.borrow_mut();
        open.retain(|topic, _| wanted.contains(topic));
        for topic in wanted {
            if open.contains_key(topic) {
                continue;
            }
            let source: Option<Arc<dyn Source>> = match (self.fixtures.get(topic), &self.client) {
                (Some(fixed), _) => Some(fixed.clone()),
                (None, Some(client)) => match client.topic(topic) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(_) => client.lease_topic(topic).ok().map(|t| Arc::new(t) as _),
                },
                (None, None) => None,
            };
            if let Some(source) = source {
                let subscription = self
                    .wake
                    .borrow()
                    .as_ref()
                    .and_then(|wake| Self::follow(&source, wake));
                open.insert(topic.clone(), (source, subscription));
            }
        }
    }
    /// The data of a followed topic while it is live and an object.
    fn topic(&self, name: &str) -> Option<Json> {
        let source = self.open.borrow().get(name).map(|(s, _)| s.clone())?;
        let snapshot = source.snapshot();
        (snapshot.connected && snapshot.data.is_object()).then(|| (*snapshot.data).clone())
    }
    /// `(rev, data, readable)` of a store document.
    fn document(&self, store: &str) -> (u64, Json, bool) {
        let Some(source) = self
            .open
            .borrow()
            .get(&format!("settings.store.{store}"))
            .map(|(s, _)| s.clone())
        else {
            return (0, Json::Null, false);
        };
        let snapshot = source.snapshot();
        let doc = &*snapshot.data;
        let ok = snapshot.connected
            && doc.get("store").and_then(Json::as_str) == Some(store)
            && doc.get("error").is_none_or(Json::is_null);
        (doc["rev"].as_u64().unwrap_or(0), doc["data"].clone(), ok)
    }
    fn current(&self, setting: &Json) -> Json {
        let (_, data, _) = self.document(setting["store"]["id"].as_str().unwrap_or(""));
        let default = setting["default"].clone();
        let value = dig(&data, &key_of(setting))
            .cloned()
            .unwrap_or(default.clone());
        let fits = match setting["type"].as_str() {
            Some("boolean") => value.is_boolean(),
            Some("integer" | "number") => value.is_number(),
            Some("string") => value.is_string(),
            Some("array") => value.is_array(),
            Some("enum") => setting["options"]
                .as_array()
                .is_some_and(|o| o.iter().any(|o| o["value"] == value)),
            _ => false,
        };
        if fits { value } else { default }
    }
    fn depends_on(&self, setting: &Json) -> bool {
        match setting["dependsOn"]
            .as_str()
            .and_then(|id| self.registry.setting(id))
        {
            Some(parent) => self.current(parent) == json!(true),
            None => true,
        }
    }
    /// Re-read the `capabilities` flags from the followed topic.
    fn refresh_caps(&self) {
        let flags = self
            .topic("capabilities")
            .map(|t| t["flags"].clone())
            .filter(Json::is_object);
        *self.caps.borrow_mut() = flags;
    }
    /// A flag of the `capabilities` topic. A flag the topic does not name is false (as QML), except
    /// `bluetooth` and `battery`, which the registry names but the topic does not carry (a Nacre
    /// ask): they come from the topics themselves.
    fn flag(&self, flags: &Json, name: &str) -> bool {
        if let Some(value) = flags.get(name).and_then(Json::as_bool) {
            return value;
        }
        let healthy = |t: &Json| t["health"]["available"] == true;
        match name {
            "bluetooth" => self.topic("bluetooth").is_some_and(|t| {
                healthy(&t)
                    && t["adapters"]["items"]
                        .as_object()
                        .is_some_and(|a| !a.is_empty())
            }),
            "battery" => self.topic("battery").is_some_and(|t| healthy(&t)),
            _ => false,
        }
    }
    fn holds(&self, flags: &Json, condition: &Json) -> bool {
        if let Some(name) = condition["capability"].as_str() {
            return self.flag(flags, name);
        }
        if let Some(all) = condition["all"].as_array() {
            return all.iter().all(|c| self.holds(flags, c));
        }
        if let Some(any) = condition["any"].as_array() {
            return any.iter().any(|c| self.holds(flags, c));
        }
        if let Some(not) = condition.get("not") {
            return !self.holds(flags, not);
        }
        true
    }
    /// A registry `visibleWhen`/`enabledWhen` condition. No condition, or no published flags,
    /// allows everything.
    fn allowed(&self, condition: Option<&Json>) -> bool {
        match (condition, &*self.caps.borrow()) {
            (Some(condition), Some(flags)) => self.holds(flags, condition),
            _ => true,
        }
    }
    /// QML hides a `backend: "planned"` setting (and a section left empty) outside development.
    fn planned(setting: &Json) -> bool {
        setting["backend"] == "planned"
    }
    /// Rows the Settings app cannot edit as a plain control: helper adapters, object lists
    /// (launcher entries, startup apps) and editors only the QML shell has.
    fn unavailable(&self, setting: &Json) -> bool {
        let id = setting["id"].as_str().unwrap_or("");
        setting.get("adapter").is_some()
            || setting.get("backend").is_some()
            || matches!(setting["entityType"].as_str(), Some("entry" | "startup"))
            || (setting.get("editorOwned").is_some()
                && setting["type"] == "array"
                && !self.registry.ordered.iter().any(|o| o == id))
    }

    fn row(&self, setting: &Json, editable: bool, state: &State) -> Json {
        let id = setting["id"].as_str().unwrap_or("");
        let label = self.registry.text(setting, "labelId");
        let description = self.registry.text(setting, "descriptionId");
        let mut row = json!({"id": id, "label": label, "description": description,
            "enabled": editable && self.depends_on(setting) && self.allowed(setting.get("enabledWhen")),
            "editing": state.editing == id});
        if self.unavailable(setting) {
            row["kind"] = "help".into();
            row["description"] = "".into();
            row["label"] = format!(
                "{label}: {}",
                if setting["unavailableId"].is_string() {
                    self.registry.text(setting, "unavailableId")
                } else {
                    tr("bead.settings.not-here", &[])
                }
            )
            .into();
            return row;
        }
        let value = self.current(setting);
        // Staged values show as the pending choice until applied or discarded.
        let staged = state
            .staged
            .as_ref()
            .filter(|(s, _)| s == id)
            .map(|(_, v)| v.clone());
        let shown = staged.clone().unwrap_or(value);
        row["staged"] = staged.is_some().into();
        row["apply"] = (setting["apply"] == "confirm").into();
        match setting["type"].as_str() {
            Some("boolean") => {
                row["kind"] = "toggle".into();
                row["checked"] = (shown == json!(true)).into();
            }
            Some(kind @ ("integer" | "number")) => {
                let (min, max, step) = number_step(setting);
                row["kind"] = "number".into();
                row["number"] = shown.clone();
                row["text"] = shown
                    .as_f64()
                    .map(|n| format!("{n}"))
                    .unwrap_or_default()
                    .into();
                row["integer"] = (kind == "integer").into();
                row["min"] = json!(min);
                row["max"] = json!(max);
                row["step"] = json!(step);
                row["unit"] = setting["unit"].as_str().unwrap_or("").into();
                row["at_min"] = (shown.as_f64().unwrap_or(min) <= min).into();
                row["at_max"] = (shown.as_f64().unwrap_or(max) >= max).into();
            }
            Some("enum") => {
                row["kind"] = "choice".into();
                let options: Vec<Json> = self
                    .registry
                    .array(setting, "options")
                    .enumerate()
                    .map(|(i, o)| {
                        json!({"id": format!("{id}|{i}"), "label": self.registry.text(o, "labelId"),
                            "selected": o["value"] == shown})
                    })
                    .collect();
                row["options"] = options.into();
            }
            Some("string") => {
                let text = shown.as_str().unwrap_or("").to_owned();
                row["kind"] = "text".into();
                row["text"] = if text.is_empty() {
                    tr("bead.settings.default-value", &[])
                } else {
                    text.clone()
                }
                .into();
                if let Some(color) = hex_color(&text) {
                    row["swatch"] = json!(color);
                }
            }
            Some("array") => {
                row["kind"] = "list".into();
                let items: Vec<Json> = shown
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(MAX_ITEMS)
                    .enumerate()
                    .map(|(i, v)| {
                        json!({"id": format!("{id}|rm:{i}"), "up": format!("{id}|up:{i}"),
                            "down": format!("{id}|down:{i}"),
                            "label": v.as_str().map(String::from).unwrap_or_else(|| v.to_string())})
                    })
                    .collect();
                row["reorder"] = self.registry.ordered.iter().any(|o| o == id).into();
                row["items"] = items.into();
            }
            _ => {
                row["kind"] = "help".into();
            }
        }
        row
    }

    fn page_items(&self, page: &str, state: &State) -> (Vec<Json>, Vec<String>) {
        let registry = &self.registry;
        let editable = registry.editable(page);
        let mut items = Vec::new();
        let mut topics: Vec<String> = Vec::new();
        let mut children: Vec<&Json> = registry
            .array(&registry.navigation, "pages")
            .filter(|p| p["parent"] == page && self.allowed(p.get("visibleWhen")))
            .collect();
        children.sort_by_key(|p| Registry::order(p));
        for child in children {
            let id = child["id"].as_str().unwrap_or("");
            items.push(json!({"id": format!("link:{id}"), "kind": "link", "page": id,
                "label": registry.text(child, "labelId"), "description": registry.text(child, "descriptionId")}));
        }
        let mut sections: Vec<&Json> = registry
            .array(&registry.settings, "sections")
            .filter(|s| s["page"] == page && self.allowed(s.get("visibleWhen")))
            .collect();
        sections.sort_by_key(|s| Registry::order(s));
        for section in &sections {
            let sid = section["id"].as_str().unwrap_or("");
            let mut body = Vec::new();
            for setting in registry
                .array(&registry.settings, "settings")
                .filter(|s| s["page"] == page && s["section"].as_str() == Some(sid))
                .filter(|s| !Self::planned(s) && self.allowed(s.get("visibleWhen")))
            {
                if let Some(store) = setting["store"]["id"].as_str()
                    && !self.unavailable(setting)
                {
                    let topic = format!("settings.store.{store}");
                    if !topics.contains(&topic) {
                        topics.push(topic);
                    }
                }
                body.push(self.row(setting, editable, state));
            }
            if section.get("customType").is_some() {
                match self.custom_rows(section, editable, state) {
                    Some((rows, read)) => {
                        body.extend(rows);
                        for topic in read {
                            if !topics.contains(&topic) {
                                topics.push(topic);
                            }
                        }
                    }
                    None if body.is_empty() => body.push(json!({"id": format!("custom:{sid}"),
                        "kind": "help", "label": tr("bead.settings.custom-unavailable", &[])})),
                    None => {}
                }
            }
            for (rows, read) in self.extra_rows(page, sid, editable, state) {
                body.push(rows);
                for topic in read {
                    if !topics.contains(&topic) {
                        topics.push(topic);
                    }
                }
            }
            let mut actions: Vec<&Json> = registry
                .array(&registry.settings, "actions")
                .filter(|a| a["page"] == page && a["section"].as_str() == Some(sid))
                .filter(|a| !Self::planned(a) && self.allowed(a.get("visibleWhen")))
                .collect();
            actions.sort_by_key(|a| Registry::order(a));
            for action in actions {
                if let Some(row) = self.action_row(action, editable, state) {
                    body.push(row);
                }
            }
            if body.is_empty() {
                continue;
            }
            items.push(json!({"id": format!("section:{sid}"), "kind": "section",
                "label": registry.text(section, "labelId"), "description": registry.text(section, "descriptionId")}));
            items.extend(body);
        }
        (items, topics)
    }

    fn search(&self, query: &str) -> Vec<Json> {
        let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let registry = &self.registry;
        let matches = |hay: &str| {
            let hay = hay.to_lowercase();
            tokens.iter().all(|t| hay.contains(t))
        };
        let page_title = |id: &str| {
            registry
                .page(id)
                .map(|p| registry.text(p, "labelId"))
                .unwrap_or_default()
        };
        let mut out = Vec::new();
        for page in registry
            .array(&registry.navigation, "pages")
            .filter(|p| self.allowed(p.get("visibleWhen")))
        {
            let hay = format!(
                "{} {}",
                registry.text(page, "labelId"),
                registry.text(page, "descriptionId")
            );
            if matches(&hay) {
                let id = page["id"].as_str().unwrap_or("");
                out.push(json!({"id": format!("page:{id}"), "kind": "link", "page": id,
                    "label": registry.text(page, "labelId"), "description": registry.text(page, "descriptionId")}));
            }
        }
        let shown = |page: &str| {
            registry
                .page(page)
                .is_none_or(|p| self.allowed(p.get("visibleWhen")))
        };
        for setting in registry.array(&registry.settings, "settings").filter(|s| {
            !Self::planned(s)
                && self.allowed(s.get("visibleWhen"))
                && shown(s["page"].as_str().unwrap_or(""))
        }) {
            let label = registry.text(setting, "labelId");
            let synonyms = registry
                .array(setting, "synonyms")
                .filter_map(Json::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            let hay = format!(
                "{label} {} {synonyms} {}",
                registry.text(setting, "descriptionId"),
                setting["key"].as_str().unwrap_or("")
            );
            if matches(&hay) {
                let id = setting["id"].as_str().unwrap_or("");
                out.push(json!({"id": format!("setting:{id}"), "kind": "link",
                    "page": setting["page"], "label": label,
                    "description": page_title(setting["page"].as_str().unwrap_or(""))}));
            }
        }
        for action in registry.array(&registry.settings, "actions").filter(|a| {
            !Self::planned(a)
                && self.allowed(a.get("visibleWhen"))
                && shown(a["page"].as_str().unwrap_or(""))
        }) {
            let label = registry.text(action, "labelId");
            let hay = format!("{label} {}", registry.text(action, "descriptionId"));
            if matches(&hay) {
                let id = action["id"].as_str().unwrap_or("");
                out.push(json!({"id": format!("action:{id}"), "kind": "link",
                    "page": action["page"], "label": label,
                    "description": page_title(action["page"].as_str().unwrap_or(""))}));
            }
        }
        out.truncate(MAX_RESULTS);
        out
    }

    /// The page shown for `settings.page`, or the registry default for anything unknown.
    pub fn page_for(&self, requested: &str) -> String {
        if self.registry.page(requested).is_some() {
            requested.into()
        } else {
            self.registry.first_page()
        }
    }

    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let text = |name: &str| match stores.get(name) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let page = self.page_for(&text("settings.page"));
        let query = text("settings.search");
        let searching = !query.trim().is_empty();
        let ready = self.ready().is_some();
        self.state.borrow_mut().pick = text("settings.pick.text");
        // Which topics the page reads decides the leases; rows are built after they open. The
        // capability flags and the two topics that stand in for missing flags stay followed.
        self.refresh_caps();
        let mut wanted = if searching {
            Vec::new()
        } else {
            self.page_items(&page, &self.state.borrow()).1
        };
        for base in ALWAYS {
            if !wanted.iter().any(|t| t == base) {
                wanted.push((*base).into());
            }
        }
        self.retain(&wanted);
        self.refresh_caps();
        let notice = self.settle();
        self.fetch(&page);
        let items = if searching {
            self.search(&query)
        } else {
            self.page_items(&page, &self.state.borrow()).0
        };
        let editable = self.registry.editable(&page);
        let readable = wanted
            .iter()
            .filter_map(|t| t.strip_prefix("settings.store."))
            .all(|store| self.document(store).2);
        let writable = ready && editable && readable;
        let mut message = String::new();
        {
            let state = self.state.borrow();
            if !state.message.is_empty() {
                message = tr(&state.message, &[]);
            }
        }
        let busy = self.state.borrow().pending.is_some();
        let mut nav = Vec::new();
        for (group, pages) in self.registry.groups() {
            let pages: Vec<&Json> = pages
                .into_iter()
                .filter(|p| self.allowed(p.get("visibleWhen")))
                .collect();
            if pages.is_empty() {
                continue;
            }
            nav.push(json!({"id": format!("group:{}", group["id"].as_str().unwrap_or("")),
                "kind": "group", "label": self.registry.text(group, "labelId"), "icon": "", "selected": false}));
            for p in pages {
                let id = p["id"].as_str().unwrap_or("");
                let selected = !searching && (page == id || page.starts_with(&format!("{id}/")));
                nav.push(
                    json!({"id": id, "kind": "page", "label": self.registry.text(p, "labelId"),
                    "icon": p["icon"].as_str().unwrap_or(""), "selected": selected}),
                );
            }
        }
        let current = self.registry.page(&page);
        let (title, description) = if searching {
            (tr("bead.settings.search-results", &[]), String::new())
        } else {
            current.map_or_else(Default::default, |p| {
                (
                    self.registry.text(p, "labelId"),
                    self.registry.text(p, "descriptionId"),
                )
            })
        };
        BTreeMap::from([
            ("settings.page".into(), Value::Text(page)),
            ("settings.nav".into(), Value::list(nav)),
            ("settings.items".into(), Value::list(items.clone())),
            ("settings.title".into(), Value::Text(title)),
            ("settings.description".into(), Value::Text(description)),
            ("settings.message".into(), Value::Text(message)),
            ("settings.noticed".into(), Value::Bool(!notice.is_empty())),
            ("settings.notice".into(), Value::Text(notice)),
            ("settings.writable".into(), Value::Bool(writable && !busy)),
            (
                "settings.readonly".into(),
                Value::Bool(!searching && !editable),
            ),
            ("settings.searching".into(), Value::Bool(searching)),
            ("settings.empty".into(), Value::Bool(items.is_empty())),
        ])
    }

    fn say(&self, id: &str) {
        self.state.borrow_mut().message = id.into();
    }

    /// One host action `settings.do`, argument `op:payload`. Returns store changes for
    /// the window (page, text field). Ids are re-resolved against the registry; the
    /// argument text never names a store, key or method.
    pub fn act(&self, arg: &str, edit_text: &str, wake: CallWake) -> BTreeMap<String, Value> {
        let (op, payload) = arg.split_once(':').unwrap_or((arg, ""));
        let none = BTreeMap::new();
        match op {
            "do" => return self.do_op(payload, wake),
            "page" => {
                {
                    let mut state = self.state.borrow_mut();
                    state.editing.clear();
                    state.confirm.clear();
                    state.picked.clear();
                    state.joining.clear();
                    state.secret.clear();
                    state.message.clear();
                    state.notice.clear();
                }
                return BTreeMap::from([
                    ("settings.page".into(), Value::Text(self.page_for(payload))),
                    ("settings.search".into(), Value::Text(String::new())),
                    ("settings.edit.text".into(), Value::Text(String::new())),
                    ("settings.pick.text".into(), Value::Text(String::new())),
                ]);
            }
            "cancel" => {
                let mut state = self.state.borrow_mut();
                state.editing.clear();
                state.staged = None;
                return BTreeMap::from([("settings.edit.text".into(), Value::Text(String::new()))]);
            }
            "discard" => {
                self.state.borrow_mut().staged = None;
                return none;
            }
            _ => {}
        }
        // Everything below writes: it needs the exact method, a connected daemon and
        // a readable store on an editable page.
        let (id, how) = payload.split_once('|').unwrap_or((payload, ""));
        let Some(setting) = self.registry.setting(id) else {
            return none;
        };
        let page = setting["page"].as_str().unwrap_or("");
        if self.unavailable(setting) || !self.registry.editable(page) || !self.depends_on(setting) {
            return none;
        }
        let store = setting["store"]["id"].as_str().unwrap_or("");
        let (_, _, readable) = self.document(store);
        if self.ready().is_none() || !readable {
            self.say("bead.settings.unavailable-now");
            return none;
        }
        let current = self.current(setting);
        if op == "edit" {
            self.state.borrow_mut().editing = id.into();
            let prefill = match current.as_str() {
                Some(text) => text.to_owned(),
                None => String::new(),
            };
            return BTreeMap::from([("settings.edit.text".into(), Value::Text(prefill))]);
        }
        let value: Option<Json> = match (op, setting["type"].as_str()) {
            ("apply", _) => self
                .state
                .borrow_mut()
                .staged
                .take()
                .filter(|(s, _)| s == id)
                .map(|(_, v)| v),
            ("set", Some("boolean")) => Some(json!(current != json!(true))),
            ("set", Some(kind @ ("integer" | "number"))) => {
                how.parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| {
                    let (min, max, step) = number_step(setting);
                    let snapped = (min + ((n - min) / step).round() * step).clamp(min, max);
                    if kind == "integer" {
                        json!(snapped.round() as i64)
                    } else {
                        json!(snapped)
                    }
                })
            }
            ("set", Some("enum")) => how
                .parse::<usize>()
                .ok()
                .and_then(|i| setting["options"].get(i))
                .map(|o| o["value"].clone()),
            ("set", Some("string")) => clean_text(edit_text).map(Json::String),
            ("set", Some("array")) => {
                let mut list = current.as_array().cloned().unwrap_or_default();
                if let Some(index) = how
                    .strip_prefix("rm:")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    (index < list.len()).then(|| {
                        list.remove(index);
                        Json::Array(list.clone())
                    })
                } else if let Some((from, to)) = how
                    .split_once(':')
                    .filter(|_| self.registry.ordered.iter().any(|o| o == id))
                    .and_then(|(dir, i)| Some((dir, i.parse::<usize>().ok()?)))
                    .and_then(|(dir, i)| match dir {
                        "up" => i.checked_sub(1).map(|j| (i, j)),
                        "down" => Some((i, i + 1)),
                        _ => None,
                    })
                {
                    (from < list.len() && to < list.len()).then(|| {
                        list.swap(from, to);
                        Json::Array(list.clone())
                    })
                } else if how == "add" && list.len() < MAX_ITEMS {
                    clean_text(edit_text).filter(|t| !t.is_empty()).map(|t| {
                        list.push(Json::String(t));
                        Json::Array(list.clone())
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(value) = value else { return none };
        if setting["apply"] == "confirm" && op != "apply" {
            self.state.borrow_mut().staged = Some((id.into(), value));
            return none;
        }
        self.submit(setting, value, wake);
        let mut changes = BTreeMap::new();
        if matches!(setting["type"].as_str(), Some("string" | "array")) {
            self.state.borrow_mut().editing.clear();
            changes.insert("settings.edit.text".into(), Value::Text(String::new()));
        }
        changes
    }

    fn submit(&self, setting: &Json, value: Json, wake: CallWake) {
        let (rev, _, _) = self.document(setting["store"]["id"].as_str().unwrap_or(""));
        // The revision fence: a stale revision is refused and nothing is replayed.
        let params = json!({"store": setting["store"]["id"], "key": setting["store"]["key"],
            "value": value, "rev": rev});
        self.call(METHOD, params, wake, "");
    }

    /// One daemon call at a time: the published method, its advertised deadline, the boot it was
    /// seen on. The reply is read by `settle` on a later projection.
    fn call(&self, method: &str, params: Json, wake: CallWake, ok: &str) {
        let mut state = self.state.borrow_mut();
        if state.pending.is_some() {
            return;
        }
        state.message.clear();
        state.notice.clear();
        let (Some((boot, timeout)), Some(client)) = (self.ready_for(method), &self.client) else {
            state.message = "bead.settings.unavailable-now".into();
            return;
        };
        match client.begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        ) {
            Ok(ticket) => {
                state.pending = Some(ticket);
                state.pending_write = method == METHOD;
                state.on_ok = ok.into();
            }
            Err(_) => state.message = "bead.settings.save-failed".into(),
        }
    }
}

/// The Settings window as a framework `WindowSpec`: one factory, one deep-link mapping.
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
    fallback.width = 800.;
    fallback.bounds = bead::render::Rect::new(0., 0., 800., 600.);
    let cold = fallback.clone();
    Ok(bead::wayland::WindowSpec {
        app_id: "nacre-settings".into(),
        title: tr("bead.settings.title", &[]),
        size: (960, 640),
        min_size: (560, 400),
        fallback,
        factory: Arc::new(move |_| {
            let session = bead::ui::Session::new(
                root.clone(),
                "settings/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::new(session)
                .with_trace(trace)
                .with_settings(client.clone(), &root)
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
        model: Arc::new(|args| {
            args.first()
                .and_then(Json::as_str)
                .map(|page| {
                    BTreeMap::from([
                        ("settings.page".into(), Value::Text(page.into())),
                        ("settings.search".into(), Value::Text(String::new())),
                    ])
                })
                .unwrap_or_default()
        }),
    })
}
