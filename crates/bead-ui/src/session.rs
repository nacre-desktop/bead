// SPDX-License-Identifier: MIT OR Apache-2.0
//! Durable stores, editor/focus state and validate-then-commit reload transactions.
use super::*;
#[cfg(test)]
pub(crate) static RECOVERY_SERIALIZE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
fn recovery_fingerprint(bundle: &bead_core::recovery::Bundle) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    bundle.version.hash(&mut fingerprint);
    bundle.entry.hash(&mut fingerprint);
    bundle.palette.hash(&mut fingerprint);
    for (name, bytes) in &bundle.files {
        name.hash(&mut fingerprint);
        bytes.hash(&mut fingerprint);
    }
    fingerprint.finish()
}
pub(super) struct Pending {
    entry: PathBuf,
    pub(super) generation: Option<Generation>,
    animation_checkpoint: Option<frontend::AnimationSnapshot>,
    pub(super) stores: Stores,
    pub(super) palette: Palette,
    pub(super) base_palette: Palette,
    pub(super) runtime_palette: Option<Palette>,
    pub(super) palette_bytes: Option<Vec<u8>>,
    source_root: PathBuf,
    palette_source: PathBuf,
    pub(super) watch_revision: u64,
    pub(super) nodes: Vec<Node>,
}
pub type Projector = Box<dyn Fn(&Stores) -> Result<BTreeMap<String, Value>, Diagnostic>>;
/// Scope host-intercepted pointer actions so programmatic focus uses pointer styling.
pub struct PointerInput {
    state: std::rc::Rc<std::cell::Cell<bool>>,
    previous: bool,
}
impl Drop for PointerInput {
    fn drop(&mut self) {
        self.state.set(self.previous);
    }
}
/// The state owner survives every view/script replacement.
pub struct Session {
    font_picker: Option<crate::transcript::Picker>,
    projector: Option<Projector>,
    pointer_input: std::rc::Rc<std::cell::Cell<bool>>,
    pub(super) preview_deadline: Option<std::time::Instant>,
    frame_interval: std::time::Duration,
    pointer_inside: bool,
    pointer_seen: bool,
    edge_hover: bead_core::edge_hover::EdgeHover,
    coalesce_projection: bool,
    recovery_path: Option<PathBuf>,
    recovered: Option<bead_core::recovery::Restored>,
    recorded_revision: Option<u64>,
    recovery_fingerprint: Option<u64>,
    recovery_pending: std::collections::VecDeque<(u64, bead_core::recovery::Bundle, u64)>,
    pub(crate) root: PathBuf,
    pub(crate) entry: PathBuf,
    pub(super) palette_path: PathBuf,
    pub(super) palette: Palette,
    pub(super) base_palette: Palette,
    pub(super) runtime_palette: Option<Palette>,
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
    /// The last reload failed on the source itself (keeps the notice until a reload succeeds).
    pub(super) source_fault: bool,
    /// The last reload only overran the CPU budget: retry it on the next change.
    pub(super) retry_reload: bool,
    pub(super) attempts: u64,
    pub(super) watcher: Option<super::watcher::Subscription>,
    pub(super) restore: Option<BTreeMap<String, Value>>,
    pub(super) deferred_action: Option<DeferredAction>,
    pub(super) deferred_gesture: bool,
    pub(super) range_grab: Option<String>,
    pub(super) transfer_capture: Option<TransferCapture>,
}
pub(super) struct TransferCapture {
    pub(super) source: String,
    pub(super) action: String,
    pub(super) payload: Value,
    pub(super) origin: (f32, f32),
    pub(super) source_rect: (f32, f32, f32, f32),
    pub(super) active: bool,
}
pub(super) struct DeferredAction {
    pub(super) id: String,
    pub(super) action: String,
    pub(super) argument: Value,
    pub(super) pointer: (f32, f32),
}
impl Session {
    /// Keep the returned guard alive while routing an intercepted pointer action.
    pub fn pointer_input(&self) -> PointerInput {
        PointerInput {
            previous: self.pointer_input.replace(true),
            state: self.pointer_input.clone(),
        }
    }
    pub fn set_projector(&mut self, projector: Projector) {
        self.projector = Some(projector);
    }
    /// Set this session's intrinsic text measurement policy if none was supplied.
    /// Hosts call this before the first load; explicit test policies remain intact.
    pub fn set_font_picker_if_missing(&mut self, picker: crate::transcript::Picker) {
        if self.font_picker.is_none() {
            self.font_picker = Some(picker);
        }
    }
    /// Resolve host policy from the staged transaction, including computed values.
    pub fn resolve(&self, name: &str) -> Result<Value, Diagnostic> {
        let (generation, stores) = if let Some(pending) = &self.pending {
            (
                pending.generation.as_ref().or(self.generation.as_ref()),
                &pending.stores,
            )
        } else {
            (self.generation.as_ref(), &self.stores)
        };
        generation
            .ok_or_else(|| Diagnostic::new(&self.entry, 1, 1, "view is not loaded"))?
            .script
            .resolve(name, stores)
    }

    /// Run host-selected presentation feedback without consulting the external projector.
    ///
    /// The closure should change only pure local presentation input state, such as hover or
    /// pointer focus. Normal actions and model updates must keep the projector enabled. This
    /// opt-in scope lets a host avoid service work for transient local feedback.
    pub fn with_local_feedback(
        &mut self,
        feedback: impl FnOnce(&mut Session) -> Result<Option<Scene>, Diagnostic>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let projector = self.projector.take();
        let result = feedback(self);
        self.projector = projector;
        result
    }
    /// Recheck an asynchronous projection once after markup, without waiting.
    /// A reply already ready replaces the intermediate scene before raster work.
    pub fn set_projection_coalescing(&mut self, enabled: bool) {
        self.coalesce_projection = enabled;
    }
    pub fn new(
        root: PathBuf,
        entry: PathBuf,
        palette_path: PathBuf,
        palette: Palette,
        fallback: Scene,
    ) -> Self {
        Self {
            font_picker: None,
            projector: None,
            pointer_input: Default::default(),
            preview_deadline: None,
            frame_interval: std::time::Duration::from_millis(16),
            pointer_inside: false,
            pointer_seen: false,
            edge_hover: Default::default(),
            coalesce_projection: false,
            recovery_path: None,
            recovered: None,
            recorded_revision: None,
            recovery_fingerprint: None,
            recovery_pending: Default::default(),
            root,
            entry: entry
                .components()
                .filter(|c| *c != std::path::Component::CurDir)
                .collect(),
            palette_path,
            palette: palette.clone(),
            base_palette: palette,
            runtime_palette: None,
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
            source_fault: false,
            retry_reload: false,
            attempts: 0,
            watcher: None,
            restore: None,
            deferred_action: None,
            deferred_gesture: false,
            range_grab: None,
            transfer_capture: None,
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
    pub fn recovery_key(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .hash(&mut hash);
        self.entry.hash(&mut hash);
        format!("ui-{:016x}.json", hash.finish())
    }
    pub fn enable_recovery(&mut self, path: PathBuf) {
        self.recovery_path = Some(path);
    }
    pub fn recovered_on_start(&self) -> bool {
        self.recovered.is_some()
    }
    /// The host calls this only after a successful first-frame presentation.
    pub fn presented(&mut self, revision: u64) -> Result<(), Diagnostic> {
        let Some(path) = &self.recovery_path else {
            return Ok(());
        };
        if self
            .recorded_revision
            .is_some_and(|saved| saved >= revision)
        {
            return Ok(());
        }
        let Some((_, bundle, fingerprint)) = self
            .recovery_pending
            .iter()
            .find(|(id, _, _)| *id == revision)
        else {
            return Ok(());
        };
        if self.recovery_fingerprint != Some(*fingerprint) {
            #[cfg(test)]
            RECOVERY_SERIALIZE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            bundle.save(path).map_err(|_| {
                Diagnostic::new(&self.entry, 1, 1, "last-good bundle could not be saved")
            })?;
            self.recovery_fingerprint = Some(*fingerprint);
        }
        self.recorded_revision = Some(revision);
        self.recovery_pending.retain(|(id, _, _)| *id > revision);
        eprintln!("RECOVERY_BUNDLE_RECORDED generation={revision}");
        Ok(())
    }
    fn capture_bundle(
        &self,
        generation: &Generation,
        palette: &[u8],
        entry: &Path,
    ) -> Result<bead_core::recovery::Bundle, Diagnostic> {
        let size = generation.sources.values().map(Vec::len).sum::<usize>()
            + generation.assets.values().map(|b| b.len()).sum::<usize>()
            + palette.len();
        let pending = self
            .recovery_pending
            .iter()
            .map(|(_, b, _)| b.palette.len() + b.files.values().map(Vec::len).sum::<usize>())
            .sum::<usize>();
        if size > 8 * 1024 * 1024
            || pending + size > 8 * 1024 * 1024
            || self.recovery_pending.len() >= 32
        {
            return Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "recovery budget awaiting presentation",
            ));
        }
        let mut files = generation.sources.clone();
        for (path, bytes) in &generation.assets {
            let name = path
                .strip_prefix(&generation.root)
                .map_err(|_| Diagnostic::new(&self.entry, 1, 1, "recovery asset outside bundle"))?;
            files.insert(name.into(), bytes.to_vec());
        }
        let bundle = bead_core::recovery::Bundle {
            version: 1,
            entry: entry.into(),
            palette: palette.to_vec(),
            files,
        };
        bundle
            .validate()
            .map_err(|_| Diagnostic::new(&self.entry, 1, 1, "invalid recovery snapshot"))?;
        Ok(bundle)
    }
    pub fn reload(&mut self) -> Result<Scene, Diagnostic> {
        let result = self.reload_current();
        let overrun = result
            .as_ref()
            .err()
            .is_some_and(|e| e.message.contains("exceeded 50 ms"));
        self.source_fault = result.is_err() && !overrun;
        self.retry_reload = overrun;
        if result.is_ok() || self.generation.is_some() || self.recovered.is_some() {
            return result;
        }
        let Some(path) = &self.recovery_path else {
            return result;
        };
        let restore = (|| -> Result<_, Box<dyn std::error::Error>> {
            let bundle = bead_core::recovery::Bundle::load(path)?;
            if bundle.entry != self.entry {
                return Err("last-good entry mismatch".into());
            }
            bundle.restore(path.parent().ok_or("recovery parent missing")?)
        })();
        let Ok(restored) = restore else {
            return result;
        };
        let original_root = std::mem::replace(&mut self.root, restored.path.clone());
        let original_palette = std::mem::replace(
            &mut self.palette_path,
            restored.path.join(".bead-recovery-palette.json"),
        );
        let recovered = self.reload_current();
        self.root = original_root;
        self.palette_path = original_palette;
        if recovered.is_ok() {
            eprintln!("RECOVERY code=startup_bundle");
            self.recovered = Some(restored);
            recovered
        } else {
            result
        }
    }
    /// Stage another entry in the same session. Stores and the accepted entry stay
    /// unchanged until commit; a failed load keeps the previous generation usable.
    pub fn navigate(
        &mut self,
        entry: PathBuf,
        changes: BTreeMap<String, Value>,
    ) -> Result<Scene, Diagnostic> {
        let mut stores = self.stores.clone();
        stores
            .commit(changes)
            .map_err(|e| Diagnostic::new(&entry, 1, 1, e))?;
        let previous_entry = std::mem::replace(&mut self.entry, entry);
        let previous_stores = std::mem::replace(&mut self.stores, stores);
        let result = self.reload_current();
        self.entry = previous_entry;
        self.stores = previous_stores;
        result
    }
    fn reload_current(&mut self) -> Result<Scene, Diagnostic> {
        self.deferred_gesture = false;
        if self.deferred_action.take().is_some()
            && self.stores.get("bead.pressed") == Some(&Value::Bool(true))
        {
            self.stores
                .commit(BTreeMap::from([(
                    "bead.pressed".into(),
                    Value::Bool(false),
                )]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
        self.attempts += 1;
        self.pending = None;
        let watch_revision = self
            .watch_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        let palette_bytes = fs::read(&self.palette_path)
            .map_err(|e| Diagnostic::new(Path::new("palette.json"), 1, 1, e))?;
        let base_palette = Palette::parse(&palette_bytes)
            .map_err(|e| Diagnostic::new(Path::new("palette.json"), 1, 1, e))?;
        let palette = merged_palette(&base_palette, self.runtime_palette.as_ref());
        let (generation, mut stores) = Generation::load_with_picker(
            &self.root,
            &self.entry,
            &self.stores,
            self.attempts,
            self.font_picker.clone(),
        )?;
        generation.set_frame_interval(self.frame_interval);
        bead_core::profile::mark("generation_ready");
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
        if let Some(projector) = &self.projector {
            let _projection_profile = bead_core::profile::Span::new("projection");
            stores
                .commit(projector(&stores)?)
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

        bead_core::profile::mark("layout_ready");
        self.pending = Some(Pending {
            entry: self.entry.clone(),
            generation: Some(generation),
            animation_checkpoint: None,
            stores,
            palette,
            base_palette,
            runtime_palette: self.runtime_palette.clone(),
            palette_bytes: Some(palette_bytes),
            source_root: self.root.clone(),
            palette_source: self.palette_path.clone(),
            watch_revision,
            nodes,
        });
        Ok(scene)
    }
    pub fn commit(&mut self, scene: Scene) {
        if let Some(pending) = &self.pending {
            self.entry = pending.entry.clone();
        }
        let capture = self
            .pending
            .as_ref()
            .and_then(|p| p.generation.as_ref().map(|g| (p, g)))
            .filter(|(_, g)| {
                self.recovery_path.is_some()
                    && self.recovered.as_ref().is_none_or(|r| r.path != g.root)
            })
            .and_then(|(p, g)| {
                p.palette_bytes
                    .as_ref()
                    .and_then(|palette| self.capture_bundle(g, palette, &p.entry).ok())
                    .map(|bundle| {
                        let fingerprint = recovery_fingerprint(&bundle);
                        (g.revision, bundle, fingerprint)
                    })
            });
        if let Some(capture) = capture {
            self.recovery_pending.push_back(capture);
        }
        if let Some(Pending {
            generation,
            stores,
            palette,
            base_palette,
            runtime_palette,
            nodes,
            ..
        }) = self.pending.take()
        {
            if let Some(generation) = generation {
                if self
                    .recovered
                    .as_ref()
                    .is_some_and(|r| r.path != generation.root)
                {
                    self.recovered = None;
                }
                self.generation = Some(generation);
                self.error = None;
                self.source_fault = false;
                self.retry_reload = false;
            } else if !self.source_fault {
                // A later render succeeded: a transient failure's notice is stale.
                self.error = None;
            }
            self.stores = stores;
            self.palette = palette;
            self.base_palette = base_palette;
            self.runtime_palette = runtime_palette;
            self.current = scene;
            self.current_nodes = nodes;
            self.cancel_invalid_deferred();
            self.committed_editors = self.editors.clone();
        }
    }
    pub fn reject(&mut self, message: String) -> Scene {
        if let Some(Pending {
            generation: None,
            animation_checkpoint: Some(checkpoint),
            ..
        }) = self.pending.as_ref()
            && let Some(generation) = &self.generation
        {
            generation.restore_animations(checkpoint.clone());
        }
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
                weight: 400,
                align: None,
                size: 14.,
                color: [255, 225, 225, 255],
                shadow: None,
            });
        }
        scene
    }
    /// Earliest transient-feedback or Luau animation frame deadline.
    pub fn preview_deadline(&self) -> Option<std::time::Instant> {
        let preview = (self.stores.get("bead.pressed") != Some(&Value::Bool(true)))
            .then_some(self.preview_deadline)
            .flatten();
        let animation = self
            .generation
            .as_ref()
            .and_then(Generation::animation_deadline);
        [preview, animation].into_iter().flatten().min()
    }
    /// Duration of the current panel pose transition, used by hosts to keep
    /// view-local scalar animation timers out of compositor-driven slide frames.
    pub fn motion_duration(&self, opening: bool) -> Option<std::time::Duration> {
        let motion = self.current.motion.as_ref()?;
        let seconds = if opening {
            motion.duration
        } else {
            motion.return_duration.unwrap_or(motion.duration)
        };
        Some(std::time::Duration::from_secs_f64(seconds.max(0.)))
    }
    fn animation_checkpoint(&self) -> Option<frontend::AnimationSnapshot> {
        self.pending
            .as_ref()
            .filter(|pending| pending.generation.is_none())
            .and_then(|pending| pending.animation_checkpoint.clone())
            .or_else(|| {
                self.generation
                    .as_ref()
                    .and_then(Generation::animation_snapshot)
            })
    }
    /// Shared edge-panel hover controller. Authored `panel.close_on_leave` and
    /// `panel.leave_delay` select the policy; lip/body reentry or input focus holds it.
    pub fn pointer_region(&mut self, inside: bool) -> Result<Option<Scene>, Diagnostic> {
        self.pointer_inside = inside;
        self.pointer_seen = true;
        self.update(BTreeMap::new())
    }
    pub fn hover_close_deadline(&self) -> Option<std::time::Instant> {
        self.edge_hover.deadline()
    }
    pub(super) fn preview_changes(&mut self, key: String, value: f32) -> BTreeMap<String, Value> {
        if key.is_empty() {
            return BTreeMap::new();
        }
        self.preview_deadline = if self
            .current_nodes
            .iter()
            .any(|node| node.preview_key == key && node.preview_source.is_some())
        {
            None
        } else {
            Some(std::time::Instant::now() + std::time::Duration::from_millis(750))
        };
        BTreeMap::from([
            ("bead.preview.key".into(), Value::Text(key)),
            ("bead.preview.value".into(), Value::Number(f64::from(value))),
        ])
    }
    pub(super) fn stage_state(&mut self, stores: Stores) -> Result<Option<Scene>, Diagnostic> {
        let base = self.stores.revision();
        self.stage_state_from(stores, base)
    }
    /// Stage `stores`; `base` is the revision they were derived from (nothing to paint when it is unchanged).
    fn stage_state_from(&mut self, stores: Stores, base: u64) -> Result<Option<Scene>, Diagnostic> {
        if self.retry_reload {
            self.retry_reload = false;
            return self.reload().map(Some);
        }
        let checkpoint = self.animation_checkpoint();
        let result = self.stage_state_from_inner(stores, base, checkpoint.clone());
        if result.is_err()
            && let (Some(generation), Some(checkpoint)) = (&self.generation, checkpoint)
        {
            generation.restore_animations(checkpoint);
        }
        result
    }
    fn stage_state_from_inner(
        &mut self,
        stores: Stores,
        base: u64,
        animation_checkpoint: Option<frontend::AnimationSnapshot>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let mut stores = stores;
        if self.pointer_input.get() && stores.get("bead.focus_visible").is_some() {
            stores
                .commit(BTreeMap::from([(
                    "bead.focus_visible".into(),
                    Value::Bool(false),
                )]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
        if stores.get("bead.pressed") != Some(&Value::Bool(true))
            && self
                .preview_deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            self.preview_deadline = None;
            stores
                .commit(BTreeMap::from([(
                    "bead.preview.key".into(),
                    Value::Text(String::new()),
                )]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
        if let Some(projector) = &self.projector {
            let _projection_profile = bead_core::profile::Span::new("projection");
            stores
                .commit(projector(&stores)?)
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
        let hover_close = self.pointer_seen
            && generation
                .script
                .resolve("panel.close_on_leave", &stores)
                .ok()
                == Some(Value::Bool(true));
        let open = stores.get("panel.open") != Some(&Value::Bool(false));
        if !open {
            self.pointer_inside = false;
            self.pointer_seen = false;
        }
        let delay = generation
            .script
            .resolve("panel.leave_delay", &stores)
            .ok()
            .and_then(|value| match value {
                Value::Number(delay) if delay.is_finite() && (0.0..=60.0).contains(&delay) => {
                    Some(std::time::Duration::from_secs_f64(delay))
                }
                _ => None,
            });
        let focused = matches!(stores.get("bead.focused"), Some(Value::Text(id))
            if self.current_nodes.iter().any(|node| &node.id == id && node.input.is_some()));
        if self.edge_hover.update(
            open && hover_close && delay.is_some(),
            (
                stores.get("panel.anchor_hovered") == Some(&Value::Bool(true)),
                self.pointer_inside,
            ),
            focused,
            std::time::Instant::now(),
            delay.unwrap_or_default(),
        ) {
            self.pointer_seen = false;
            stores
                .commit(BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        }
        // A service notification may carry no local store edits. Consult the
        // external projector before deciding that its wakeup changed nothing.
        let animation_due = generation
            .animation_deadline()
            .is_some_and(|deadline| std::time::Instant::now() >= deadline);
        if stores.revision() == base && !animation_due {
            return Ok(None);
        }
        let (mut scene, mut nodes) =
            generation.materialize(&stores, &self.palette, self.viewport)?;
        if self.coalesce_projection
            && let Some(projector) = &self.projector
        {
            let revision = stores.revision();
            let _projection_profile = bead_core::profile::Span::new("projection");
            let changes = projector(&stores)?;
            stores
                .commit(changes)
                .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
            if stores.revision() != revision {
                (scene, nodes) = generation.materialize(&stores, &self.palette, self.viewport)?;
            }
        }
        if self.preview_deadline.is_none()
            && stores.get("bead.pressed") != Some(&Value::Bool(true))
            && let Some(Value::Text(key)) = stores.get("bead.preview.key")
            && !key.is_empty()
            && let Some(Value::Number(target)) = stores.get("bead.preview.value")
        {
            let source = nodes.iter().find(|node| node.preview_key == *key);
            let acknowledged = source.is_none_or(|node| {
                node.preview_source.is_some_and(|value| {
                    let expected = node.preview_expected.map(f64::from).unwrap_or(*target);
                    (f64::from(value) - expected).abs() <= f64::from(node.preview_tolerance)
                })
            });
            if acknowledged {
                stores
                    .commit(BTreeMap::from([(
                        "bead.preview.key".into(),
                        Value::Text(String::new()),
                    )]))
                    .map_err(|e| generation.script.error(e))?;
                (scene, nodes) = generation.materialize(&stores, &self.palette, self.viewport)?;
            }
        }
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

        if self.deferred_action.as_ref().is_some_and(|pending| {
            stores.get("bead.pressed") != Some(&Value::Bool(true))
                || !super::interaction::deferred_matches(&nodes, pending)
        }) {
            self.deferred_action = None;
            if stores.get("bead.pressed") == Some(&Value::Bool(true)) {
                stores
                    .commit(BTreeMap::from([(
                        "bead.pressed".into(),
                        Value::Bool(false),
                    )]))
                    .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
                (scene, nodes) = generation.materialize(&stores, &self.palette, self.viewport)?;
            }
        }

        scene.font_files = self.current.font_files.clone();
        if self.error.is_some() && self.source_fault {
            let old = self.current.clone();
            self.current = scene.clone();
            scene = self.error_scene();
            self.current = old;
        }
        self.pending = Some(Pending {
            entry: self.entry.clone(),
            generation: None,
            animation_checkpoint,
            stores,
            palette: Palette(self.palette.0.clone()),
            base_palette: self.base_palette.clone(),
            runtime_palette: self.runtime_palette.clone(),
            palette_bytes: None,
            source_root: self.root.clone(),
            palette_source: self.palette_path.clone(),
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
        if matches!(changes.get("panel.anchor_hovered"), Some(Value::Bool(_))) {
            self.pointer_seen = true;
        }
        // A change in the same event (a hover, then a service value) builds on the staged state, not on the
        // committed one, so the later scene still carries the earlier change.
        let mut stores = match &self.pending {
            Some(pending) if pending.generation.is_none() => pending.stores.clone(),
            _ => self.stores.clone(),
        };
        let base = stores.revision();
        stores
            .commit(changes)
            .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        if self.generation.is_none() {
            self.stores = stores;
            return Ok(None);
        }
        self.stage_state_from(stores, base)
    }
    /// Host-owned models stay current without painting when no binding reads them.
    /// Behavior/input updates continue through the ordinary staged transaction.
    pub fn update_model(
        &mut self,
        changes: BTreeMap<String, Value>,
    ) -> Result<Option<Scene>, Diagnostic> {
        // An empty/unused model delta may be a wakeup for external data. Read
        // its projector before deciding there is no view change.
        if self.projector.is_none()
            && self.edge_hover.deadline().is_none()
            && self
                .preview_deadline()
                .is_none_or(|deadline| std::time::Instant::now() < deadline)
            && self.pending.is_none()
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
    pub fn set_frame_interval(&mut self, interval: std::time::Duration) {
        let interval = if interval.is_zero() {
            std::time::Duration::from_millis(16)
        } else {
            interval
        };
        if self.frame_interval == interval {
            return;
        }
        self.frame_interval = interval;
        if let Some(generation) = &self.generation {
            generation.set_frame_interval(interval);
        }
        if let Some(generation) = self
            .pending
            .as_ref()
            .and_then(|pending| pending.generation.as_ref())
        {
            generation.set_frame_interval(interval);
        }
    }
    pub fn relayout(&mut self) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let checkpoint = self.animation_checkpoint();
        let materialized = generation.materialize(&self.stores, &self.palette, self.viewport);
        let (mut scene, nodes) = match materialized {
            Ok(result) => result,
            Err(error) => {
                if let Some(checkpoint) = checkpoint.clone() {
                    generation.restore_animations(checkpoint);
                }
                return Err(error);
            }
        };
        scene.font_files = self.current.font_files.clone();
        self.pending = Some(Pending {
            entry: self.entry.clone(),
            generation: None,
            animation_checkpoint: checkpoint,
            stores: self.stores.clone(),
            palette: Palette(self.palette.0.clone()),
            base_palette: self.base_palette.clone(),
            runtime_palette: self.runtime_palette.clone(),
            palette_bytes: None,
            source_root: self.root.clone(),
            palette_source: self.palette_path.clone(),
            watch_revision: 0,
            nodes,
        });
        Ok(Some(scene))
    }
    /// Stage daemon-provided presentation colors without writing the palette file.
    pub fn set_runtime_palette(&mut self, overlay: Palette) -> Result<Option<Scene>, Diagnostic> {
        let palette = merged_palette(&self.base_palette, Some(&overlay));
        let Some(generation) = &self.generation else {
            self.palette = palette;
            self.runtime_palette = Some(overlay);
            return Ok(None);
        };
        let checkpoint = self.animation_checkpoint();
        let materialized = generation.materialize(&self.stores, &palette, self.viewport);
        let (mut scene, nodes) = match materialized {
            Ok(result) => result,
            Err(error) => {
                if let Some(checkpoint) = checkpoint.clone() {
                    generation.restore_animations(checkpoint);
                }
                return Err(error);
            }
        };
        scene.font_files = self.current.font_files.clone();
        self.pending = Some(Pending {
            entry: self.entry.clone(),
            generation: None,
            animation_checkpoint: checkpoint,
            stores: self.stores.clone(),
            palette,
            base_palette: self.base_palette.clone(),
            runtime_palette: Some(overlay),
            palette_bytes: None,
            source_root: self.root.clone(),
            palette_source: self.palette_path.clone(),
            watch_revision: 0,
            nodes,
        });
        Ok(Some(scene))
    }
    /// Accepted layout bounds by stable authored ID, including noninteractive views.
    pub fn paint_bounds(&self, id: &str) -> Option<Rect> {
        self.current
            .layers
            .iter()
            .find(|layer| layer.id == id)
            .map(|layer| layer.bounds)
    }
    /// Resolve a view-authored paint expression against the accepted palette.
    pub fn paint_color(&self, value: &serde_json::Value) -> Option<[u8; 4]> {
        super::layout::color(
            Some(value),
            [0; 4],
            &self.palette,
            &super::layout::Source {
                file: self.entry.display().to_string(),
                line: 1,
            },
        )
        .ok()
    }
    pub fn view_nodes(&self) -> &[Node] {
        &self.current_nodes
    }
    pub fn check_sources(&self) -> Result<(), Diagnostic> {
        if let Some(pending) = &self.pending
            && let Some(generation) = &pending.generation
        {
            if self.recovery_path.is_some()
                && self
                    .recovered
                    .as_ref()
                    .is_none_or(|r| r.path != generation.root)
                && let Some(palette) = &pending.palette_bytes
            {
                self.capture_bundle(generation, palette, &pending.entry)?;
            }
            generation.check_sources(&pending.source_root)?;
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
                && fs::read(&pending.palette_source).ok().as_ref() != Some(bytes)
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
        let (compiled, values, markup_vm) = self
            .generation
            .as_ref()
            .map_or((0, 0, 0), Generation::markup_resource_bytes);
        format!(
            "stores={} nodes={} watchers={} subscriptions={} bindings={} sources={} luau_bytes={} markup_compiled_bytes={compiled} markup_values_bytes={values} markup_vm_build_bytes={markup_vm} source_bytes={}",
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
                .map_or(0, |g| g.script.memory_bytes()),
            self.generation.as_ref().map_or(0, |g| g
                .sources
                .values()
                .map(Vec::capacity)
                .sum::<usize>())
        )
    }
    /// Store dump for traces. Stores named `*.secret` (password buffers) are never included.
    pub fn state_summary(&self) -> String {
        let shown: BTreeMap<_, _> = self
            .stores
            .values()
            .iter()
            .filter(|(id, _)| !id.ends_with(SECRET_SUFFIX))
            .collect();
        serde_json::to_string(&shown).unwrap_or_default()
    }
}

/// Store ids with this suffix hold secrets: never traced, logged or persisted.
pub const SECRET_SUFFIX: &str = ".secret";

fn merged_palette(base: &Palette, overlay: Option<&Palette>) -> Palette {
    let Some(overlay) = overlay else {
        return base.clone();
    };
    let mut colours = base.0.as_ref().clone();
    colours.extend(overlay.0.iter().map(|(key, value)| (key.clone(), *value)));
    Palette(colours.into())
}
