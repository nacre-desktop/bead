// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plugins v1 (spec 0203): a directory with `plugin.toml` plus Luau files.
//!
//! Discover → validate → enable/disable (persisted) → mount the enabled plugins'
//! Luau into bundles that opt in (`"plugins": true` in `bundle.json`) under
//! `plugins/<id>/`, next to a generated `plugins/index.luau` listing every
//! discovered plugin. Host functions check the declared capabilities of the
//! calling plugin; Luau sandboxing is a stability boundary, not a security one.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// Plugin API level this host implements. Levels only grow and stay cumulative.
pub const API_LEVEL: u32 = 1;
/// Oldest plugin API level this host still loads.
pub const API_MIN: u32 = 1;
/// Root-relative prefix of mounted plugin sources.
pub const MOUNT: &str = "plugins";
const MAX_PLUGINS: usize = 64;
const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Store names any plugin may read: framework interaction state only.
const FRAMEWORK_TOPICS: [&str; 6] = [
    "bead.focused",
    "bead.hovered",
    "bead.focus_visible",
    "bead.pressed",
    "bead.viewport.",
    "bead.preview.",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    pub api: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default, rename = "widget")]
    pub widgets: Vec<WidgetEntry>,
    #[serde(default)]
    pub settings: BTreeMap<String, Setting>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WidgetEntry {
    pub id: String,
    pub entry: PathBuf,
    /// Optional translation ids; the plugin name is the fallback title.
    #[serde(default)]
    pub title_id: Option<String>,
    #[serde(default)]
    pub description_id: Option<String>,
    /// Palette role the widget is drawn in by default (`primary`, `secondary`, ...).
    #[serde(default)]
    pub accent: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    pub default: serde_json::Value,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
}

/// A declared capability. Only `topic:` and `action:` have host functions at API 1;
/// `network`, `files` and `exec` are shown to the user and denied by omission.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Network,
    Files,
    Exec,
    Topic(String),
    Action(String),
}
impl Capability {
    pub fn parse(text: &str) -> Result<Self, String> {
        let name = |prefix: &str| {
            let valid = !prefix.is_empty()
                && prefix.len() <= 128
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
            valid
                .then(|| prefix.to_owned())
                .ok_or_else(|| format!("invalid capability {text}"))
        };
        match text {
            "network" => Ok(Self::Network),
            "files" => Ok(Self::Files),
            "exec" => Ok(Self::Exec),
            _ => match text.split_once(':') {
                Some(("topic", prefix)) => name(prefix).map(Self::Topic),
                Some(("action", prefix)) => name(prefix).map(Self::Action),
                _ => Err(format!("unknown capability {text}")),
            },
        }
    }
}

/// What the host lets one mounted plugin reach.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grants {
    pub topics: Vec<String>,
    pub actions: Vec<String>,
}
impl Grants {
    fn from(manifest: &Manifest) -> Self {
        let mut grants = Self::default();
        for capability in &manifest.capabilities {
            match Capability::parse(capability) {
                Ok(Capability::Topic(prefix)) => grants.topics.push(prefix),
                Ok(Capability::Action(prefix)) => grants.actions.push(prefix),
                _ => {}
            }
        }
        grants
    }
    pub fn topic(&self, name: &str) -> bool {
        FRAMEWORK_TOPICS.iter().any(|allowed| {
            name == *allowed || (allowed.ends_with('.') && name.starts_with(allowed))
        }) || self
            .topics
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
    }
    pub fn action(&self, name: &str) -> bool {
        self.actions
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}
fn relative(path: &Path) -> bool {
    path.components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
        && path.components().next().is_some()
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("invalid plugin id {:?}", self.id));
        }
        for (field, value) in [
            ("name", &self.name),
            ("version", &self.version),
            ("author", &self.author),
            ("description", &self.description),
            ("license", &self.license),
        ] {
            if value.len() > 512 || (matches!(field, "name" | "version") && value.is_empty()) {
                return Err(format!("invalid plugin {field}"));
            }
        }
        if !(API_MIN..=API_LEVEL).contains(&self.api) {
            return Err(format!(
                "plugin API {} is outside this host's range {API_MIN}..={API_LEVEL}",
                self.api
            ));
        }
        for capability in &self.capabilities {
            Capability::parse(capability)?;
        }
        if self.widgets.is_empty() {
            return Err("a plugin needs at least one [[widget]] entry".into());
        }
        let mut ids = BTreeSet::new();
        for widget in &self.widgets {
            let scoped = widget
                .id
                .strip_prefix(self.id.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'));
            if !scoped || widget.id.len() > 128 || !ids.insert(&widget.id) {
                return Err(format!(
                    "widget id {:?} must be unique and be the plugin id or <id>.<name>",
                    widget.id
                ));
            }
            if !relative(&widget.entry) || widget.entry.extension().is_none_or(|e| e != "luau") {
                return Err(format!(
                    "widget entry {} must be a relative .luau path",
                    widget.entry.display()
                ));
            }
        }
        for (key, setting) in &self.settings {
            let valid_key = !key.is_empty()
                && key.len() <= 64
                && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
            let (min, max) = (setting.min.unwrap_or(0.), setting.max.unwrap_or(0.));
            let default = &setting.default;
            let ok = valid_key
                && match setting.kind.as_str() {
                    "toggle" => default.is_boolean(),
                    "choice" => setting.options.as_ref().is_some_and(|options| {
                        !options.is_empty()
                            && options.len() <= 32
                            && default
                                .as_str()
                                .is_some_and(|d| options.iter().any(|o| o == d))
                    }),
                    "number" | "slider" => {
                        min.is_finite()
                            && max.is_finite()
                            && min <= max
                            && setting.min.is_some()
                            && setting.max.is_some()
                            && default.as_f64().is_some_and(|d| (min..=max).contains(&d))
                    }
                    "text" => default.as_str().is_some_and(|d| d.len() <= 4096),
                    _ => false,
                };
            if !ok {
                return Err(format!("invalid setting {key}"));
            }
        }
        Ok(())
    }
}

/// Where plugins and their state live. Paths are configuration, never Nacre-specific.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Bundled (first-party) plugin directories: enabled unless the user disabled them.
    pub system: Vec<PathBuf>,
    /// User plugin directory (local copies and git clones): disabled until enabled.
    pub user: Option<PathBuf>,
    /// State file (enabled overrides, settings, faults, update notices).
    pub state: Option<PathBuf>,
}
impl Config {
    /// Default layout for an application: `<root>/plugins`, the XDG data dir and XDG config file.
    pub fn for_app(root: &Path, app: &str) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg = |variable: &str, fallback: &str| {
            std::env::var_os(variable)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| home.as_ref().map(|home| home.join(fallback)))
        };
        Self {
            system: vec![key(root).join(MOUNT)],
            user: xdg("XDG_DATA_HOME", ".local/share").map(|d| d.join(app).join(MOUNT)),
            state: xdg("XDG_CONFIG_HOME", ".config").map(|d| d.join(app).join("plugins.json")),
        }
    }
    /// Paths a watcher should follow so enable/disable and plugin edits reload views.
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.system.clone();
        paths.extend(self.user.clone());
        paths.retain(|path| path.is_dir());
        paths.extend(self.state.clone());
        paths
    }
}
fn configs() -> &'static Mutex<BTreeMap<PathBuf, Config>> {
    static CONFIGS: std::sync::OnceLock<Mutex<BTreeMap<PathBuf, Config>>> =
        std::sync::OnceLock::new();
    CONFIGS.get_or_init(Default::default)
}
fn key(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}
/// Set the plugin configuration of one UI root (the shell calls this at startup).
pub fn configure(root: &Path, config: Config) {
    configs()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(key(root), config);
}
/// The configuration of a UI root; unconfigured roots only see `<root>/plugins`.
pub fn config(root: &Path) -> Config {
    let root = key(root);
    configs()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&root)
        .cloned()
        .unwrap_or_else(|| Config {
            system: vec![root.join(MOUNT)],
            ..Config::default()
        })
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub version: u32,
    /// Explicit user choices; absent ids use the origin default.
    pub enabled: BTreeMap<String, bool>,
    pub settings: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// A plugin that failed while loading or rendering stays off until re-enabled.
    pub faults: BTreeMap<String, String>,
    /// Git plugins with a newer upstream revision (checked on request only).
    pub updates: BTreeSet<String>,
    /// Last install/update outcome shown on the Plugins page.
    pub notice: String,
}
impl State {
    pub fn load(path: Option<&Path>) -> Self {
        let Some(bytes) = path.and_then(|path| fs::read(path).ok()) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_else(|error| Self {
            notice: format!("plugin state unreadable, using defaults: {error}"),
            ..Self::default()
        })
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("plugin state needs a parent directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut state = self.clone();
        state.version = 1;
        let bytes = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    }
}
/// Read-modify-write of the state file; serialised within this process.
pub fn update_state(config: &Config, change: impl FnOnce(&mut State)) -> Result<State, String> {
    static WRITER: Mutex<()> = Mutex::new(());
    let _guard = WRITER.lock().unwrap_or_else(|p| p.into_inner());
    let path = config
        .state
        .as_deref()
        .ok_or("no plugin state file configured")?;
    let mut state = State::load(Some(path));
    let before = state.clone();
    change(&mut state);
    if state != before {
        state.save(path)?;
    }
    Ok(state)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    System,
    User,
}
#[derive(Clone, Debug)]
pub struct Plugin {
    pub id: String,
    pub dir: PathBuf,
    pub origin: Origin,
    pub manifest: Result<Manifest, String>,
    pub enabled: bool,
    pub git: bool,
}
impl Plugin {
    /// Loaded into views: enabled, valid and not faulted.
    pub fn active(&self, state: &State) -> bool {
        self.enabled && self.manifest.is_ok() && !state.faults.contains_key(&self.id)
    }
}

/// Every plugin directory in the configured sources, valid or not, in id order.
pub fn discover(config: &Config, state: &State) -> Vec<Plugin> {
    let mut found: BTreeMap<String, Plugin> = BTreeMap::new();
    let sources = config
        .system
        .iter()
        .map(|dir| (dir, Origin::System))
        .chain(config.user.iter().map(|dir| (dir, Origin::User)));
    for (source, origin) in sources {
        let Ok(entries) = fs::read_dir(source) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        dirs.sort();
        for dir in dirs {
            if found.len() >= MAX_PLUGINS {
                break;
            }
            let id = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if id.starts_with('.') || !dir.join("plugin.toml").is_file() {
                continue;
            }
            let manifest = fs::read_to_string(dir.join("plugin.toml"))
                .map_err(|e| e.to_string())
                .and_then(|text| Manifest::parse(&text))
                .and_then(|manifest| {
                    if manifest.id != id {
                        return Err(format!("directory {id} holds plugin {}", manifest.id));
                    }
                    for widget in &manifest.widgets {
                        if !dir.join(&widget.entry).is_file() {
                            return Err(format!("missing entry {}", widget.entry.display()));
                        }
                    }
                    Ok(manifest)
                });
            if found.contains_key(&id) {
                found.insert(
                    format!("{id}@user"),
                    Plugin {
                        id: id.clone(),
                        dir,
                        origin,
                        manifest: Err(format!("{id} duplicates an installed plugin")),
                        enabled: false,
                        git: false,
                    },
                );
                continue;
            }
            let enabled = state
                .enabled
                .get(&id)
                .copied()
                .unwrap_or(origin == Origin::System);
            let git = dir.join(".git").exists();
            found.insert(
                id.clone(),
                Plugin {
                    id,
                    dir,
                    origin,
                    manifest,
                    enabled,
                    git,
                },
            );
        }
    }
    found.into_values().collect()
}

/// Mounted plugin sources and the grants of each active plugin.
#[derive(Default)]
pub struct Mount {
    pub sources: crate::frontend::Sources,
    pub grants: Arc<BTreeMap<String, Grants>>,
}
/// Collect the active plugins' Luau and the generated `plugins/index.luau`.
/// A plugin whose files cannot be read is listed with that error and not loaded.
pub fn mount(root: &Path) -> Mount {
    let config = config(root);
    let state = State::load(config.state.as_deref());
    let mut sources = crate::frontend::Sources::new();
    let mut grants = BTreeMap::new();
    let mut listed = Vec::new();
    for plugin in discover(&config, &state) {
        let mut error = plugin.manifest.as_ref().err().cloned();
        if error.is_none() && plugin.active(&state) {
            match read_luau(&plugin.dir) {
                Ok(files) => {
                    for (path, bytes) in files {
                        sources.insert(Path::new(MOUNT).join(&plugin.id).join(path), bytes);
                    }
                    if let Ok(manifest) = &plugin.manifest {
                        grants.insert(plugin.id.clone(), Grants::from(manifest));
                    }
                }
                Err(message) => error = Some(message),
            }
        }
        let fault = state.faults.get(&plugin.id).cloned();
        let manifest = plugin.manifest.as_ref().ok();
        let mut values = serde_json::Map::new();
        if let Some(manifest) = manifest {
            for (key, setting) in &manifest.settings {
                let saved = state.settings.get(&plugin.id).and_then(|s| s.get(key));
                values.insert(
                    key.clone(),
                    saved.cloned().unwrap_or_else(|| setting.default.clone()),
                );
            }
        }
        listed.push(serde_json::json!({
            "id": plugin.id,
            "name": manifest.map_or(plugin.id.as_str(), |m| m.name.as_str()),
            "version": manifest.map_or("", |m| m.version.as_str()),
            "author": manifest.map_or("", |m| m.author.as_str()),
            "description": manifest.map_or("", |m| m.description.as_str()),
            "api": manifest.map_or(0, |m| m.api),
            "origin": if plugin.origin == Origin::System { "bundled" } else { "user" },
            "git": plugin.git,
            "enabled": plugin.enabled,
            "loaded": error.is_none() && fault.is_none() && plugin.enabled,
            "error": error.or(fault).unwrap_or_default(),
            "update": state.updates.contains(&plugin.id),
            "capabilities": manifest.map_or(vec![], |m| m.capabilities.clone()),
            "settings": manifest.map_or(serde_json::Value::Null, |m| serde_json::to_value(&m.settings).unwrap_or_default()),
            "values": values,
            "widgets": manifest.map_or(vec![], |m| m.widgets.iter().map(|w| serde_json::json!({
                "id": w.id,
                "module": Path::new(MOUNT).join(&plugin.id).join(&w.entry).with_extension("").to_string_lossy(),
                "title_id": w.title_id.clone().unwrap_or_default(),
                "description_id": w.description_id.clone().unwrap_or_default(),
                "accent": w.accent.clone().unwrap_or_else(|| "primary".into()),
            })).collect()),
        }));
    }
    let index = serde_json::json!({
        "api": API_LEVEL,
        "user_dir": config.user.is_some(),
        "notice": state.notice,
        "plugins": listed,
    });
    sources.insert(
        Path::new(MOUNT).join("index.luau"),
        format!(
            "-- Generated by bead-ui plugins (spec 0203).\nreturn {}\n",
            luau_literal(&index)
        )
        .into_bytes(),
    );
    Mount {
        sources,
        grants: Arc::new(grants),
    }
}
fn read_luau(dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_dir) = pending.pop() {
        let entries = fs::read_dir(dir.join(&relative_dir)).map_err(|e| e.to_string())?;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = relative_dir.join(&name);
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "luau") {
                if files.len() >= MAX_FILES {
                    return Err(format!("plugin has more than {MAX_FILES} Luau files"));
                }
                let size = entry.metadata().map_err(|e| e.to_string())?.len();
                if size > MAX_FILE_BYTES {
                    return Err(format!("{} exceeds 256 KiB", path.display()));
                }
                files.push((path, fs::read(entry.path()).map_err(|e| e.to_string())?));
            }
        }
    }
    Ok(files)
}
/// A Luau table literal for JSON data (generated index sources).
fn luau_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "nil".into(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => {
            let mut out = String::from("\"");
            for byte in s.bytes() {
                match byte {
                    b'"' | b'\\' => {
                        out.push('\\');
                        out.push(byte as char);
                    }
                    0x20..=0x7e => out.push(byte as char),
                    _ => out.push_str(&format!("\\{byte:03}")),
                }
            }
            out.push('"');
            out
        }
        serde_json::Value::Array(items) => format!(
            "{{{}}}",
            items.iter().map(luau_literal).collect::<Vec<_>>().join(",")
        ),
        serde_json::Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!(
                    "[{}]={}",
                    luau_literal(&serde_json::json!(k)),
                    luau_literal(v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// The calling plugin of a mounted source file, if any.
pub fn owner(file: &str) -> Option<&str> {
    let rest = file.strip_prefix(MOUNT)?.strip_prefix('/')?;
    let (id, _) = rest.split_once('/')?;
    valid_id(id).then_some(id)
}

/// Record a load/render failure once: the plugin stays off until the user re-enables it.
static FAULTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
pub fn fault(root: &Path, id: &str, message: &str) {
    let message: String = message.chars().take(512).collect();
    if !FAULTED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.to_owned())
    {
        return;
    }
    eprintln!("PLUGIN_ERROR id={id} {message}");
    let config = config(root);
    if config.state.is_some() {
        let _ = update_state(&config, |state| {
            state.faults.insert(id.to_owned(), message);
        });
    }
}
/// Enable or disable a plugin. Enabling clears a recorded fault.
pub fn set_enabled(config: &Config, id: &str, enabled: bool) -> Result<(), String> {
    let state = State::load(config.state.as_deref());
    if !discover(config, &state).iter().any(|p| p.id == id) {
        return Err(format!("unknown plugin {id}"));
    }
    update_state(config, |state| {
        state.enabled.insert(id.to_owned(), enabled);
        if enabled {
            state.faults.remove(id);
        }
    })?;
    if enabled {
        FAULTED.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
    }
    Ok(())
}
/// Store a plugin-level setting after checking it against the manifest schema.
pub fn set_setting(
    config: &Config,
    id: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let state = State::load(config.state.as_deref());
    let plugin = discover(config, &state)
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("unknown plugin {id}"))?;
    let manifest = plugin.manifest?;
    let setting = manifest
        .settings
        .get(key)
        .ok_or_else(|| format!("unknown setting {key}"))?;
    let mut probe = setting.clone();
    probe.default = value.clone();
    let mut check = manifest.clone();
    check.settings = BTreeMap::from([(key.to_owned(), probe)]);
    check
        .validate()
        .map_err(|_| format!("invalid value for {key}"))?;
    update_state(config, |state| {
        state
            .settings
            .entry(id.to_owned())
            .or_default()
            .insert(key.to_owned(), value);
    })
    .map(drop)
}

fn user_dir(config: &Config) -> Result<&Path, String> {
    config
        .user
        .as_deref()
        .ok_or_else(|| "no user plugin directory configured".into())
}
fn git(args: &[&str], dir: Option<&Path>) -> Result<String, String> {
    let mut command = std::process::Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let output = command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("git unavailable: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("git failed")
            .to_owned())
    }
}
/// Move a staged plugin directory into the user dir under its manifest id. It starts disabled.
fn adopt(config: &Config, staged: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(staged.join("plugin.toml"))
        .map_err(|_| "no plugin.toml in that source".to_owned())
        .and_then(|text| Manifest::parse(&text))?;
    let state = State::load(config.state.as_deref());
    if discover(config, &state).iter().any(|p| p.id == manifest.id) {
        return Err(format!("plugin {} is already installed", manifest.id));
    }
    let target = user_dir(config)?.join(&manifest.id);
    fs::rename(staged, &target).map_err(|e| e.to_string())?;
    update_state(config, |state| {
        state.enabled.insert(manifest.id.clone(), false);
        state.notice = format!(
            "Installed {} {} (disabled)",
            manifest.name, manifest.version
        );
    })?;
    Ok(manifest.id)
}
fn staging(config: &Config) -> Result<PathBuf, String> {
    let dir = user_dir(config)?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let staged = dir.join(format!(".staging-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staged);
    Ok(staged)
}
/// Install from a git URL (shallow clone). Blocking: call it off the UI thread.
pub fn install_git(config: &Config, url: &str) -> Result<String, String> {
    let url = url.trim();
    let scheme = ["https://", "http://", "ssh://", "git@", "file://"]
        .iter()
        .any(|s| url.starts_with(s));
    if !scheme || url.len() > 2048 || url.chars().any(char::is_whitespace) {
        return Err("expected an https://, ssh://, git@ or file:// git URL".into());
    }
    let staged = staging(config)?;
    let staged_text = staged.to_string_lossy().into_owned();
    let result = git(
        &["clone", "--quiet", "--depth", "1", "--", url, &staged_text],
        None,
    )
    .and_then(|_| adopt(config, &staged));
    let _ = fs::remove_dir_all(&staged);
    result
}
/// Install a copy of a local plugin folder. Symlinks and oversized trees are refused.
pub fn install_local(config: &Config, source: &Path) -> Result<String, String> {
    if !source.is_absolute() || !source.join("plugin.toml").is_file() {
        return Err("expected an absolute path to a folder with plugin.toml".into());
    }
    let staged = staging(config)?;
    let mut count = 0usize;
    let mut pending = vec![PathBuf::new()];
    let copied = (|| {
        while let Some(relative_dir) = pending.pop() {
            fs::create_dir_all(staged.join(&relative_dir)).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(source.join(&relative_dir)).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = relative_dir.join(entry.file_name());
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                count += 1;
                if count > 512 {
                    return Err("plugin folder has more than 512 entries".to_owned());
                }
                if kind.is_symlink() {
                    return Err(format!("{} is a symlink", path.display()));
                } else if kind.is_dir() {
                    pending.push(path);
                } else if entry.metadata().map_err(|e| e.to_string())?.len() > 4 * MAX_FILE_BYTES {
                    return Err(format!("{} is too large", path.display()));
                } else {
                    fs::copy(entry.path(), staged.join(&path)).map_err(|e| e.to_string())?;
                }
            }
        }
        adopt(config, &staged)
    })();
    let _ = fs::remove_dir_all(&staged);
    copied
}
/// Fetch every git plugin and record which have upstream changes. Never applies them.
pub fn check_updates(config: &Config) -> Result<usize, String> {
    let state = State::load(config.state.as_deref());
    let mut newer = BTreeSet::new();
    for plugin in discover(config, &state).into_iter().filter(|p| p.git) {
        if git(&["fetch", "--quiet", "--depth", "1"], Some(&plugin.dir)).is_ok()
            && git(&["rev-parse", "HEAD"], Some(&plugin.dir))
                != git(&["rev-parse", "FETCH_HEAD"], Some(&plugin.dir))
        {
            newer.insert(plugin.id);
        }
    }
    let count = newer.len();
    update_state(config, |state| {
        state.updates = newer;
        state.notice = format!("{count} plugin update(s) available");
    })?;
    Ok(count)
}
/// Apply a fetched update to one git plugin (explicit user action).
pub fn apply_update(config: &Config, id: &str) -> Result<(), String> {
    let state = State::load(config.state.as_deref());
    let plugin = discover(config, &state)
        .into_iter()
        .find(|p| p.id == id && p.git && p.origin == Origin::User)
        .ok_or_else(|| format!("{id} is not a git plugin"))?;
    git(
        &["reset", "--quiet", "--hard", "FETCH_HEAD"],
        Some(&plugin.dir),
    )?;
    update_state(config, |state| {
        state.updates.remove(id);
        state.faults.remove(id);
        state.notice = format!("Updated {id}");
    })
    .map(drop)
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
