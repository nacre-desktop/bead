// SPDX-License-Identifier: MIT OR Apache-2.0
//! Durable stores, editor/focus state and validate-then-commit reload transactions.
use super::*;
pub(super) struct Pending {
    pub(super) generation: Option<Generation>,
    pub(super) stores: Stores,
    pub(super) palette: Palette,
    pub(super) palette_bytes: Option<Vec<u8>>,
    pub(super) watch_revision: u64,
    pub(super) nodes: Vec<Node>,
}
/// The state owner survives every view/script replacement.
pub struct Session {
    pub(crate) root: PathBuf,
    pub(crate) entry: PathBuf,
    pub(super) palette_path: PathBuf,
    pub(super) palette: Palette,
    pub stores: Stores,
    pub(super) generation: Option<Generation>,
    pub(super) pending: Option<Pending>,
    pub(super) watch_revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(crate) current: Scene,
    pub(super) current_nodes: Vec<Node>,
    pub(super) editors: BTreeMap<String, input::Editor>,
    pub(super) committed_editors: BTreeMap<String, input::Editor>,
    pub(super) viewport: (f32, f32),
    pub(super) error: Option<String>,
    pub(super) attempts: u64,
    pub(super) watcher: Option<super::watcher::Subscription>,
    pub(super) restore: Option<BTreeMap<String, Value>>,
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
            restore: None,
        }
    }
    pub fn restore_on_start(&mut self, values: BTreeMap<String, Value>) {
        self.restore = Some(values);
    }
    pub fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let revision = self.watch_revision.clone();
        self.watcher = Some(super::watcher::subscribe(
            self.root.canonicalize()?,
            std::path::absolute(&self.palette_path)?,
            Box::new(move |event| {
                if event.is_ok() {
                    revision.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                wakeup(event);
            }),
        )?);
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
        if self.generation.is_none()
            && let Some(saved) = &self.restore
        {
            let compatible: BTreeMap<_, _> = saved
                .iter()
                .filter(|(id, value)| {
                    stores.get(id).is_some_and(|current| {
                        std::mem::discriminant(current) == std::mem::discriminant(*value)
                    })
                })
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
            stores
                .commit(compatible)
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
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
            scene.prepared.clear();
            if scene.motion.as_ref().is_some_and(|m| !m.open) {
                scene.primitives.clear();
                scene.layers.clear();
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
    pub(super) fn stage_state(&mut self, stores: Stores) -> Result<Option<Scene>, Diagnostic> {
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
    /// Host-owned models stay current without painting when no binding reads them.
    /// Behavior/input updates continue through the ordinary staged transaction.
    pub fn update_model(
        &mut self,
        changes: BTreeMap<String, Value>,
    ) -> Result<Option<Scene>, Diagnostic> {
        if self.pending.is_none()
            && let Some(generation) = &self.generation
            && !generation.model_affects_view(changes.iter().filter_map(|(id, value)| {
                (self.stores.get(id) != Some(value)).then_some(id.as_str())
            }))
        {
            self.stores
                .commit(changes)
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
            return Ok(None);
        }
        self.update(changes)
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
