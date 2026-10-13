// SPDX-License-Identifier: MIT OR Apache-2.0
//! Replaceable typed views and bounded Luau behavior over durable Rust state.
pub mod script;
use bead_core::{Stores, Value};
use bead_render::{Palette, Primitive, Rect, Scene};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}
impl std::error::Error for Diagnostic {}
impl Diagnostic {
    pub fn new(path: &Path, line: usize, column: usize, message: impl ToString) -> Self {
        Self {
            file: path.to_string_lossy().into_owned(),
            line,
            column,
            message: message.to_string(),
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    #[serde(skip)]
    pub source: Option<(PathBuf, usize, usize)>,
    pub draw: Primitive,
    #[serde(default)]
    pub bind: BTreeMap<String, String>,
    pub visible: Option<String>,
    pub action: Option<String>,
    pub icon: Option<String>,
    pub input: Option<String>,
    pub scroll: Option<String>,
}
impl Node {
    fn diagnostic(&self, message: impl ToString) -> Diagnostic {
        let (path, line, column) =
            self.source
                .clone()
                .unwrap_or((PathBuf::from("ui/main.json"), 1, 1));
        Diagnostic::new(&path, line, column, message)
    }
}
fn locate_nodes(path: &Path, bytes: &[u8], nodes: &mut [Node]) {
    let source = String::from_utf8_lossy(bytes);
    let mut offset = 0;
    for node in nodes {
        for (start, _) in source[offset..].match_indices("\"id\"") {
            let start = offset + start;
            let rest = source[start + 4..]
                .trim_start()
                .strip_prefix(':')
                .map(str::trim_start);
            if let Some(rest) = rest {
                let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<String>();
                if stream
                    .next()
                    .is_some_and(|id| id.ok().as_ref() == Some(&node.id))
                {
                    let prefix = &source[..start];
                    node.source = Some((
                        path.to_path_buf(),
                        1 + prefix.bytes().filter(|&c| c == b'\n').count(),
                        1 + prefix
                            .rsplit('\n')
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .count(),
                    ));
                    offset = start + 4;
                    break;
                }
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    script: PathBuf,
    width: f32,
    bounds: Rect,
    #[serde(default)]
    includes: Vec<PathBuf>,
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fragment {
    nodes: Vec<Node>,
}
pub struct Generation {
    pub script: script::Script,
    pub nodes: Vec<Node>,
    pub width: f32,
    pub bounds: Rect,
    pub sources: BTreeMap<PathBuf, Vec<u8>>,
    pub revision: u64,
    assets: BTreeMap<PathBuf, std::sync::Arc<[u8]>>,
}
struct Loader {
    root: PathBuf,
    sources: BTreeMap<PathBuf, Vec<u8>>,
}
impl Loader {
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, Diagnostic> {
        let full = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|e| Diagnostic::new(path, 1, 1, e))?;
        if !full.starts_with(&self.root) {
            return Err(Diagnostic::new(path, 1, 1, "file escapes UI bundle"));
        }
        let bytes = fs::read(full).map_err(|e| Diagnostic::new(path, 1, 1, e))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Diagnostic::new(path, 1, 1, "UI source exceeds 1 MiB"));
        }
        self.sources.insert(path.to_path_buf(), bytes.clone());
        Ok(bytes)
    }
    fn json<T: serde::de::DeserializeOwned>(&mut self, path: &Path) -> Result<T, Diagnostic> {
        let bytes = self.read(path)?;
        serde_json::from_slice(&bytes).map_err(|e| Diagnostic::new(path, e.line(), e.column(), e))
    }
}
impl Generation {
    /// Entire candidate graph is parsed before live stores or views change.
    pub fn load(
        root: &Path,
        entry: &Path,
        stores: &Stores,
        revision: u64,
    ) -> Result<(Self, Stores), Diagnostic> {
        let normalized_entry = entry
            .components()
            .filter(|component| !matches!(component, std::path::Component::CurDir))
            .collect::<PathBuf>();
        let entry = normalized_entry.as_path();
        let mut loader = Loader {
            root: root
                .canonicalize()
                .map_err(|e| Diagnostic::new(entry, 1, 1, e))?,
            sources: BTreeMap::new(),
        };
        let doc: Document = loader.json(entry)?;
        if doc.version != 1
            || !doc.width.is_finite()
            || !(1. ..=20000.).contains(&doc.width)
            || !valid_rect(doc.bounds)
        {
            return Err(Diagnostic::new(
                entry,
                1,
                1,
                "invalid version or panel geometry",
            ));
        }
        // Parse every UI source before executing behavior, including files not yet imported.
        let mut directories = vec![entry.parent().unwrap_or(Path::new("")).to_path_buf()];
        let mut checked = 0;
        while let Some(directory) = directories.pop() {
            for item in fs::read_dir(loader.root.join(&directory))
                .map_err(|e| Diagnostic::new(&directory, 1, 1, e))?
            {
                let item = item.map_err(|e| Diagnostic::new(&directory, 1, 1, e))?;
                let path = directory.join(item.file_name());
                let kind = item
                    .file_type()
                    .map_err(|e| Diagnostic::new(&path, 1, 1, e))?;
                checked += 1;
                if checked > 128 {
                    return Err(Diagnostic::new(
                        &path,
                        1,
                        1,
                        "UI bundle exceeds 128 source entries",
                    ));
                }
                if kind.is_dir() {
                    directories.push(path);
                    continue;
                }
                if path == entry || path == Path::new("palette.json") {
                    continue;
                }
                match path.extension().and_then(|ext| ext.to_str()) {
                    Some("json") => {
                        let _: Fragment = loader.json(&path)?;
                    }
                    Some("luau") => {
                        let bytes = loader.read(&path)?;
                        script::Script::validate_source(&bytes, &path)?;
                    }
                    _ => {}
                }
            }
        }
        let mut nodes = doc.nodes;
        locate_nodes(entry, &loader.sources[entry], &mut nodes);
        let mut seen = BTreeSet::from([entry.to_path_buf()]);
        for path in &doc.includes {
            if !seen.insert(path.clone()) {
                return Err(Diagnostic::new(entry, 1, 1, "duplicate include"));
            }
            let mut fragment: Fragment = loader.json(path)?;
            locate_nodes(path, &loader.sources[path], &mut fragment.nodes);
            nodes.extend(fragment.nodes);
        }
        let script = script::Script::load(&loader.read(&doc.script)?, &doc.script)?;
        let mut defaults = script.schema()?;
        if defaults
            .insert("bead.focused".into(), Value::Text(String::new()))
            .is_some()
        {
            return Err(Diagnostic::new(
                &doc.script,
                1,
                1,
                "bead.focused is reserved",
            ));
        }
        let stores = stores
            .reconcile(&defaults)
            .map_err(|e| Diagnostic::new(&doc.script, 1, 1, e))?;
        let mut ids = BTreeSet::new();
        let mut assets = BTreeMap::new();
        if nodes.len() > 512 {
            return Err(Diagnostic::new(entry, 1, 1, "view exceeds 512 nodes"));
        }
        for node in &mut nodes {
            if node.id.is_empty() || node.id.len() > 128 || !ids.insert(node.id.clone()) {
                return Err(node.diagnostic(format!("invalid/duplicate view ID {}", node.id)));
            }
            let valid_shape = match &node.draw {
                Primitive::Rounded { radius, border, .. } => {
                    radius.is_finite()
                        && *radius >= 0.
                        && *radius <= 1024.
                        && border.as_ref().is_none_or(|(_, width)| {
                            width.is_finite() && *width >= 0. && *width <= 64.
                        })
                }
                Primitive::Text { size, text, .. } => {
                    size.is_finite() && *size >= 1. && *size <= 256. && text.len() <= 65536
                }
                Primitive::Shadow { radius, blur, .. } => {
                    radius.is_finite()
                        && *radius >= 0.
                        && *radius <= 1024.
                        && blur.is_finite()
                        && *blur >= 0.
                        && *blur <= 64.
                }
                Primitive::Image { .. } => true,
            };
            if !valid_shape || !valid_rect(rect(&node.draw)) {
                return Err(node.diagnostic(format!("invalid geometry for {}", node.id)));
            }
            if let Primitive::Image { path, .. } = &mut node.draw {
                let bytes = loader.read(path)?;
                bead_render::validate_asset(path, &bytes)
                    .map_err(|e| Diagnostic::new(path, 1, 1, e))?;
                *path = loader.root.join(&*path);
                assets.insert(path.clone(), std::sync::Arc::from(bytes));
            }
            for action in [&node.action, &node.scroll].into_iter().flatten() {
                script.validate_action(action)?;
            }
            if let Some(id) = &node.input
                && !matches!(stores.get(id), Some(Value::Text(_)))
            {
                return Err(Diagnostic::new(
                    entry,
                    1,
                    1,
                    format!("input {} needs a text store", node.id),
                ));
            }
        }
        script.validate_computed(&stores)?;
        let generation = Self {
            script,
            nodes,
            width: doc.width,
            bounds: doc.bounds,
            sources: loader.sources,
            revision,
            assets,
        };
        // Validate properties even when a node is currently hidden.
        generation.scene(&stores, &Palette(std::collections::HashMap::new()), false)?;
        generation.check_sources(&loader.root)?;
        Ok((generation, stores))
    }
    pub fn check_sources(&self, root: &Path) -> Result<(), Diagnostic> {
        for (path, bytes) in &self.sources {
            if fs::read(root.join(path)).ok().as_ref() != Some(bytes) {
                return Err(Diagnostic::new(
                    path,
                    1,
                    1,
                    "file changed while staging; retrying next watch event",
                ));
            }
        }
        Ok(())
    }
    pub fn scene(
        &self,
        stores: &Stores,
        palette: &Palette,
        colors: bool,
    ) -> Result<Scene, Diagnostic> {
        self.script.begin();
        let mut primitives = Vec::new();
        for node in &self.nodes {
            let mut draw = node.draw.clone();
            for (property, binding) in &node.bind {
                let value = self.script.resolve(binding, stores)?;
                let error = || {
                    node.diagnostic(format!(
                        "{}.{} has incompatible binding {}",
                        node.id, property, binding
                    ))
                };
                match (property.as_str(), &mut draw, value) {
                    ("text", Primitive::Text { text, .. }, Value::Text(value))
                        if value.len() <= 65536 =>
                    {
                        *text = value
                    }
                    ("color", Primitive::Text { color, .. }, Value::Text(value))
                    | ("fill", Primitive::Rounded { fill: color, .. }, Value::Text(value)) => {
                        if colors {
                            *color = palette.get(&value).map_err(|e| self.script.error(e))?;
                        }
                    }
                    (
                        "border_color",
                        Primitive::Rounded {
                            border: Some((color, _)),
                            ..
                        },
                        Value::Text(value),
                    ) => {
                        if colors {
                            *color = palette.get(&value).map_err(|e| node.diagnostic(e))?;
                        }
                    }
                    ("y", primitive, Value::Number(value)) if value.is_finite() => {
                        rect_mut(primitive).y += value as f32;
                    }
                    _ => return Err(error()),
                }
            }
            if !valid_rect(rect(&draw)) {
                return Err(node.diagnostic("binding produced invalid geometry"));
            }
            let visible = if let Some(binding) = &node.visible {
                match self.script.resolve(binding, stores)? {
                    Value::Bool(value) => value,
                    _ => {
                        return Err(node.diagnostic(format!("{}.visible must be boolean", node.id)));
                    }
                }
            } else {
                true
            };
            if let Some(icon) = &node.icon {
                if let Primitive::Image { path, .. } = &mut draw {
                    *path = PathBuf::from(format!("@icon:{icon}"));
                } else {
                    return Err(self
                        .script
                        .error(format!("{}: icon needs an image node", node.id)));
                }
            }
            if visible {
                primitives.push(draw);
            }
        }
        Ok(Scene {
            opacity: 1.,
            font_files: Vec::new(),
            width: self.width,
            slide: None,
            primitives,
            offset: (32., 32.),
            bounds: self.bounds,
            resource_revision: self.revision,
            asset_data: self.assets.clone(),
        })
    }
    pub fn hit(&self, stores: &Stores, x: f32, y: f32) -> Option<&Node> {
        self.script.begin();
        self.nodes.iter().rev().find(|node| {
            if node
                .visible
                .as_ref()
                .is_some_and(|id| self.script.resolve(id, stores).ok() != Some(Value::Bool(true)))
            {
                return false;
            }
            let mut r = rect(&node.draw);
            if let Some(id) = node.bind.get("y")
                && let Ok(Value::Number(value)) = self.script.resolve(id, stores)
            {
                r.y += value as f32;
            }
            x >= r.x
                && y >= r.y
                && x < r.x + r.w
                && y < r.y + r.h
                && (node.action.is_some() || node.input.is_some() || node.scroll.is_some())
        })
    }
}
pub fn rect(draw: &Primitive) -> Rect {
    match draw {
        Primitive::Rounded { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => *rect,
    }
}
fn rect_mut(draw: &mut Primitive) -> &mut Rect {
    match draw {
        Primitive::Rounded { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => rect,
    }
}
fn valid_rect(r: Rect) -> bool {
    [r.x, r.y, r.w, r.h]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 20000.)
        && r.w > 0.
        && r.h > 0.
}

/// The state owner survives every view/script replacement.
struct Pending {
    generation: Option<Generation>,
    stores: Stores,
    palette: Palette,
    palette_bytes: Option<Vec<u8>>,
    watch_revision: u64,
}
pub struct Session {
    root: PathBuf,
    entry: PathBuf,
    palette_path: PathBuf,
    palette: Palette,
    pub stores: Stores,
    generation: Option<Generation>,
    pending: Option<Pending>,
    watch_revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    current: Scene,
    error: Option<String>,
    attempts: u64,
    watcher: Option<notify::RecommendedWatcher>,
}
impl Session {
    pub fn new(
        root: PathBuf,
        entry: PathBuf,
        palette_path: PathBuf,
        palette: Palette,
        fallback: Scene,
    ) -> Self {
        Self {
            root,
            entry,
            palette_path,
            palette,
            stores: Stores::default(),
            generation: None,
            pending: None,
            watch_revision: Default::default(),
            current: fallback,
            error: None,
            attempts: 0,
            watcher: None,
        }
    }
    pub fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use notify::Watcher;
        let revision = self.watch_revision.clone();
        let root = self.root.canonicalize()?;
        let palette = std::path::absolute(&self.palette_path)?;
        let external_parent =
            (!palette.starts_with(&root)).then(|| palette.parent().unwrap().to_path_buf());
        let watched_root = root.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(event)
                    if !event.kind.is_access()
                        && (event.need_rescan()
                            || event.paths.is_empty()
                            || event.paths.iter().any(|path| {
                                if path == &palette {
                                    return true;
                                }
                                if !path.starts_with(&watched_root) {
                                    return false;
                                }
                                if path.is_dir() || path.extension().is_none() {
                                    return true;
                                }
                                path.extension().is_some_and(|ext| {
                                    ["json", "luau", "svg", "png", "jpg", "jpeg", "webp"]
                                        .iter()
                                        .any(|suffix| ext == *suffix)
                                })
                            })) =>
                {
                    revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    wakeup(Ok(()));
                }
                Err(error) => wakeup(Err(error.to_string())),
                _ => {}
            })?;
        watcher.watch(&root, notify::RecursiveMode::Recursive)?;
        if let Some(parent) = external_parent {
            // Watch the directory: atomic replacement would detach a file-only watch.
            watcher.watch(&parent, notify::RecursiveMode::NonRecursive)?;
        }
        self.watcher = Some(watcher);
        Ok(())
    }
    pub fn reload(&mut self) -> Result<Scene, Diagnostic> {
        self.attempts += 1;
        self.pending = None;
        let watch_revision = self
            .watch_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        let palette_bytes = fs::read(&self.palette_path)
            .map_err(|e| Diagnostic::new(Path::new("palette.json"), 1, 1, e))?;
        let palette = Palette::parse(&palette_bytes)
            .map_err(|e| Diagnostic::new(Path::new("palette.json"), 1, 1, e))?;
        let (generation, stores) =
            Generation::load(&self.root, &self.entry, &self.stores, self.attempts)?;
        let scene = generation.scene(&stores, &palette, true)?;
        self.pending = Some(Pending {
            generation: Some(generation),
            stores,
            palette,
            palette_bytes: Some(palette_bytes),
            watch_revision,
        });
        Ok(scene)
    }
    pub fn commit(&mut self, scene: Scene) {
        if let Some(Pending {
            generation,
            stores,
            palette,
            ..
        }) = self.pending.take()
        {
            if let Some(generation) = generation {
                self.generation = Some(generation);
                self.error = None;
            }
            self.stores = stores;
            self.palette = palette;
            self.current = scene;
        }
    }
    pub fn reject(&mut self, message: String) -> Scene {
        self.pending = None;
        self.error = Some(message);
        self.error_scene()
    }
    pub fn error_scene(&self) -> Scene {
        let mut scene = self.current.clone();
        if let Some(error) = &self.error {
            scene.resource_revision = self.attempts;
            scene.primitives.push(Primitive::Rounded {
                rect: Rect::new(16., 505., 1068., 68.),
                radius: 8.,
                fill: [75, 20, 25, 250],
                border: Some(([255, 140, 150, 255], 1.)),
            });
            scene.primitives.push(Primitive::Text {
                rect: Rect::new(28., 515., 1044., 52.),
                text: format!(
                    "Reload failed — old UI kept\n{}",
                    error.chars().take(500).collect::<String>()
                ),
                family: "IBM Plex Sans".into(),
                size: 14.,
                color: [255, 225, 225, 255],
            });
        }
        scene
    }
    pub fn click(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let hit = generation.hit(&self.stores, x, y);
        let (action, focused) = hit.map_or((None, String::new()), |node| {
            (
                node.action.clone(),
                if node.input.is_some() {
                    node.id.clone()
                } else {
                    String::new()
                },
            )
        });
        let mut stores = self.stores.clone();
        stores
            .commit(BTreeMap::from([(
                "bead.focused".into(),
                Value::Text(focused),
            )]))
            .map_err(|e| generation.script.error(e))?;
        if let Some(action) = action {
            stores = generation
                .script
                .action(&action, &stores, Value::Bool(true))?;
        }
        self.stage_state(stores)
    }
    pub fn scroll(&mut self, x: f32, y: f32, delta: f64) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let Some(action) = generation
            .hit(&self.stores, x, y)
            .and_then(|node| node.scroll.clone())
        else {
            return Ok(None);
        };
        let stores = generation
            .script
            .action(&action, &self.stores, Value::Number(delta))?;
        self.stage_state(stores)
    }
    pub fn key(&mut self, text: &str, backspace: bool) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let Some(Value::Text(focused)) = self.stores.get("bead.focused") else {
            return Ok(None);
        };
        let Some(id) = generation
            .nodes
            .iter()
            .find(|node| &node.id == focused)
            .and_then(|node| node.input.clone())
        else {
            return Ok(None);
        };
        let Some(Value::Text(value)) = self.stores.get(&id) else {
            return Ok(None);
        };
        let mut value = value.clone();
        if backspace {
            value.pop();
        } else {
            value.extend(text.chars().filter(|c| !c.is_control()));
        }
        let mut stores = self.stores.clone();
        stores
            .commit(BTreeMap::from([(id, Value::Text(value))]))
            .map_err(|e| generation.script.error(e))?;
        self.stage_state(stores)
    }
    fn stage_state(&mut self, stores: Stores) -> Result<Option<Scene>, Diagnostic> {
        if stores.revision() == self.stores.revision() {
            return Ok(None);
        }
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let mut scene = generation.scene(&stores, &self.palette, true)?;
        scene.font_files = self.current.font_files.clone();
        if self.error.is_some() {
            let old = self.current.clone();
            self.current = scene.clone();
            scene = self.error_scene();
            self.current = old;
        }
        self.pending = Some(Pending {
            generation: None,
            stores,
            palette: Palette(self.palette.0.clone()),
            palette_bytes: None,
            watch_revision: 0,
        });
        Ok(Some(scene))
    }
    pub fn check_sources(&self) -> Result<(), Diagnostic> {
        if let Some(pending) = &self.pending
            && let Some(generation) = &pending.generation
        {
            generation.check_sources(&self.root)?;
            if pending.watch_revision
                != self
                    .watch_revision
                    .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(Diagnostic::new(
                    &self.entry,
                    1,
                    1,
                    "newer file edit superseded this staged generation",
                ));
            }
            if let Some(bytes) = &pending.palette_bytes
                && fs::read(&self.palette_path).ok().as_ref() != Some(bytes)
            {
                return Err(Diagnostic::new(
                    Path::new("palette.json"),
                    1,
                    1,
                    "palette changed during staging",
                ));
            }
        }
        Ok(())
    }
    pub fn asset_snapshot(&self, path: &Path) -> Option<std::sync::Arc<[u8]>> {
        self.current.asset_data.get(path).cloned()
    }
    pub fn state_summary(&self) -> String {
        serde_json::to_string(self.stores.values()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
