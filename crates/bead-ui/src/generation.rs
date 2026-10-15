// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tracked source loading and format-independent generation materialization.
use super::*;
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
            markup_frontend: None,
            root: loader.root.clone(),
        };
        // Validate properties even when a node is currently hidden.
        generation.scene(&stores, &Palette(std::collections::HashMap::new()), false)?;
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
            let raw = self
                .markup_frontend
                .as_ref()
                .ok_or_else(|| Diagnostic::new(entry, 1, 1, "missing markup frontend"))?
                .build(entry, &resolve)?;
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
