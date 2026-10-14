// SPDX-License-Identifier: MIT OR Apache-2.0
//! Replaceable typed views and bounded Luau behavior over durable Rust state.
pub mod accessibility;
pub mod frontend;
pub mod input;
pub mod layout;
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
    #[serde(default)]
    pub action_arg: Option<Value>,
    #[serde(default)]
    pub scroll_store: Option<String>,
    #[serde(default)]
    pub focusable: bool,
    #[serde(skip)]
    pub clip: Option<Rect>,
    #[serde(default)]
    pub label: String,
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
    markup_entry: Option<PathBuf>,
    root: PathBuf,
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
        if entry.extension().is_some_and(|ext| ext == "luau") {
            return Self::load_markup(root, entry, stores, revision);
        }
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
            markup_entry: None,
            root: loader.root.clone(),
        };
        // Validate properties even when a node is currently hidden.
        generation.scene(&stores, &Palette(std::collections::HashMap::new()), false)?;
        generation.check_sources(&loader.root)?;
        Ok((generation, stores))
    }
    fn load_markup(
        root: &Path,
        entry: &Path,
        stores: &Stores,
        revision: u64,
    ) -> Result<(Self, Stores), Diagnostic> {
        let mut loader = Loader {
            root: root
                .canonicalize()
                .map_err(|e| Diagnostic::new(entry, 1, 1, e))?,
            sources: BTreeMap::new(),
        };
        let mut directories = vec![entry.parent().unwrap_or(Path::new("")).to_path_buf()];
        let mut count = 0;
        while let Some(directory) = directories.pop() {
            for item in fs::read_dir(loader.root.join(&directory))
                .map_err(|e| Diagnostic::new(&directory, 1, 1, e))?
            {
                let item = item.map_err(|e| Diagnostic::new(&directory, 1, 1, e))?;
                let path = directory.join(item.file_name());
                count += 1;
                if count > 256 {
                    return Err(Diagnostic::new(&path, 1, 1, "UI bundle exceeds256sources"));
                }
                if item
                    .file_type()
                    .map_err(|e| Diagnostic::new(&path, 1, 1, e))?
                    .is_dir()
                {
                    directories.push(path);
                    continue;
                }
                match path.extension().and_then(|s| s.to_str()) {
                    Some("luau") => {
                        let bytes = loader.read(&path)?;
                        script::Script::validate_source(&bytes, &path)?;
                    }
                    Some("json") => {
                        let _: serde_json::Value = loader.json(&path)?;
                    }
                    Some("svg" | "png" | "jpg" | "jpeg" | "webp") => {
                        loader.read(&path)?;
                    }
                    _ => {}
                }
            }
        }
        // Root assets are snapshots too, including ones initially outside the scroll viewport.
        for item in fs::read_dir(&loader.root).map_err(|e| Diagnostic::new(entry, 1, 1, e))? {
            let item = item.map_err(|e| Diagnostic::new(entry, 1, 1, e))?;
            let path = PathBuf::from(item.file_name());
            if path.extension().is_some_and(|ext| {
                ["svg", "png", "jpg", "jpeg", "webp"]
                    .iter()
                    .any(|s| ext == *s)
            }) {
                loader.read(&path)?;
            }
        }
        let script_path = entry
            .parent()
            .unwrap_or(Path::new(""))
            .join("behavior.luau");
        let script = script::Script::load(
            loader.sources.get(&script_path).ok_or_else(|| {
                Diagnostic::new(&script_path, 1, 1, "missing sibling behavior.luau")
            })?,
            &script_path,
        )?;
        let mut defaults = script.schema()?;
        for (id, value) in [
            ("bead.focused", Value::Text(String::new())),
            ("bead.hovered", Value::Text(String::new())),
        ] {
            if defaults.insert(id.into(), value).is_some() {
                return Err(Diagnostic::new(
                    &script_path,
                    1,
                    1,
                    format!("{id} is reserved"),
                ));
            }
        }
        let stores = stores
            .reconcile(&defaults)
            .map_err(|e| Diagnostic::new(&script_path, 1, 1, e))?;
        script.validate_computed(&stores)?;
        for (path, bytes) in &loader.sources {
            if path.extension().is_some_and(|ext| {
                ["svg", "png", "jpg", "jpeg", "webp"]
                    .iter()
                    .any(|s| ext == *s)
            }) {
                bead_render::validate_asset(path, bytes)
                    .map_err(|e| Diagnostic::new(path, 1, 1, e))?;
            }
        }
        let assets = loader
            .sources
            .iter()
            .filter(|(path, _)| {
                path.extension().is_some_and(|ext| {
                    ["svg", "png", "jpg", "jpeg", "webp"]
                        .iter()
                        .any(|s| ext == *s)
                })
            })
            .map(|(path, data)| (loader.root.join(path), std::sync::Arc::from(data.clone())))
            .collect();
        let generation = Self {
            script,
            nodes: vec![],
            width: 1100.,
            bounds: Rect::new(-32., -32., 1164., 654.),
            sources: loader.sources,
            revision,
            assets,
            markup_entry: Some(entry.to_path_buf()),
            root: loader.root,
        };
        generation.check_sources(root)?;
        Ok((generation, stores))
    }
    pub fn materialize(
        &self,
        stores: &Stores,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<(Scene, Vec<Node>), Diagnostic> {
        if let Some(entry) = &self.markup_entry {
            use frontend::FrontEnd;
            self.script.begin();
            let resolve = |name: &str| self.script.resolve(name, stores);
            let raw = frontend::LuauFrontEnd.build(entry, &self.sources, &resolve)?;
            let view = layout::ViewSpec::from_raw(&raw, &resolve)?;
            let mut result = layout::layout(&view, &resolve, palette, self.revision, viewport)?;
            for node in &result.nodes {
                if let Some(action) = &node.action {
                    self.script.validate_action(action)?;
                }
                if let Some(input) = &node.input
                    && !matches!(stores.get(input), Some(Value::Text(_)))
                {
                    return Err(node.diagnostic("input binding must name a mutable text store"));
                }
                if let Some(scroll) = &node.scroll_store
                    && !matches!(stores.get(scroll), Some(Value::Number(_)))
                {
                    return Err(node.diagnostic("scroll binding must name a mutable number store"));
                }
            }
            for primitive in &mut result.scene.primitives {
                if let Primitive::Image { path, .. } = primitive
                    && !path.to_str().is_some_and(|s| s.starts_with("@icon:"))
                {
                    let full = self.root.join(&*path);
                    if !self.assets.contains_key(&full) {
                        return Err(Diagnostic::new(
                            path,
                            1,
                            1,
                            "asset not in this generation's snapshot",
                        ));
                    }
                    *path = full;
                }
            }
            if let Some(Value::Text(focused)) = stores.get("bead.focused")
                && let Some(node) = result.nodes.iter().find(|n| &n.id == focused)
            {
                let index = result.scene.primitives.len();
                result.scene.primitives.push(Primitive::Rounded {
                    rect: rect(&node.draw),
                    radius: 8.,
                    fill: [0, 0, 0, 0],
                    border: Some((palette.get("primary").unwrap_or([200, 200, 200, 255]), 1.)),
                });
                if let Some(clip) = node.clip {
                    result.scene.clips.insert(index, clip);
                }
            }
            result.scene.asset_data = self.assets.clone();
            return Ok((result.scene, result.nodes));
        }
        Ok((self.scene(stores, palette, true)?, self.nodes.clone()))
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
        if self.markup_entry.is_some() {
            return Ok(self.materialize(stores, palette, (1200., 800.))?.0);
        }
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
            motion: None,
            font_files: Vec::new(),
            width: self.width,
            slide: None,
            primitives,
            offset: (32., 32.),
            bounds: self.bounds,
            resource_revision: self.revision,
            asset_data: self.assets.clone(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
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
    nodes: Vec<Node>,
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
    current_nodes: Vec<Node>,
    editors: BTreeMap<String, input::Editor>,
    committed_editors: BTreeMap<String, input::Editor>,
    viewport: (f32, f32),
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
            current_nodes: vec![],
            viewport: (1200., 800.),
            editors: BTreeMap::new(),
            committed_editors: BTreeMap::new(),
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
        let (generation, mut stores) =
            Generation::load(&self.root, &self.entry, &self.stores, self.attempts)?;
        let (mut scene, mut nodes) = generation.materialize(&stores, &palette, self.viewport)?;
        if let Some(Value::Text(focused)) = stores.get("bead.focused")
            && !focused.is_empty()
            && !nodes
                .iter()
                .any(|n| &n.id == focused && (n.focusable || n.input.is_some()))
        {
            let next = nodes
                .iter()
                .find(|n| n.focusable || n.input.is_some())
                .map_or(String::new(), |n| n.id.clone());
            stores
                .commit(BTreeMap::from([("bead.focused".into(), Value::Text(next))]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
            (scene, nodes) = generation.materialize(&stores, &palette, self.viewport)?;
        }

        self.pending = Some(Pending {
            generation: Some(generation),
            stores,
            palette,
            palette_bytes: Some(palette_bytes),
            watch_revision,
            nodes,
        });
        Ok(scene)
    }
    pub fn commit(&mut self, scene: Scene) {
        if let Some(Pending {
            generation,
            stores,
            palette,
            nodes,
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
            self.current_nodes = nodes;
            self.committed_editors = self.editors.clone();
        }
    }
    pub fn reject(&mut self, message: String) -> Scene {
        self.pending = None;
        self.error = Some(message);
        self.editors = self.committed_editors.clone();
        self.error_scene()
    }
    pub fn error_scene(&self) -> Scene {
        let mut scene = self.current.clone();
        if let Some(error) = &self.error {
            if scene.motion.as_ref().is_some_and(|m| !m.open) {
                scene.primitives.clear();
                scene.clips.clear();
            }
            scene.motion = None;
            scene.opacity = 1.;
            scene.offset.1 = 32.;
            let banner_y = (scene.bounds.h - 64. - 84.).max(16.);
            let banner_width = (scene.width - 32.).max(64.);
            scene.resource_revision = self.attempts;
            scene.primitives.push(Primitive::Rounded {
                rect: Rect::new(16., banner_y, banner_width, 68.),
                radius: 8.,
                fill: [75, 20, 25, 250],
                border: Some(([255, 140, 150, 255], 1.)),
            });
            scene.primitives.push(Primitive::Text {
                rect: Rect::new(28., banner_y + 10., (banner_width - 24.).max(1.), 52.),
                text: format!(
                    "Reload failed — old UI kept\n{}",
                    error.chars().take(500).collect::<String>()
                ),
                family: "IBM Plex Sans".into(),
                align: None,
                size: 14.,
                color: [255, 225, 225, 255],
            });
        }
        scene
    }
    fn hit(&self, x: f32, y: f32, scroll: bool) -> Option<&Node> {
        let generation = self.generation.as_ref()?;
        if generation.markup_entry.is_none() {
            return generation.hit(&self.stores, x, y);
        }
        self.current_nodes.iter().rev().find(|node| {
            let r = rect(&node.draw);
            let inside = |r: Rect| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;
            inside(r)
                && node.clip.is_none_or(inside)
                && if scroll {
                    node.scroll_store.is_some()
                } else {
                    node.focusable
                }
        })
    }
    pub fn click(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let hit = self.hit(x, y, false);
        let (action, focused) = hit.map_or((None, String::new()), |node| {
            (
                node.action.clone(),
                if node.input.is_some() || node.focusable {
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
            stores = generation.script.action(
                &action,
                &stores,
                hit.and_then(|node| node.action_arg.clone())
                    .unwrap_or(Value::Bool(true)),
            )?;
        }
        self.stage_state(stores)
    }
    pub fn scroll(&mut self, x: f32, y: f32, delta: f64) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let Some(node) = self.hit(x, y, true) else {
            return Ok(None);
        };
        let stores = if let Some(id) = &node.scroll_store {
            let Some(Value::Number(value)) = self.stores.get(id) else {
                return Err(node.diagnostic("scroll store not a number"));
            };
            let mut stores = self.stores.clone();
            stores
                .commit(BTreeMap::from([(
                    id.clone(),
                    Value::Number((value + delta).max(0.)),
                )]))
                .map_err(|e| node.diagnostic(e))?;
            stores
        } else if let Some(action) = &node.scroll {
            generation
                .script
                .action(action, &self.stores, Value::Number(delta))?
        } else {
            return Ok(None);
        };
        self.stage_state(stores)
    }
    pub fn hover(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        if self.stores.get("bead.hovered").is_none() {
            return Ok(None);
        }
        let id = self
            .hit(x, y, false)
            .map_or(String::new(), |node| node.id.clone());
        self.update(BTreeMap::from([("bead.hovered".into(), Value::Text(id))]))
    }
    pub fn focus_next(&mut self, backward: bool) -> Result<Option<Scene>, Diagnostic> {
        let ids: Vec<_> = self
            .current_nodes
            .iter()
            .filter(|n| n.focusable || n.input.is_some())
            .map(|n| n.id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let current = self.stores.get("bead.focused");
        let index = ids
            .iter()
            .position(|id| current == Some(&Value::Text(id.clone())));
        let next = match index {
            None => {
                if backward {
                    ids.len() - 1
                } else {
                    0
                }
            }
            Some(i) => {
                if backward {
                    (i + ids.len() - 1) % ids.len()
                } else {
                    (i + 1) % ids.len()
                }
            }
        };
        self.focus_id(&ids[next])
    }
    pub fn focus_id(&mut self, id: &str) -> Result<Option<Scene>, Diagnostic> {
        if !self
            .current_nodes
            .iter()
            .any(|node| node.id == id && (node.focusable || node.input.is_some()))
        {
            return Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "focus target is not visible/focusable",
            ));
        }
        self.update(BTreeMap::from([(
            "bead.focused".into(),
            Value::Text(id.into()),
        )]))
    }
    pub fn action_at(&self, x: f32, y: f32) -> Option<(String, String, Value)> {
        let node = self.hit(x, y, false)?;
        Some((
            node.id.clone(),
            node.action.clone()?,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        ))
    }
    pub fn focused_action(&self) -> Option<(String, String, Value)> {
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        let node = self.current_nodes.iter().find(|n| &n.id == focused)?;
        Some((
            node.id.clone(),
            node.action.clone()?,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        ))
    }
    pub fn activate_focused(&mut self) -> Result<Option<Scene>, Diagnostic> {
        let Some(Value::Text(focused)) = self.stores.get("bead.focused") else {
            return Ok(None);
        };
        let Some(node) = self.current_nodes.iter().find(|n| &n.id == focused) else {
            return Ok(None);
        };
        let Some(action) = &node.action else {
            return Ok(None);
        };
        let generation = self.generation.as_ref().unwrap();
        let stores = generation.script.action(
            action,
            &self.stores,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        )?;
        self.stage_state(stores)
    }
    fn focused_input(&self) -> Option<(String, String, String)> {
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        let node = self.current_nodes.iter().find(|n| &n.id == focused)?;
        let store = node.input.as_ref()?;
        let Value::Text(text) = self.stores.get(store)? else {
            return None;
        };
        Some((focused.clone(), store.clone(), text.clone()))
    }
    pub fn edit_key(
        &mut self,
        key: input::EditKey,
        shift: bool,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, mut text)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        if matches!(key, input::EditKey::SelectAll) {
            editor.anchor = 0;
            editor.cursor = text.len();
        } else if matches!(key, input::EditKey::Delete) {
            let end = if editor.cursor == editor.anchor {
                input::move_cursor(&text, editor.cursor, input::EditKey::Right)
            } else {
                editor.anchor
            };
            let (new, cursor) = input::replace(&text, editor.cursor, end, "");
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        } else {
            editor.cursor = input::move_cursor(&text, editor.cursor, key);
            if !shift {
                editor.anchor = editor.cursor;
            }
        }
        self.update(BTreeMap::from([(store, Value::Text(text))]))?
            .map_or_else(|| self.relayout(), |scene| Ok(Some(scene)))
    }
    pub fn ime(
        &mut self,
        preedit: String,
        commit: Option<String>,
        before: u32,
        after: u32,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, mut text)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        let start = editor.cursor.saturating_sub(before as usize);
        let end = (editor.cursor + after as usize).min(text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "IME deletion must use UTF-8 boundaries",
            ));
        }
        if before > 0 || after > 0 {
            let (new, cursor) = input::replace(&text, start, end, "");
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        }
        if let Some(commit) = commit {
            let (new, cursor) = input::replace(&text, editor.cursor, editor.anchor, &commit);
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        }
        editor.preedit = preedit;
        self.update(BTreeMap::from([(store, Value::Text(text))]))?
            .map_or_else(|| self.relayout(), |scene| Ok(Some(scene)))
    }
    pub fn decorate_inputs(&self, scene: &mut Scene) -> Result<(), Diagnostic> {
        let nodes = self
            .pending
            .as_ref()
            .map_or(&self.current_nodes, |p| &p.nodes);
        let stores = self.pending.as_ref().map_or(&self.stores, |p| &p.stores);
        let Some(Value::Text(focused)) = stores.get("bead.focused") else {
            return Ok(());
        };
        let Some(node) = nodes.iter().find(|n| &n.id == focused && n.input.is_some()) else {
            return Ok(());
        };
        let Some(Value::Text(value)) = stores.get(node.input.as_ref().unwrap()) else {
            return Ok(());
        };
        let r = rect(&node.draw);
        let editor = self.editors.get(focused);
        let cursor = input::boundaries(value)
            .into_iter()
            .rfind(|p| *p <= editor.map_or(value.len(), |e| e.cursor))
            .unwrap_or(0);
        let anchor = input::boundaries(value)
            .into_iter()
            .rfind(|p| *p <= editor.map_or(cursor, |e| e.anchor))
            .unwrap_or(0);
        let preedit = editor.map_or("", |e| e.preedit.as_str());
        let text_index=scene.primitives.iter().position(|p|matches!(p,Primitive::Text{rect,..}if rect.x>=r.x&&rect.y>=r.y&&rect.x<r.x+r.w&&rect.y<r.y+r.h));
        let Some(index) = text_index else {
            return Ok(());
        };
        let Primitive::Text {
            rect: area,
            text,
            family,
            size,
            ..
        } = &scene.primitives[index]
        else {
            return Ok(());
        };
        let area = *area;
        let family = family.clone();
        let size = *size;
        let _ = text;
        let measure = |pos| {
            bead_render::text_boundary_x(scene, value, pos, &family, size, area.w)
                .map_err(|e| node.diagnostic(e))
        };
        let x = measure(cursor)?;
        let end = measure(anchor)?;
        if cursor != anchor {
            let highlight = Primitive::Rounded {
                rect: Rect::new(
                    area.x + x.min(end),
                    area.y,
                    (x - end).abs().max(1.),
                    size * 1.4,
                ),
                radius: 1.,
                fill: {
                    let mut c = self.palette.get("primary").unwrap_or([120, 120, 120, 255]);
                    c[3] = 70;
                    c
                },
                border: None,
            };
            // Insert behind the text; move clip indices along with the command.
            scene.primitives.insert(index, highlight);
            for layer in &mut scene.layers {
                if layer.own.contains(&index) {
                    layer.own.end += 1;
                } else if layer.own.start >= index {
                    layer.own.start += 1;
                    layer.own.end += 1;
                }
                if layer.subtree_end > index {
                    layer.subtree_end += 1;
                }
            }

            scene.clips = scene
                .clips
                .iter()
                .map(|(i, r)| (if *i >= index { i + 1 } else { *i }, *r))
                .collect();
        }
        let text_index = if cursor != anchor { index + 1 } else { index };
        if !preedit.is_empty()
            && let Primitive::Text { text, .. } = &mut scene.primitives[text_index]
        {
            *text = format!("{}{}{}", &value[..cursor], preedit, &value[cursor..]);
        }
        scene.primitives.push(Primitive::Rounded {
            rect: Rect::new(area.x + x, area.y, 1., size * 1.4),
            radius: 0.,
            fill: self
                .palette
                .get("onSurface")
                .unwrap_or([220, 220, 220, 255]),
            border: None,
        });
        Ok(())
    }
    pub fn pointer_cursor(&self, x: f32, y: f32) -> &'static str {
        self.hit(x, y, false).map_or("default", |n| {
            if n.input.is_some() { "text" } else { "pointer" }
        })
    }
    pub fn selected_text(&self) -> Option<String> {
        let (id, _, text) = self.focused_input()?;
        let editor = self.editors.get(&id)?;
        let points = input::boundaries(&text);
        let cursor = points
            .iter()
            .copied()
            .rfind(|p| *p <= editor.cursor)
            .unwrap_or(0);
        let anchor = points
            .into_iter()
            .rfind(|p| *p <= editor.anchor)
            .unwrap_or(0);
        (cursor != anchor).then(|| text[cursor.min(anchor)..cursor.max(anchor)].to_owned())
    }
    pub fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        let (id, _, text) = self.focused_input()?;
        let editor = self.editors.get(&id);
        Some((
            text.clone(),
            editor.map_or(text.len(), |e| e.cursor).min(text.len()) as i32,
            editor.map_or(text.len(), |e| e.anchor).min(text.len()) as i32,
        ))
    }
    pub fn key(&mut self, text: &str, backspace: bool) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, value)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(value.len()));
        editor.clamp(&value);
        let anchor = if backspace && editor.cursor == editor.anchor {
            input::move_cursor(&value, editor.cursor, input::EditKey::Left)
        } else {
            editor.anchor
        };
        let insert: String = text.chars().filter(|c| !c.is_control()).collect();
        let (value, cursor) = input::replace(
            &value,
            editor.cursor,
            anchor,
            if backspace { "" } else { &insert },
        );
        editor.cursor = cursor;
        editor.anchor = cursor;
        editor.preedit.clear();
        self.update(BTreeMap::from([(store, Value::Text(value))]))
    }
    fn stage_state(&mut self, stores: Stores) -> Result<Option<Scene>, Diagnostic> {
        if stores.revision() == self.stores.revision() {
            return Ok(None);
        }
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let mut stores = stores;
        let (mut scene, mut nodes) =
            generation.materialize(&stores, &self.palette, self.viewport)?;
        if let Some(Value::Text(focused)) = stores.get("bead.focused")
            && !focused.is_empty()
            && !nodes
                .iter()
                .any(|n| &n.id == focused && (n.focusable || n.input.is_some()))
        {
            let next = nodes
                .iter()
                .find(|n| n.focusable || n.input.is_some())
                .map_or(String::new(), |n| n.id.clone());
            stores
                .commit(BTreeMap::from([("bead.focused".into(), Value::Text(next))]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
            (scene, nodes) = generation.materialize(&stores, &self.palette, self.viewport)?;
        }

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
            nodes,
        });
        Ok(Some(scene))
    }
    /// External model updates use the same staged view transaction as input.
    pub fn update(
        &mut self,
        changes: BTreeMap<String, Value>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let mut stores = self.stores.clone();
        stores
            .commit(changes)
            .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        if self.generation.is_none() {
            self.stores = stores;
            return Ok(None);
        }
        self.stage_state(stores)
    }
    pub fn set_viewport(&mut self, width: u32, height: u32) -> bool {
        let next = (width.max(1) as f32, height.max(1) as f32);
        let changed = self.viewport != next;
        self.viewport = next;
        changed
    }
    pub fn relayout(&mut self) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let (mut scene, nodes) =
            generation.materialize(&self.stores, &self.palette, self.viewport)?;
        scene.font_files = self.current.font_files.clone();
        self.pending = Some(Pending {
            generation: None,
            stores: self.stores.clone(),
            palette: Palette(self.palette.0.clone()),
            palette_bytes: None,
            watch_revision: 0,
            nodes,
        });
        Ok(Some(scene))
    }
    pub fn view_nodes(&self) -> &[Node] {
        &self.current_nodes
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
    pub fn accessibility_tree(&self) -> accesskit::TreeUpdate {
        accessibility::tree(&self.current_nodes, &self.stores)
    }
    pub fn accessibility_action(
        &mut self,
        target: u64,
        action: &str,
        value: Option<String>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let node = self
            .current_nodes
            .iter()
            .find(|n| accessibility::id(&n.id).0 == target)
            .ok_or_else(|| {
                Diagnostic::new(&self.entry, 1, 1, "accessibility target unavailable")
            })?;
        let id = node.id.clone();
        let input = node.input.clone();
        match action {
            "focus" => self.focus_id(&id),
            "click" => {
                let node = self.current_nodes.iter().find(|n| n.id == id).unwrap();
                let action = node.action.clone();
                let arg = node.action_arg.clone().unwrap_or(Value::Bool(true));
                let mut stores = self.stores.clone();
                stores
                    .commit(BTreeMap::from([("bead.focused".into(), Value::Text(id))]))
                    .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
                if let Some(action) = action {
                    stores = self
                        .generation
                        .as_ref()
                        .unwrap()
                        .script
                        .action(&action, &stores, arg)?;
                }
                self.stage_state(stores)
            }
            "set-value" => {
                let store = input
                    .ok_or_else(|| Diagnostic::new(&self.entry, 1, 1, "target is not editable"))?;
                self.update(BTreeMap::from([(
                    store,
                    Value::Text(value.ok_or_else(|| {
                        Diagnostic::new(&self.entry, 1, 1, "set-value requires text")
                    })?),
                )]))
            }
            _ => Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "unsupported accessibility action",
            )),
        }
    }
    pub fn resource_counts(&self) -> String {
        format!(
            "stores={} nodes={} watchers={} subscriptions={} bindings={} sources={} luau_bytes={}",
            self.stores.values().len(),
            self.current_nodes.len(),
            usize::from(self.watcher.is_some()),
            usize::from(self.watcher.is_some()),
            self.generation
                .as_ref()
                .map_or(0, |g| g.script.binding_count()),
            self.generation.as_ref().map_or(0, |g| g.sources.len()),
            self.generation
                .as_ref()
                .map_or(0, |g| g.script.memory_bytes())
        )
    }
    pub fn state_summary(&self) -> String {
        serde_json::to_string(self.stores.values()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod markup_tests;
