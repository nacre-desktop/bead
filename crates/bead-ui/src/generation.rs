// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tracked source loading and format-independent generation materialization.
use super::*;
struct WeakAsset {
    data: std::sync::Weak<[u8]>,
    charge: bead_core::cache::Charge,
}
type AssetWeakIndex = std::collections::HashMap<PathBuf, WeakAsset>;
static ASSET_WEAK_INDEX: std::sync::OnceLock<std::sync::Mutex<AssetWeakIndex>> =
    std::sync::OnceLock::new();
fn intern_asset(path: &Path, bytes: Vec<u8>) -> std::sync::Arc<[u8]> {
    let index = ASSET_WEAK_INDEX.get_or_init(|| std::sync::Mutex::new(AssetWeakIndex::new()));
    let mut index = index.lock().unwrap_or_else(|poison| poison.into_inner());
    index.retain(|_, value| value.data.strong_count() != 0);
    if let Some(existing) = index.get(path).and_then(|entry| entry.data.upgrade())
        && existing.as_ref() == bytes.as_slice()
    {
        return existing;
    }
    let data: std::sync::Arc<[u8]> = std::sync::Arc::from(bytes);
    let key = path.to_path_buf();
    let charge_bytes = std::mem::size_of::<(PathBuf, WeakAsset)>() + key.capacity();
    let budget = bead_core::cache::Budget::new("weak-ui-asset-index", 64 * 1024);
    let Ok(charge) = budget.reserve(charge_bytes) else {
        return data;
    };
    index.remove(path);
    while index.len() >= 128
        || index
            .values()
            .map(|entry| entry.charge.bytes())
            .sum::<usize>()
            + charge_bytes
            > 64 * 1024
    {
        if let Some(oldest) = index.keys().next().cloned() {
            index.remove(&oldest);
        } else {
            break;
        }
    }
    index.insert(
        key,
        WeakAsset {
            data: std::sync::Arc::downgrade(&data),
            charge,
        },
    );
    data
}

#[cfg(test)]
mod asset_sharing_tests {
    use super::*;
    #[test]
    fn equal_asset_snapshots_share_but_changed_bytes_and_idle_release_do_not() {
        let path = Path::new("/asset-sharing-proof/icon.svg");
        let a = intern_asset(path, b"original".to_vec());
        let b = intern_asset(path, b"original".to_vec());
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        let changed = intern_asset(path, b"changed".to_vec());
        assert_eq!(a.as_ref(), b"original");
        assert!(!std::sync::Arc::ptr_eq(&a, &changed));
        let weak = std::sync::Arc::downgrade(&changed);
        drop(changed);
        assert!(weak.upgrade().is_none());
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
    pub(super) version: u32,
    pub(super) script: PathBuf,
    pub(super) width: f32,
    pub(super) bounds: Rect,
    #[serde(default)]
    pub(super) includes: Vec<PathBuf>,
    pub(super) nodes: Vec<Node>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fragment {
    pub(super) nodes: Vec<Node>,
}
pub struct Generation {
    pub script: script::Script,
    pub nodes: Vec<Node>,
    pub width: f32,
    pub bounds: Rect,
    pub sources: BTreeMap<PathBuf, Vec<u8>>,
    pub revision: u64,
    pub(super) assets: BTreeMap<PathBuf, std::sync::Arc<[u8]>>,
    pub(super) markup_entry: Option<PathBuf>,
    markup_frontend: Option<frontend::LuauFrontEnd>,
    materialized: std::cell::RefCell<Option<crate::materialized::Cached>>,
    view_reads: std::cell::RefCell<BTreeSet<String>>,
    view_reads_overflow: std::cell::Cell<bool>,
    pub(super) root: PathBuf,
}
struct Loader {
    pub(super) root: PathBuf,
    pub(super) sources: BTreeMap<PathBuf, Vec<u8>>,
}
impl Loader {
    pub(super) fn read(&mut self, path: &Path) -> Result<Vec<u8>, Diagnostic> {
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
    pub(super) fn json<T: serde::de::DeserializeOwned>(
        &mut self,
        path: &Path,
    ) -> Result<T, Diagnostic> {
        let bytes = self.read(path)?;
        serde_json::from_slice(&bytes).map_err(|e| Diagnostic::new(path, e.line(), e.column(), e))
    }
}
impl Generation {
    pub(super) fn markup_resource_bytes(&self) -> (usize, usize, usize) {
        self.markup_frontend
            .as_ref()
            .map_or((0, 0, 0), frontend::LuauFrontEnd::resource_bytes)
    }
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
                Primitive::Arc {
                    start_angle,
                    sweep_angle,
                    stroke_width,
                    ..
                } => {
                    start_angle.is_finite()
                        && (-360. ..=360.).contains(start_angle)
                        && sweep_angle.is_finite()
                        && (-360. ..=360.).contains(sweep_angle)
                        && stroke_width.is_finite()
                        && (0. ..=64.).contains(stroke_width)
                        && *stroke_width > 0.
                }
                Primitive::Polyline { points, stroke_width, .. } => {
                    points.len() <= 256 && stroke_width.is_finite()
                        && (0. ..=64.).contains(stroke_width)
                        && points.iter().flatten().all(|value| value.is_finite() && (0. ..=1.).contains(value))
                }
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
                Primitive::Image { blur, .. } => blur.is_finite() && (0.0..=32.0).contains(blur),
            };
            if !valid_shape || !valid_rect(rect(&node.draw)) {
                return Err(node.diagnostic(format!("invalid geometry for {}", node.id)));
            }
            if let Primitive::Image { path, .. } = &mut node.draw {
                let bytes = loader.read(path)?;
                bead_render::validate_asset(path, &bytes)
                    .map_err(|e| Diagnostic::new(path, 1, 1, e))?;
                *path = loader.root.join(&*path);
                assets.insert(path.clone(), intern_asset(path, bytes));
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
            markup_frontend: None,
            materialized: Default::default(),
            view_reads: Default::default(),
            view_reads_overflow: Default::default(),
            root: loader.root.clone(),
        };
        // Validate properties even when a node is currently hidden.
        generation.scene(&stores, &Palette(Default::default()), false)?;
        generation.check_sources(&loader.root)?;
        Ok((generation, stores))
    }
    pub(super) fn load_markup(
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
        // Optional shared modules are explicit, root-relative snapshots. This keeps
        // sibling views out of the bundle unless the application actually reuses them.
        let manifest = entry.parent().unwrap_or(Path::new("")).join("bundle.json");
        if loader.root.join(&manifest).is_file() {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Bundle {
                modules: Vec<PathBuf>,
            }
            let bundle: Bundle = loader.json(&manifest)?;
            if bundle.modules.len() + loader.sources.len() > 256 {
                return Err(Diagnostic::new(
                    &manifest,
                    1,
                    1,
                    "UI bundle exceeds256sources",
                ));
            }
            for module in bundle.modules {
                if module.extension().and_then(|s| s.to_str()) != Some("luau")
                    || !module
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_)))
                {
                    return Err(Diagnostic::new(
                        &manifest,
                        1,
                        1,
                        "shared modules must be root-relative Luau paths",
                    ));
                }
                let bytes = loader.read(&module)?;
                script::Script::validate_source(&bytes, &module)?;
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
            ("bead.focus_visible", Value::Bool(true)),
            ("bead.preview.key", Value::Text(String::new())),
            ("bead.preview.value", Value::Number(0.)),
            // True from a pointer press until its release, as seen by the press's own action.
            ("bead.pressed", Value::Bool(false)),
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
            .map(|(path, data)| {
                let full = loader.root.join(path);
                (full.clone(), intern_asset(&full, data.clone()))
            })
            .collect();
        let markup_frontend = Some(frontend::LuauFrontEnd::new(entry, &loader.sources)?);
        let generation = Self {
            script,
            nodes: vec![],
            width: 1100.,
            bounds: Rect::new(-32., -32., 1164., 654.),
            sources: loader.sources,
            revision,
            assets,
            markup_entry: Some(entry.to_path_buf()),
            markup_frontend,
            materialized: Default::default(),
            view_reads: Default::default(),
            view_reads_overflow: Default::default(),
            root: loader.root,
        };
        generation.check_sources(root)?;
        Ok((generation, stores))
    }
    pub(super) fn model_affects_view<'a>(&self, mut names: impl Iterator<Item = &'a str>) -> bool {
        if self.markup_entry.is_none() || self.view_reads_overflow.get() {
            return true;
        }
        let reads = self.view_reads.borrow();
        names.any(|name| reads.contains(name))
    }
    pub fn materialize(
        &self,
        stores: &Stores,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<(Scene, Vec<Node>), Diagnostic> {
        let (mut scene, nodes) = self.materialize_view(stores, palette, viewport)?;
        for primitive in &mut scene.primitives {
            if let Primitive::Image { path, .. } = primitive
                && path.to_str().is_some_and(|p| p.ends_with(".themed.svg"))
            {
                *path = crate::themed::themed_asset(path, palette, &mut scene.asset_data)
                    .map_err(|e| Diagnostic::new(path, 1, 1, e))?;
            }
        }
        Ok((scene, nodes))
    }
    fn materialize_view(
        &self,
        stores: &Stores,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<(Scene, Vec<Node>), Diagnostic> {
        if let Some(entry) = &self.markup_entry {
            use frontend::FrontEnd;
            self.script.begin();
            let resolve = |name: &str| {
                if name == "bead.viewport.width" {
                    return Ok(Value::Number(viewport.0 as f64));
                }
                if name == "bead.viewport.height" {
                    return Ok(Value::Number(viewport.1 as f64));
                }
                let value = self.script.resolve(name, stores)?;
                // Union within a generation: a rejected branch must not erase
                // dependencies still used by the committed view.
                if !self.view_reads_overflow.get() {
                    let mut reads = self.view_reads.borrow_mut();
                    reads.extend(self.script.store_reads(name, stores));
                    // A host can reconcile its schema during a generation's life.
                    // On overflow use ordinary staging instead of growing this cache.
                    if reads.len() > 4096 {
                        reads.clear();
                        self.view_reads_overflow.set(true);
                    }
                }
                Ok(value)
            };
            let markup_profile = bead_core::profile::Span::new("markup");
            let raw = self
                .markup_frontend
                .as_ref()
                .ok_or_else(|| Diagnostic::new(entry, 1, 1, "missing markup frontend"))?
                .build(entry, &resolve)?;
            drop(markup_profile);
            bead_core::profile::mark("luau_view_ready");
            if let Some(cached) = &*self.materialized.borrow()
                && let Some(result) = cached.patch(&raw, &resolve, palette, viewport)?
            {
                return Ok(result);
            }
            let view_profile = bead_core::profile::Span::new("view_ir");
            let view = layout::ViewSpec::from_raw(&raw, &resolve)?;
            drop(view_profile);
            let layout_profile = bead_core::profile::Span::new("layout");
            let mut result = layout::layout(&view, &resolve, palette, self.revision, viewport)?;
            drop(layout_profile);
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
            if stores.get("bead.focus_visible") != Some(&Value::Bool(false))
                && let Some(Value::Text(focused)) = stores.get("bead.focused")
                && let Some(node) = result
                    .nodes
                    .iter()
                    .find(|n| &n.id == focused && n.focus_ring)
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
            *self.materialized.borrow_mut() = None;
            *self.materialized.borrow_mut() = crate::materialized::Cached::new(
                raw,
                &view,
                &result.scene,
                &result.nodes,
                &resolve,
                palette,
                viewport,
            )?;
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
            node.contains_in(r, x, y)
                && (node.action.is_some() || node.input.is_some() || node.scroll.is_some())
        })
    }
}
