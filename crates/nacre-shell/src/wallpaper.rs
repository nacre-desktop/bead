// SPDX-License-Identifier: GPL-3.0-only
//! Secure, descriptor-pinned decoding for the published wallpaper contract.
use bead::core::source::{Publisher, Source, Subscription};
use nacre_protocol::wallpaper::{ActiveWallpaper, WallpaperActive, WallpaperFileIdentity};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

fn supports_wallpaper_v1(status: &crate::nacre_source::Status) -> bool {
    status.topics.get("wallpaper.active") == Some(&1)
        && status
            .methods
            .get("wallpaper.activate")
            .is_some_and(|method| method.v == 1)
}

fn activation_reply_matches(
    output: &str,
    boot: &str,
    expected: &ActiveWallpaper,
    activated: &ActiveWallpaper,
    latest: &WallpaperActive,
    status: &crate::nacre_source::Status,
) -> bool {
    status.boot.as_deref() == Some(boot)
        && supports_wallpaper_v1(status)
        && latest.revision == activated.revision
        && latest.outputs.get(output) == Some(activated)
        && expected == activated
}

#[derive(Clone)]
struct PreviewSelection {
    requested_at: Instant,
    file_bytes: u64,
    source_width: u32,
    source_height: u32,
    boot: String,
    id: String,
    kind: String,
    video_predecode: Option<nacre_protocol::wallpaper::WallpaperVideoPredecode>,
    generation: u64,
}
pub(crate) type SwitchMetadata = (u64, u32, u32);
#[derive(Default)]
struct PreviewState {
    generation: u64,
    selected: Option<PreviewSelection>,
}

#[derive(Clone, Debug, PartialEq)]
struct PredecodeKey {
    generation: u64,
    boot: String,
    source: String,
    identity: WallpaperFileIdentity,
    decode_path: String,
    decode_identity: WallpaperFileIdentity,
    width: u32,
    height: u32,
    presentation: String,
}
#[derive(Clone)]
struct PredecodeTarget {
    boot: String,
    pair: Arc<ActiveWallpaper>,
    width: u32,
    height: u32,
}
struct PredecodeJob {
    key: PredecodeKey,
    pair: ActiveWallpaper,
}
struct CachedPredecode {
    key: PredecodeKey,
    rgba: Vec<u8>,
    expires: Instant,
}
#[derive(Default)]
struct PredecodeState {
    target: Option<PredecodeTarget>,
    desired: Option<PredecodeKey>,
    pending: Option<PredecodeJob>,
    active: Option<PredecodeKey>,
    cached: Option<CachedPredecode>,
    running: bool,
    stopping: bool,
}
#[derive(Default)]
struct PredecodeSignal {
    state: Mutex<PredecodeState>,
    changed: Condvar,
}
impl PredecodeSignal {
    fn lock(&self) -> std::sync::MutexGuard<'_, PredecodeState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}
#[derive(Default)]
struct Predecoder {
    signal: Arc<PredecodeSignal>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

const PREDECODE_WAIT: Duration = Duration::from_millis(150);
const PREDECODE_LIFETIME: Duration = Duration::from_secs(2);

impl Predecoder {
    fn target(&self) -> Option<PredecodeTarget> {
        self.signal.lock().target.clone()
    }

    fn register_target(&self, boot: String, pair: Arc<ActiveWallpaper>, width: u32, height: u32) {
        if width == 0 || height == 0 {
            let mut state = self.signal.lock();
            state.target = None;
            state.desired = None;
            state.pending = None;
            state.cached = None;
            self.signal.changed.notify_all();
            return;
        }
        let target = PredecodeTarget {
            boot,
            pair,
            width,
            height,
        };
        self.signal.lock().target = Some(target);
    }

    fn submit(&self, selection: PreviewSelection, target: PredecodeTarget) {
        let Some(job) = make_predecode_job(selection, target) else {
            return;
        };
        let mut state = self.signal.lock();
        if state.desired.as_ref() == Some(&job.key) {
            return;
        }
        state.desired = Some(job.key.clone());
        state.pending = Some(job);
        state.cached = None;
        if !state.running {
            let mut worker = self
                .worker
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(previous) = worker.take() {
                let _ = previous.join();
            }
            state.running = true;
            let signal = self.signal.clone();
            match std::thread::Builder::new()
                .name("wallpaper-predecode".into())
                .spawn(move || predecode_loop(signal))
            {
                Ok(handle) => *worker = Some(handle),
                Err(_) => {
                    state.running = false;
                    state.pending = None;
                    state.desired = None;
                    return;
                }
            }
        }
        self.signal.changed.notify_all();
    }

    fn take_confirmed(
        &self,
        selection: &PreviewSelection,
        boot: &str,
        pair: &ActiveWallpaper,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        if selection.boot != boot {
            return None;
        }
        let key = confirmed_predecode_key(selection, pair, width, height)?;
        let deadline = Instant::now() + PREDECODE_WAIT;
        let mut state = self.signal.lock();
        loop {
            if state
                .cached
                .as_ref()
                .is_some_and(|cached| cached.expires <= Instant::now())
            {
                state.cached = None;
            }
            if state
                .cached
                .as_ref()
                .is_some_and(|cached| cached.key == key)
            {
                let cached = state.cached.take()?;
                if identity_matches_path(&key.source, &key.identity)
                    && identity_matches_path(&key.decode_path, &key.decode_identity)
                {
                    return Some(cached.rgba);
                }
                return None;
            }
            if state.desired.as_ref() != Some(&key)
                || (state.active.as_ref() != Some(&key)
                    && state.pending.as_ref().is_none_or(|job| job.key != key))
            {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next, timeout) = self
                .signal
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner());
            state = next;
            if timeout.timed_out() {
                return None;
            }
        }
    }

    fn clear_generation(&self, generation: u64) {
        let mut state = self.signal.lock();
        if state
            .desired
            .as_ref()
            .is_some_and(|key| key.generation == generation)
        {
            state.desired = None;
            state.pending = None;
            state.cached = None;
            self.signal.changed.notify_all();
        }
    }

    fn clear(&self) {
        let mut state = self.signal.lock();
        state.desired = None;
        state.pending = None;
        state.cached = None;
        self.signal.changed.notify_all();
    }
}

impl Drop for Predecoder {
    fn drop(&mut self) {
        {
            let mut state = self.signal.lock();
            state.stopping = true;
            state.pending = None;
            state.cached = None;
            self.signal.changed.notify_all();
        }
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }
}

fn predecode_loop(signal: Arc<PredecodeSignal>) {
    // Picker warmup is background work; keep interactive UI and switch work ahead of it.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 10);
    }
    let mut idle_deadline = Instant::now() + PREDECODE_LIFETIME;
    loop {
        let job = {
            let mut state = signal.lock();
            loop {
                if state.stopping {
                    state.running = false;
                    signal.changed.notify_all();
                    return;
                }
                if let Some(job) = state.pending.take() {
                    state.active = Some(job.key.clone());
                    break job;
                }
                if state
                    .cached
                    .as_ref()
                    .is_some_and(|cached| cached.expires <= Instant::now())
                {
                    state.cached = None;
                }
                let remaining = idle_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    state.running = false;
                    state.active = None;
                    signal.changed.notify_all();
                    return;
                }
                let (next, _) = signal
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|poison| poison.into_inner());
                state = next;
            }
        };
        let result = {
            let _span = bead::core::profile::Span::new("wallpaper_predecode");
            decode_pair(job.pair, job.key.width, job.key.height).map(|decoded| decoded.rgba)
        };
        let mut state = signal.lock();
        state.active = None;
        idle_deadline = Instant::now() + PREDECODE_LIFETIME;
        if !state.stopping
            && state.desired.as_ref() == Some(&job.key)
            && let Ok(rgba) = result
        {
            state.cached = Some(CachedPredecode {
                key: job.key,
                rgba,
                expires: Instant::now() + PREDECODE_LIFETIME,
            });
        }
        signal.changed.notify_all();
    }
}

fn make_predecode_job(
    selection: PreviewSelection,
    target: PredecodeTarget,
) -> Option<PredecodeJob> {
    if selection.boot != target.boot || target.width == 0 || target.height == 0 {
        return None;
    }
    let source_identity = local_file_identity(&selection.id)?;
    let (decode_path, decode_identity) = if selection.kind == "video" {
        let descriptor = selection.video_predecode.as_ref()?;
        if descriptor.source_file_identity != source_identity
            || !identity_matches_path(&selection.id, &descriptor.source_file_identity)
            || !identity_matches_path(&descriptor.poster_path, &descriptor.poster_file_identity)
        {
            return None;
        }
        (
            descriptor.poster_path.clone(),
            descriptor.poster_file_identity.clone(),
        )
    } else {
        (selection.id.clone(), source_identity.clone())
    };
    let mut pair = target.pair.as_ref().clone();
    pair.path = decode_path.clone();
    pair.file_identity = decode_identity.clone();
    pair.motion = None;
    let key = PredecodeKey {
        generation: selection.generation,
        boot: selection.boot,
        source: selection.id,
        identity: source_identity,
        decode_path: decode_path.clone(),
        decode_identity,
        width: target.width,
        height: target.height,
        presentation: serde_json::to_string(&target.pair.presentation).ok()?,
    };
    Some(PredecodeJob { key, pair })
}

fn confirmed_predecode_key(
    selection: &PreviewSelection,
    pair: &ActiveWallpaper,
    width: u32,
    height: u32,
) -> Option<PredecodeKey> {
    let (identity, decode_identity) = if selection.kind == "video" {
        let descriptor = selection.video_predecode.as_ref()?;
        let motion = pair
            .motion
            .as_ref()
            .filter(|motion| motion.path == selection.id)?;
        if motion.file_identity != descriptor.source_file_identity
            || pair.path != descriptor.poster_path
            || pair.file_identity != descriptor.poster_file_identity
        {
            return None;
        }
        (
            descriptor.source_file_identity.clone(),
            descriptor.poster_file_identity.clone(),
        )
    } else if pair.path == selection.id {
        (pair.file_identity.clone(), pair.file_identity.clone())
    } else {
        return None;
    };
    let key = PredecodeKey {
        generation: selection.generation,
        boot: selection.boot.clone(),
        source: selection.id.clone(),
        identity,
        decode_path: pair.path.clone(),
        decode_identity,
        width,
        height,
        presentation: serde_json::to_string(&pair.presentation).ok()?,
    };
    (identity_matches_path(&key.source, &key.identity)
        && identity_matches_path(&key.decode_path, &key.decode_identity))
    .then_some(key)
}

fn local_file_identity(path: &str) -> Option<WallpaperFileIdentity> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(WallpaperFileIdentity::from_metadata(&metadata))
}

fn identity_matches_path(path: &str, identity: &WallpaperFileIdentity) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && identity.matches(&metadata))
}

/// One shared pending selection fence. Never display its image with the previous palette.
#[derive(Default)]
pub(crate) struct Preview {
    state: Mutex<PreviewState>,
    predecode: Predecoder,
}
impl Preview {
    #[cfg(test)]
    pub(crate) fn select(
        &self,
        boot: String,
        id: String,
        kind: String,
        video_predecode: Option<nacre_protocol::wallpaper::WallpaperVideoPredecode>,
    ) -> u64 {
        self.select_at(boot, id, kind, video_predecode, Instant::now(), (0, 0, 0))
    }

    pub(crate) fn select_at(
        &self,
        boot: String,
        id: String,
        kind: String,
        video_predecode: Option<nacre_protocol::wallpaper::WallpaperVideoPredecode>,
        requested_at: Instant,
        metadata: SwitchMetadata,
    ) -> u64 {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        // Repeated activation (for example Right followed by Enter) must not
        // discard the same in-flight poster and decode its pixels a second time.
        if let Some(selected) = &state.selected
            && selected.boot == boot
            && selected.id == id
            && selected.kind == kind
            && selected.video_predecode == video_predecode
        {
            return selected.generation;
        }
        state.generation = state.generation.saturating_add(1).max(1);
        let generation = state.generation;
        let selected = (boot.len() <= 128
            && !boot.is_empty()
            && valid_local_path(&id)
            && matches!(kind.as_str(), "image" | "video"))
        .then(|| PreviewSelection {
            requested_at,
            file_bytes: metadata.0,
            source_width: metadata.1,
            source_height: metadata.2,
            boot: boot.clone(),
            id: id.clone(),
            kind,
            video_predecode,
            generation,
        });
        state.selected = selected.clone();
        drop(state);
        self.predecode.clear();
        if let Some(selection) = selected
            && let Some(target) = self.predecode.target()
            && target.boot == selection.boot
        {
            self.predecode.submit(selection, target);
        }
        generation
    }
    pub(crate) fn cancel(&self, generation: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state
            .selected
            .as_ref()
            .is_some_and(|selected| selected.generation == generation)
        {
            return false;
        }
        state.selected = None;
        drop(state);
        self.predecode.clear_generation(generation);
        true
    }
    pub(crate) fn generation_for(&self, boot: &str, id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selected
            .as_ref()
            .filter(|selected| selected.boot == boot && selected.id == id)
            .map(|selected| selected.generation)
    }
    fn selected(&self) -> Option<PreviewSelection> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selected
            .clone()
    }

    fn register_target(
        &self,
        selection: Option<PreviewSelection>,
        boot: String,
        pair: Arc<ActiveWallpaper>,
        width: u32,
        height: u32,
    ) {
        self.predecode.register_target(boot, pair, width, height);
        if let Some(selection) = selection
            && let Some(target) = self.predecode.target()
            && target.boot == selection.boot
        {
            self.predecode.submit(selection, target);
        }
    }
}
fn valid_local_path(path: &str) -> bool {
    path.len() <= 4096
        && !path.is_empty()
        && !path.contains(['\0', '\n', '\r'])
        && !path.contains("://")
        && std::path::Path::new(path).is_absolute()
        && !std::path::Path::new(path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}
pub(crate) struct Consumer {
    client: crate::nacre_source::Client,
    topic: crate::nacre_source::Topic,
    preview: Arc<Preview>,
    subscriptions: Mutex<Vec<Subscription>>,
    displayed: Mutex<BTreeMap<String, ActiveWallpaper>>,
    motion_commits: Publisher,
    visibility: Mutex<BTreeMap<String, VisibilityRecord>>,
}
type VisibilityRecord = (u64, Instant, &'static str, Option<SwitchMetadata>);
struct PairToken {
    requested_at: Option<Instant>,
    boot: String,
    pair: ActiveWallpaper,
}
impl Consumer {
    #[cfg(test)]
    pub(crate) fn start(socket: std::path::PathBuf) -> Result<Arc<Self>, String> {
        Self::start_with_preview(socket, Arc::new(Preview::default()))
    }
    pub(crate) fn start_with_preview(
        socket: std::path::PathBuf,
        preview: Arc<Preview>,
    ) -> Result<Arc<Self>, String> {
        let client = crate::nacre_source::Client::start(crate::nacre_source::Config::new(
            socket,
            // The picker settings drive motion pause rules; a daemon without the topic is
            // simply not subscribed to it.
            vec![
                "daemon".into(),
                "wallpaper.active".into(),
                crate::prefs::WALLPAPER_PICKER.into(),
            ],
        ))
        .map_err(|_| "wallpaper daemon connection failed")?;
        let topic = client
            .topic("wallpaper.active")
            .map_err(|_| "wallpaper topic unavailable")?;
        let consumer = Arc::new(Self {
            client,
            topic,
            preview,
            subscriptions: Mutex::new(Vec::new()),
            displayed: Mutex::new(BTreeMap::new()),
            visibility: Mutex::new(BTreeMap::new()),
            motion_commits: Publisher::new(
                "wallpaper-motion-commits".into(),
                serde_json::json!({}),
            )
            .map_err(|error| error.to_string())?,
        });
        // Retain the reconnecting client even when the daemon starts later.
        // Contract checks belong to state/activation, never startup admission.
        Ok(consumer)
    }

    fn state(&self) -> Result<WallpaperActive, String> {
        let snapshot = Source::snapshot(&self.topic);
        if !snapshot.connected {
            return Err("wallpaper daemon disconnected".into());
        }
        let state: WallpaperActive = serde_json::from_value((*snapshot.data).clone())
            .map_err(|_| "wallpaper state malformed")?;
        state.validate().map_err(|_| "wallpaper state malformed")?;
        Ok(state)
    }

    /// Motion source of the active wallpaper: the additive optional `motion` member of the
    /// published `current` pair (a video or animation file for the poster being shown).
    /// `None` for a still wallpaper, an older daemon, or an unavailable/stale state.
    pub(crate) fn motion(&self) -> Option<bead_wallpaper::protocol::Source> {
        let status = self.client.status();
        if !supports_wallpaper_v1(&status) {
            return None;
        }
        let snapshot = Source::snapshot(&self.topic);
        if !snapshot.connected
            || snapshot.data["available"] != true
            || snapshot.data["stale"] == true
        {
            return None;
        }
        let state: WallpaperActive = serde_json::from_value((*snapshot.data).clone()).ok()?;
        state.validate().ok()?;
        let current = state.current.as_ref()?;
        let displayed = self.displayed.lock().ok()?;
        // One shared motion source: every output using it must first commit its
        // corresponding poster/palette pair. Other outputs do not hold it back.
        let mut matching = state
            .outputs
            .iter()
            .filter(|(_, pair)| pair.motion == current.motion)
            .peekable();
        if matching.peek().is_none()
            || !matching.all(|(output, pair)| displayed.get(output) == Some(pair))
            || status.boot.is_none()
            || self.client.status().boot != status.boot
        {
            return None;
        }
        parse_motion(&snapshot.data)
    }
    /// `settings.store.wallpaper-picker` on this connection (motion mode, pause, cover rule).
    pub(crate) fn picker(&self) -> Option<Arc<dyn Source>> {
        let topic = self.client.topic(crate::prefs::WALLPAPER_PICKER).ok()?;
        Some(Arc::new(topic))
    }
    pub(crate) fn watch_motion(
        &self,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Vec<Subscription>> {
        let committed_changed = changed.clone();
        let committed = self
            .motion_commits
            .subscribe(Arc::new(move |_| committed_changed()))
            .ok()?;
        let active = Source::subscribe(&self.topic, Arc::new(move |_| changed())).ok()?;
        Some(vec![committed, active])
    }

    #[cfg(test)]
    pub(crate) fn commit_token_for_test(
        &self,
        output: &str,
    ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        let state = self.state().ok()?;
        Some(Arc::new(PairToken {
            requested_at: None,
            boot: self.client.status().boot?,
            pair: state.outputs.get(output)?.clone(),
        }))
    }

    /// Palette is deliberately committed from the matching activation pair only
    /// after the corresponding background buffer is attached and committed.
    #[cfg(test)]
    pub(crate) fn displayed(&self, output: &str) -> Option<ActiveWallpaper> {
        self.displayed.lock().ok()?.get(output).cloned()
    }
}
fn wallpaper_paint_changed(
    previous: &Mutex<Option<(String, bool, serde_json::Value)>>,
    snapshot: &bead::core::source::Snapshot,
) -> bool {
    let next = if snapshot.connected {
        serde_json::from_value::<WallpaperActive>((*snapshot.data).clone())
            .ok()
            .filter(|state| state.validate().is_ok())
            .and_then(|_| {
                let mut data = (*snapshot.data).clone();
                data.as_object_mut()?
                    .insert("previous_available".into(), false.into());
                Some((snapshot.epoch.clone(), snapshot.connected, data))
            })
    } else {
        None
    };
    let mut previous = previous.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(next) = next else {
        *previous = None;
        return true;
    };
    if previous.as_ref() == Some(&next) {
        false
    } else {
        *previous = Some(next);
        true
    }
}

impl Consumer {
    fn prepare_inner(
        &self,
        output: String,
        width: u32,
        height: u32,
        staged: Option<&mut dyn FnMut(bead::wayland::WallpaperFrame)>,
    ) -> Result<Option<bead::wayland::WallpaperFrame>, String> {
        let status = self.client.status();
        if !supports_wallpaper_v1(&status) {
            return Err("wallpaper v1 contract unavailable".into());
        }
        let state = self.state()?;
        if !state.available || state.stale {
            return Err("wallpaper is stale".into());
        }
        let boot = status.boot.ok_or("wallpaper daemon boot unavailable")?;
        bead::core::profile::mark("wallpaper_decode_started");
        let pair = Arc::new(
            state
                .outputs
                .get(&output)
                .ok_or("output has no wallpaper")?
                .clone(),
        );
        let selection = self.preview.selected();
        if let Some(selection) = selection.as_ref().filter(|selection| {
            selection.boot == boot
                && state
                    .outputs
                    .get(&output)
                    .is_some_and(|pair| pair_matches_id(pair, &selection.id))
        }) {
            wallpaper_switch_phase(selection, pair.revision, "decode_start");
        }
        self.preview
            .register_target(selection.clone(), boot.clone(), pair.clone(), width, height);
        let predecoded = selection.as_ref().and_then(|selection| {
            self.preview
                .predecode
                .take_confirmed(selection, &boot, pair.as_ref(), width, height)
        });
        let decoded = if let Some(rgba) = predecoded {
            bead::core::profile::mark("wallpaper_predecoded");
            Decoded {
                width,
                height,
                rgba,
                pair: pair.as_ref().clone(),
            }
        } else {
            decode(&state, &output, width, height)?
        };
        bead::core::profile::mark("wallpaper_decode_finished");
        if let Some(selection) = selection.as_ref().filter(|selection| {
            selection.boot == boot && pair_matches_id(&decoded.pair, &selection.id)
        }) {
            wallpaper_switch_phase(selection, decoded.pair.revision, "decode_end");
            wallpaper_switch_phase(selection, decoded.pair.revision, "scale_end");
        }
        let decoded_pair = decoded.pair.clone();
        let commit_token: Arc<dyn std::any::Any + Send + Sync> = Arc::new(PairToken {
            requested_at: selection
                .as_ref()
                .filter(|selection| {
                    selection.boot == boot && pair_matches_id(&decoded_pair, &selection.id)
                })
                .map(|selection| selection.requested_at),
            boot: boot.clone(),
            pair: decoded_pair.clone(),
        });
        let frame = bead::wayland::WallpaperFrame {
            output: output.clone(),
            revision: decoded.pair.revision,
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
            previous_rgba: None,
            palette: palette(&decoded.pair),
            commit_token,
        };
        let frame = if let Some(staged) = staged {
            staged(frame);
            None
        } else {
            Some(frame)
        };
        let request = serde_json::json!({"revision": decoded_pair.revision});
        bead::core::profile::mark("wallpaper_activation_requested");
        if let Some(selection) = selection.as_ref().filter(|selection| {
            selection.boot == boot && pair_matches_id(&decoded_pair, &selection.id)
        }) {
            wallpaper_switch_phase(selection, decoded_pair.revision, "request_sent");
        }
        let reply = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|_| "wallpaper activation runtime unavailable")?
            .block_on(Source::call(
                &self.topic,
                "wallpaper.activate".into(),
                request,
                Duration::from_secs(1),
            ))
            .map_err(|_| "wallpaper activation failed")?;
        bead::core::profile::mark("wallpaper_activation_confirmed");
        if let Some(selection) = selection.as_ref().filter(|selection| {
            selection.boot == boot && pair_matches_id(&decoded_pair, &selection.id)
        }) {
            wallpaper_switch_phase(selection, decoded_pair.revision, "daemon_ack");
        }
        let activated: ActiveWallpaper =
            serde_json::from_value(reply).map_err(|_| "wallpaper activation response malformed")?;
        activated
            .validate()
            .map_err(|_| "wallpaper activation response malformed")?;
        let latest = self.state()?;
        let after_status = self.client.status();
        if !activation_reply_matches(
            &output,
            &boot,
            &decoded_pair,
            &activated,
            &latest,
            &after_status,
        ) {
            return Err("wallpaper activation superseded".into());
        }
        Ok(frame)
    }
}

impl bead::wayland::WallpaperProvider for Consumer {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let changed: Arc<dyn Fn() + Send + Sync> = Arc::from(changed);
        let active_changed = changed.clone();
        let previous = Arc::new(Mutex::new(None));
        let subscription = Source::subscribe(
            &self.topic,
            Arc::new(move |snapshot| {
                if wallpaper_paint_changed(&previous, &snapshot) {
                    active_changed();
                }
            }),
        )
        .map_err(|_| "wallpaper subscription failed")?;
        self.subscriptions
            .lock()
            .map_err(|_| "wallpaper subscription lock failed")?
            .push(subscription);
        Ok(())
    }
    fn prepare(
        &self,
        output: String,
        width: u32,
        height: u32,
    ) -> Result<bead::wayland::WallpaperFrame, String> {
        self.prepare_inner(output, width, height, None)?
            .ok_or_else(|| "wallpaper frame missing after activation".into())
    }
    fn prepare_staged(
        &self,
        output: String,
        width: u32,
        height: u32,
        staged: &mut dyn FnMut(bead::wayland::WallpaperFrame),
    ) -> Result<Option<bead::wayland::WallpaperFrame>, String> {
        self.prepare_inner(output, width, height, Some(staged))
    }
    fn prepare_transition(&self, output: &str, width: u32, height: u32) -> Option<Vec<u8>> {
        let previous = self.displayed.lock().ok()?.get(output).cloned()?;
        let state = self.state().ok()?;
        let current = state.outputs.get(output)?.clone();
        if previous == current {
            return None;
        }
        if previous.palette != current.palette {
            return None;
        }
        decode_pair(previous, width, height)
            .ok()
            .map(|decoded| decoded.rgba)
    }
    fn is_current(&self, frame: &bead::wayland::WallpaperFrame) -> bool {
        let status = self.client.status();
        let Some(token) = frame.commit_token.downcast_ref::<PairToken>() else {
            return false;
        };
        status.boot.as_deref() == Some(&token.boot)
            && supports_wallpaper_v1(&status)
            && self.preview.selected().is_none_or(|selection| {
                selection.boot != token.boot || pair_matches_id(&token.pair, &selection.id)
            })
            && self.state().is_ok_and(|state| {
                state.available
                    && !state.stale
                    && state.outputs.get(&frame.output) == Some(&token.pair)
            })
    }
    fn uploaded(&self, _output: &str, revision: u64, token: &Arc<dyn std::any::Any + Send + Sync>) {
        let Some(pair) = token.downcast_ref::<PairToken>() else {
            return;
        };
        if pair.pair.revision != revision {
            return;
        }
        if let Some(selection) = self.preview.selected().filter(|selection| {
            selection.boot == pair.boot && pair_matches_id(&pair.pair, &selection.id)
        }) {
            wallpaper_switch_phase(&selection, revision, "upload_end");
        }
    }
    fn committed(&self, output: &str, revision: u64, token: &Arc<dyn std::any::Any + Send + Sync>) {
        let Some(pair) = token.downcast_ref::<PairToken>() else {
            return;
        };
        let status = self.client.status();
        if pair.pair.revision != revision
            || status.boot.as_deref() != Some(&pair.boot)
            || !supports_wallpaper_v1(&status)
            || self.preview.selected().is_some_and(|selection| {
                selection.boot == pair.boot && !pair_matches_id(&pair.pair, &selection.id)
            })
            || status.topics.get("wallpaper.active") != Some(&1)
            || !status
                .methods
                .get("wallpaper.activate")
                .is_some_and(|method| method.v == 1)
        {
            return;
        }
        let previous_video = if let Ok(mut displayed) = self.displayed.lock() {
            displayed
                .insert(output.into(), pair.pair.clone())
                .is_some_and(|old| old.motion.is_some())
        } else {
            false
        };
        if let Some(selection) = self.preview.selected().filter(|selection| {
            selection.boot == pair.boot && pair_matches_id(&pair.pair, &selection.id)
        }) {
            wallpaper_switch_phase(&selection, revision, "commit");
        }
        let trace = self.preview.selected().filter(|selection| {
            selection.boot == pair.boot && pair_matches_id(&pair.pair, &selection.id)
        });
        if let Some(requested_at) = pair.requested_at
            && let Ok(mut visibility) = self.visibility.lock()
        {
            visibility.insert(
                output.into(),
                (
                    revision,
                    requested_at,
                    match (previous_video, pair.pair.motion.is_some()) {
                        (false, false) => "static-to-static",
                        (false, true) => "static-to-video-poster",
                        (true, false) => "video-to-static",
                        (true, true) => "video-to-video-poster",
                    },
                    trace.as_ref().map(|selection| {
                        (
                            selection.file_bytes,
                            selection.source_width,
                            selection.source_height,
                        )
                    }),
                ),
            );
        }
        if let Some(selection) = self.preview.selected().filter(|selection| {
            selection.boot == pair.boot && pair_matches_id(&pair.pair, &selection.id)
        }) {
            self.preview.cancel(selection.generation);
        }
        let _ = self
            .motion_commits
            .publish(serde_json::json!({"output":output,"revision":revision}));
    }
    fn presented(&self, output: &str, revision: u64) {
        let Ok(mut visibility) = self.visibility.lock() else {
            return;
        };
        let Some(&(wanted, requested_at, path, metadata)) = visibility.get(output) else {
            return;
        };
        if revision != wanted {
            return;
        }
        visibility.remove(output);
        if let Some((file_bytes, source_width, source_height)) = metadata {
            println!(
                "WALLPAPER_SWITCH_PHASE revision={} phase=presented ms_since_selection={:.3} file_bytes={} source_width={} source_height={}",
                revision,
                requested_at.elapsed().as_secs_f64() * 1000.0,
                file_bytes,
                source_width,
                source_height,
            );
        }
        println!(
            "WALLPAPER_SWITCH_LATENCY output={output} revision={revision} path={path} request=selection endpoint=presentation-feedback ms={:.3}",
            requested_at.elapsed().as_secs_f64() * 1000.
        );
    }
    fn discarded(&self, _output: &str, _revision: u64) {}
    fn output_removed(&self, output: &str) {
        if let Ok(mut visibility) = self.visibility.lock() {
            visibility.remove(output);
        }
        if let Ok(mut displayed) = self.displayed.lock() {
            displayed.remove(output);
        }
    }
}

fn wallpaper_switch_phase(selection: &PreviewSelection, revision: u64, phase: &str) {
    println!(
        "WALLPAPER_SWITCH_PHASE revision={} phase={} ms_since_selection={:.3} file_bytes={} source_width={} source_height={}",
        revision,
        phase,
        selection.requested_at.elapsed().as_secs_f64() * 1000.0,
        selection.file_bytes,
        selection.source_width,
        selection.source_height,
    );
}

/// `current.motion` = `{kind: "video"|"animation", path, file_identity}`. Same path rules as
/// the poster (absolute, local, no `..`), a regular non-symlink file, and the published
/// identity must still match, so a swapped file is never played.
pub(crate) fn parse_motion(state: &serde_json::Value) -> Option<bead_wallpaper::protocol::Source> {
    use nacre_protocol::wallpaper::WallpaperFileIdentity;
    if state.get("available")? != true || state.get("stale")? != false {
        return None;
    }
    let motion = state.get("current")?.get("motion")?;
    let path = motion.get("path")?.as_str()?;
    if !matches!(motion.get("kind")?.as_str()?, "video" | "animation")
        || path.len() > 4096
        || path.contains(['\0', '\n'])
        || path.contains("://")
        || !std::path::Path::new(path).is_absolute()
        || std::path::Path::new(path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let identity: WallpaperFileIdentity =
        serde_json::from_value(motion.get("file_identity")?.clone()).ok()?;
    let linked = std::fs::symlink_metadata(path).ok()?;
    if !linked.is_file() || !identity.matches(&linked) {
        return None;
    }
    Some(bead_wallpaper::protocol::Source::Video {
        path: path.to_owned(),
        fit: bead_wallpaper::protocol::Fit::Crop,
        looped: true,
    })
}

fn pair_matches_id(pair: &ActiveWallpaper, id: &str) -> bool {
    pair.path == id || pair.motion.as_ref().is_some_and(|motion| motion.path == id)
}
pub(crate) struct Decoded {
    pub width: u32,
    pub height: u32,
    /// Output-sized RGBA bytes; no pathname or source-sized image is retained.
    pub rgba: Vec<u8>,
    pub pair: ActiveWallpaper,
}

/// Decode one active pair only after validating the complete contract snapshot.
/// The short-lived worker streams from a pinned descriptor and scales during
/// decoding; the UI receives only bounded output pixels.
pub(crate) fn decode(
    state: &WallpaperActive,
    output: &str,
    width: u32,
    height: u32,
) -> Result<Decoded, String> {
    state.validate().map_err(|_| "invalid wallpaper state")?;
    if !state.available || state.stale || width == 0 || height == 0 {
        return Err("wallpaper unavailable".into());
    }
    let pair = state
        .outputs
        .get(output)
        .ok_or("output has no wallpaper")?
        .clone();
    decode_pair(pair, width, height)
}

pub(crate) fn decode_pair(
    pair: ActiveWallpaper,
    width: u32,
    height: u32,
) -> Result<Decoded, String> {
    let rgba = crate::wallpaper_cache::load_or_decode(&pair, width, height, || {
        #[cfg(test)]
        {
            crate::wallpaper_decode::decode_pair(pair.clone(), width, height)
        }
        #[cfg(not(test))]
        {
            decode_in_worker(&pair, width, height)
        }
    })?;
    Ok(Decoded {
        width,
        height,
        rgba,
        pair,
    })
}

#[cfg(not(test))]
fn decode_in_worker(pair: &ActiveWallpaper, width: u32, height: u32) -> Result<Vec<u8>, String> {
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
    };
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n > 0 && *n <= 64 * 1024 * 1024)
        .ok_or("output dimensions exceed wallpaper bound")?;
    if width > 16_384 || height > 16_384 {
        return Err("output dimensions exceed wallpaper bound".into());
    }
    let executable = std::env::current_exe()
        .map_err(|_| "wallpaper worker unavailable")?
        .with_file_name("nacre-wallpaper-decode");
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if bead::core::profile::enabled() {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()
        .map_err(|_| "wallpaper worker unavailable")?;
    let result = (|| {
        let mut input = child
            .stdin
            .take()
            .ok_or("wallpaper worker input unavailable")?;
        let request =
            serde_json::to_vec(&(pair, width, height)).map_err(|_| "invalid wallpaper request")?;
        input
            .write_all(&request)
            .map_err(|_| "wallpaper worker input failed")?;
        drop(input);
        let mut rgba = vec![0; expected];
        let mut output = child
            .stdout
            .take()
            .ok_or("wallpaper worker output unavailable")?;
        output
            .read_exact(&mut rgba)
            .map_err(|_| "wallpaper worker output failed")?;
        if output
            .read(&mut [0])
            .map_err(|_| "wallpaper worker output failed")?
            != 0
        {
            return Err("wallpaper worker output size mismatch");
        }
        Ok(rgba)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|_| "wallpaper worker wait failed")?;
    if !status.success() {
        return Err("wallpaper decode failed".into());
    }
    result.map_err(String::from)
}

pub(crate) fn palette(pair: &ActiveWallpaper) -> bead::render::Palette {
    let colours = pair
        .palette
        .colours
        .iter()
        .filter_map(|(role, value)| {
            u32::from_str_radix(value.trim_start_matches('#'), 16)
                .ok()
                .map(|rgb| {
                    (
                        role.clone(),
                        [
                            ((rgb >> 16) & 255) as u8,
                            ((rgb >> 8) & 255) as u8,
                            (rgb & 255) as u8,
                            255,
                        ],
                    )
                })
        })
        .collect();
    bead::render::Palette(std::sync::Arc::new(colours))
}

/// B0068: the live palette of the `wallpaper.active` topic for an app process (no wallpaper
/// surface of its own). A stale or unavailable state keeps the last palette the app wore.
pub(crate) struct Feed {
    topic: crate::nacre_source::Topic,
    subscription: Mutex<Option<Subscription>>,
}
impl Feed {
    pub(crate) fn new(topic: crate::nacre_source::Topic) -> Arc<Self> {
        Arc::new(Self {
            topic,
            subscription: Mutex::new(None),
        })
    }
}
fn live(snapshot: &bead::core::source::Snapshot) -> Option<BTreeMap<String, [u8; 4]>> {
    if !snapshot.connected {
        return None;
    }
    let state: WallpaperActive = serde_json::from_value((*snapshot.data).clone()).ok()?;
    if !state.available || state.stale {
        return None;
    }
    let pair = state
        .current
        .or_else(|| state.outputs.into_values().next())?;
    Some(
        palette(&pair)
            .0
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
    )
}
impl bead::wayland::PaletteFeed for Feed {
    fn current(&self) -> Option<BTreeMap<String, [u8; 4]>> {
        live(&Source::snapshot(&self.topic))
    }
    fn watch(&self, changed: Box<dyn Fn(BTreeMap<String, [u8; 4]>) + Send + Sync>) {
        let subscription = Source::subscribe(
            &self.topic,
            Arc::new(move |snapshot| {
                if let Some(palette) = live(&snapshot) {
                    changed(palette);
                }
            }),
        );
        *self.subscription.lock().unwrap_or_else(|p| p.into_inner()) = subscription.ok();
    }
}

/// Select an applicable topic revision; unavailable input never erases a good
/// displayed pair. Activation results are accepted only if still current.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct CommitState {
    pub pending_revision: u64,
    pub displayed: Option<ActiveWallpaper>,
}
#[cfg(test)]
impl CommitState {
    pub fn observe(&mut self, state: &WallpaperActive) -> Option<u64> {
        if state.validate().is_err() || !state.available || state.stale {
            return None;
        }
        let pair = state.current.as_ref()?;
        if self
            .displayed
            .as_ref()
            .is_some_and(|shown| shown.revision == pair.revision)
        {
            return None;
        }
        self.pending_revision = pair.revision;
        Some(pair.revision)
    }
    pub fn activate(&mut self, revision: u64, pair: ActiveWallpaper) -> bool {
        if revision != self.pending_revision
            || pair.revision != revision
            || pair.palette_revision != revision
            || pair.validate().is_err()
        {
            return false;
        }
        self.displayed = Some(pair);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::wayland::WallpaperProvider as _;
    use image::ImageEncoder;
    use nacre_protocol::{Capability, Message, Method, Reply, framing};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn stale_unavailable_and_superseded_activation_keep_last_good_pair() {
        let mut state = CommitState::default();
        let first = pair(1);
        state.pending_revision = 1;
        assert!(state.activate(1, first.clone()));
        assert_eq!(state.displayed.as_ref().unwrap().palette, first.palette);
        let second = pair(2);
        let active = WallpaperActive {
            available: true,
            stale: false,
            previous_available: false,
            revision: 2,
            current: Some(second.clone()),
            outputs: BTreeMap::from([("DP-1".into(), second.clone())]),
            error: None,
        };
        assert_eq!(state.observe(&active), Some(2));
        // A failed activation has no commit call, so both image token and palette stay old.
        assert_eq!(state.displayed.as_ref().unwrap().revision, 1);
        assert!(!state.activate(3, second.clone()));
        assert_eq!(state.displayed, Some(first));
        assert!(
            state
                .observe(&WallpaperActive {
                    available: false,
                    stale: true,
                    previous_available: false,
                    revision: 1,
                    current: None,
                    outputs: Default::default(),
                    error: None
                })
                .is_none()
        );
        assert_eq!(state.displayed.as_ref().unwrap().revision, 1);
        assert!(state.activate(2, second.clone()));
        assert_eq!(state.displayed, Some(second));
    }
    #[test]
    fn runtime_pixbuf_modules_cover_every_contract_codec() {
        let available: Vec<String> = gdk_pixbuf::Pixbuf::formats()
            .into_iter()
            .filter_map(|format| format.name().map(|name| name.to_string()))
            .collect();
        for codec in ["png", "jpeg", "webp", "gif", "tiff"] {
            assert!(
                available.iter().any(|name| name == codec),
                "missing GdkPixbuf loader {codec}: {available:?}"
            );
        }
    }

    #[test]
    fn wallpaper_topic_and_method_require_exact_published_v1() {
        let mut status = crate::nacre_source::Status::default();
        assert!(!supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 1);
        assert!(!supports_wallpaper_v1(&status));
        status.methods.insert(
            "wallpaper.activate".into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 1000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
        assert!(supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 2);
        assert!(!supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 1);
        status.methods.get_mut("wallpaper.activate").unwrap().v = 2;
        assert!(!supports_wallpaper_v1(&status));
    }

    #[test]
    fn activation_reply_rejects_boot_and_revision_supersession() {
        let expected = pair(1);
        let latest = active(expected.clone());
        let mut status = crate::nacre_source::Status {
            boot: Some("boot-a".into()),
            topics: BTreeMap::from([("wallpaper.active".into(), 1)]),
            ..Default::default()
        };
        status.methods.insert(
            "wallpaper.activate".into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 1000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
        assert!(activation_reply_matches(
            "DP-1", "boot-a", &expected, &expected, &latest, &status
        ));
        assert!(!activation_reply_matches(
            "DP-1", "boot-b", &expected, &expected, &latest, &status
        ));
        assert!(!activation_reply_matches(
            "DP-1",
            "boot-a",
            &expected,
            &pair(2),
            &latest,
            &status
        ));
    }

    #[test]
    fn decoder_rejects_malformed_bytes_identity_mismatch_and_symlink_paths() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let malformed = root.join("malformed.png");
        std::fs::write(&malformed, b"not an image").unwrap();
        std::fs::set_permissions(&malformed, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut malformed_pair = pair(1);
        malformed_pair.path = malformed.to_string_lossy().into_owned();
        malformed_pair.file_identity =
            nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&malformed).unwrap(),
            );
        let state = active(malformed_pair.clone());
        assert!(decode(&state, "DP-1", 2, 1).is_err());

        let other = root.join("other.png");
        std::fs::write(&other, b"other bytes").unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o600)).unwrap();
        malformed_pair.file_identity =
            nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&other).unwrap(),
            );
        assert!(decode(&active(malformed_pair), "DP-1", 2, 1).is_err());

        let alias = root.join("alias.png");
        symlink(&malformed, &alias).unwrap();
        let mut alias_pair = pair(1);
        alias_pair.path = alias.to_string_lossy().into_owned();
        alias_pair.file_identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&malformed).unwrap(),
        );
        assert!(decode(&active(alias_pair), "DP-1", 2, 1).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn private_bus_snapshot_activation_and_frame_commit_keep_the_pair_atomic() {
        use tokio::net::UnixListener;
        let root = std::env::temp_dir().join(format!("bead-wallpaper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("poster.png");
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[30, 80, 120, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&path).unwrap(),
        );
        let pair = ActiveWallpaper {
            revision: 1,
            palette_revision: 1,
            path: path.to_string_lossy().into_owned(),
            file_identity: identity,
            presentation: nacre_protocol::wallpaper::Presentation::Crop,
            motion: None,
            palette: nacre_protocol::wallpaper::WallpaperPalette {
                mode: nacre_protocol::wallpaper::PaletteMode::Dark,
                colours: BTreeMap::from([
                    ("background".into(), "#101820".into()),
                    ("onSurface".into(), "#f0f4f8".into()),
                    ("primary".into(), "#22aa88".into()),
                ]),
            },
        };
        let state = WallpaperActive {
            available: true,
            stale: false,
            previous_available: false,
            revision: 1,
            current: Some(pair.clone()),
            outputs: BTreeMap::from([("DP-1".into(), pair.clone())]),
            error: None,
        };
        let expected_pair = pair.clone();
        let socket = root.join("daemon.sock");
        let socket_for_client = socket.clone();
        let preview = Arc::new(Preview::default());
        let preview_for_client = preview.clone();
        let consumer = tokio::task::spawn_blocking(move || {
            Consumer::start_with_preview(socket_for_client, preview_for_client).unwrap()
        })
        .await
        .unwrap();
        assert!(!consumer.topic.snapshot().connected);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let (staged_call_tx, staged_call_rx) = tokio::sync::oneshot::channel::<()>();
        let (staged_release_tx, staged_release_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut reader = framing::Reader::new(tokio::io::BufReader::new(read));
            assert!(matches!(
                reader.next().await.unwrap().unwrap(),
                Message::Hello { .. }
            ));
            let mut methods = nacre_protocol::core_methods();
            // This focused peer implements only activation, not readiness
            // discovery for the consumer's other optional topics.
            methods.remove("daemon.status");
            methods.insert(
                "wallpaper.activate".into(),
                Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout: 1000,
                    params: serde_json::json!({"type":"object"}),
                    result: serde_json::json!({}),
                },
            );
            framing::write(
                &mut write,
                &Message::Welcome {
                    proto: 1,
                    min: 1,
                    boot: "00000000-0000-4000-8000-000000000021".into(),
                    topics: BTreeMap::from([("wallpaper.active".into(), 1)]),
                    methods,
                    frame_ms: Some(16),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                reader.next().await.unwrap().unwrap(),
                Message::Sub {
                    topics: vec!["wallpaper.active".into()]
                }
            );
            framing::write(
                &mut write,
                &Message::Snap {
                    topic: "wallpaper.active".into(),
                    seq: 0,
                    data: serde_json::to_value(&state).unwrap(),
                },
            )
            .await
            .unwrap();
            let mut staged_call_tx = Some(staged_call_tx);
            let mut staged_release_rx = Some(staged_release_rx);
            for call_index in 0..4 {
                let Message::Call { id, m, p, .. } = reader.next().await.unwrap().unwrap() else {
                    panic!("wallpaper.activate call expected");
                };
                assert_eq!(m, "wallpaper.activate");
                assert_eq!(p, serde_json::json!({"revision":1}));
                if call_index == 2 {
                    let _ = staged_call_tx.take().unwrap().send(());
                    let _ = staged_release_rx.take().unwrap().await;
                }
                let reply_pair = if call_index == 3 {
                    let mut pair = pair.clone();
                    pair.revision = 2;
                    pair.palette_revision = 2;
                    pair
                } else {
                    pair.clone()
                };
                framing::write(
                    &mut write,
                    &Message::Ret {
                        id,
                        reply: Reply::Ok {
                            ok: serde_json::to_value(&reply_pair).unwrap(),
                        },
                    },
                )
                .await
                .unwrap();
            }
            let _ = release_rx.await;
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !consumer.topic.snapshot().connected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let selected_id = root.join("selected.jpg").to_string_lossy().into_owned();
        let generation = preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            selected_id.clone(),
            "image".into(),
            None,
        );
        let prepare_consumer = consumer.clone();
        let frame = tokio::task::spawn_blocking(move || {
            prepare_consumer.prepare("DP-1".into(), 2, 1).unwrap()
        })
        .await
        .unwrap();
        let token = Arc::downgrade(&frame.commit_token);
        assert_eq!(frame.revision, 1);
        assert_eq!(frame.rgba.len(), 8);
        assert!(frame.commit_token.is::<PairToken>());
        assert!(
            !bead::wayland::WallpaperProvider::is_current(&*consumer, &frame),
            "a pending selection must not expose a different image with the old palette"
        );
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            frame.revision,
            &frame.commit_token,
        );
        assert!(consumer.displayed("DP-1").is_none());
        assert_eq!(
            preview.generation_for("00000000-0000-4000-8000-000000000021", &selected_id),
            Some(generation)
        );
        let superseding = preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            root.join("second-selection.jpg")
                .to_string_lossy()
                .into_owned(),
            "image".into(),
            None,
        );
        assert!(superseding > generation);
        assert!(!bead::wayland::WallpaperProvider::is_current(
            &*consumer, &frame
        ));
        assert!(preview.cancel(superseding));
        preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            expected_pair.path.clone(),
            "image".into(),
            None,
        );
        let prepare_consumer = consumer.clone();
        let confirmed = tokio::task::spawn_blocking(move || {
            prepare_consumer.prepare("DP-1".into(), 2, 1).unwrap()
        })
        .await
        .unwrap();
        assert!(confirmed.commit_token.is::<PairToken>());
        assert!(bead::wayland::WallpaperProvider::is_current(
            &*consumer, &confirmed
        ));
        preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            expected_pair.path.clone(),
            "image".into(),
            None,
        );
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            confirmed.revision,
            &confirmed.commit_token,
        );
        assert!(
            preview.selected().is_none(),
            "a successful commit releases the selection fence for future external updates"
        );
        assert_eq!(consumer.displayed("DP-1"), Some(expected_pair.clone()));
        assert!(consumer.visibility.lock().unwrap().contains_key("DP-1"));
        bead::wayland::WallpaperProvider::presented(&*consumer, "DP-1", confirmed.revision + 1);
        assert!(
            consumer.visibility.lock().unwrap().contains_key("DP-1"),
            "unrelated feedback cannot report visibility"
        );
        bead::wayland::WallpaperProvider::presented(&*consumer, "DP-1", confirmed.revision);
        assert!(
            consumer.visibility.lock().unwrap().is_empty(),
            "matching presentation reports once and releases timing state"
        );

        let (staged_frame_tx, staged_frame_rx) = tokio::sync::oneshot::channel();
        let staged_consumer = consumer.clone();
        let staged_prepare = tokio::task::spawn_blocking(move || {
            let mut staged_frame_tx = Some(staged_frame_tx);
            staged_consumer.prepare_staged("DP-1".into(), 2, 1, &mut |frame| {
                staged_frame_tx.take().unwrap().send(frame).unwrap();
            })
        });
        let staged_frame = tokio::time::timeout(Duration::from_secs(2), staged_frame_rx)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), staged_call_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(
            !staged_prepare.is_finished(),
            "pixels arrive while the activation call is still awaiting its reply"
        );
        assert!(bead::wayland::WallpaperProvider::is_current(
            &*consumer,
            &staged_frame
        ));
        let _ = staged_release_tx.send(());
        let result = tokio::time::timeout(Duration::from_secs(2), staged_prepare)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            result.is_none(),
            "staged pixels return only after confirmation"
        );
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            staged_frame.revision,
            &staged_frame.commit_token,
        );
        assert_eq!(consumer.displayed("DP-1"), Some(expected_pair.clone()));

        let (failed_frame_tx, failed_frame_rx) = tokio::sync::oneshot::channel();
        let failed_consumer = consumer.clone();
        let failed_prepare = tokio::task::spawn_blocking(move || {
            let mut failed_frame_tx = Some(failed_frame_tx);
            failed_consumer.prepare_staged("DP-1".into(), 2, 1, &mut |frame| {
                failed_frame_tx.take().unwrap().send(frame).unwrap();
            })
        });
        let failed_frame = tokio::time::timeout(Duration::from_secs(2), failed_frame_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), failed_prepare)
                .await
                .unwrap()
                .unwrap()
                .is_err(),
            "a staged pair is rejected when activation returns a newer revision"
        );
        assert_eq!(consumer.displayed("DP-1"), Some(expected_pair.clone()));
        drop(confirmed);
        drop(frame);
        drop(staged_frame);
        drop(failed_frame);
        assert!(
            token.upgrade().is_none(),
            "dropping the frame releases its pending raster token"
        );
        bead::wayland::WallpaperProvider::output_removed(&*consumer, "DP-1");
        assert!(
            consumer.displayed("DP-1").is_none(),
            "output removal must release the committed image/palette pair"
        );
        let _ = release_tx.send(());
        server.await.unwrap();
        drop(consumer);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn wallpaper_watch_filters_only_readiness_only_updates() {
        let initial = WallpaperActive {
            available: true,
            stale: false,
            previous_available: false,
            revision: 4,
            current: Some(pair(4)),
            outputs: BTreeMap::from([("DP-1".into(), pair(4))]),
            error: None,
        };
        let publisher = Publisher::new(
            "wallpaper-watch-boot".into(),
            serde_json::to_value(&initial).unwrap(),
        )
        .unwrap();
        let previous = Arc::new(Mutex::new(None));
        let callbacks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let previous_for_callback = previous.clone();
        let callbacks_for_callback = callbacks.clone();
        let subscription = publisher
            .subscribe(Arc::new(move |snapshot| {
                if wallpaper_paint_changed(&previous_for_callback, &snapshot) {
                    callbacks_for_callback.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }))
            .unwrap();
        assert_eq!(
            callbacks.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "subscribe delivers initial state"
        );
        let mut ready = initial.clone();
        ready.previous_available = true;
        publisher
            .publish(serde_json::to_value(&ready).unwrap())
            .unwrap();
        assert_eq!(
            callbacks.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "readiness-only patch is suppressed"
        );
        let mut changed = ready.clone();
        changed.revision += 1;
        changed.current.as_mut().unwrap().revision += 1;
        changed.outputs.get_mut("DP-1").unwrap().revision += 1;
        changed.current.as_mut().unwrap().path = "/wallpaper/next.png".into();
        changed.outputs.get_mut("DP-1").unwrap().path = "/wallpaper/next.png".into();
        publisher
            .publish(serde_json::to_value(&changed).unwrap())
            .unwrap();
        assert_eq!(
            callbacks.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "pair changes repaint"
        );
        publisher
            .restart("wallpaper-watch-next-boot".into())
            .unwrap();
        assert_eq!(
            callbacks.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "disconnect resets the key"
        );
        publisher
            .publish(serde_json::to_value(&changed).unwrap())
            .unwrap();
        assert_eq!(
            callbacks.load(std::sync::atomic::Ordering::SeqCst),
            4,
            "reconnect repaints"
        );
        drop(subscription);
    }

    #[test]
    fn predecode_cache_obeys_pair_identity_and_lifetime_fences() {
        use std::io::Write as _;
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-predecode-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("selected.jpg");
        image::RgbImage::from_pixel(2, 1, image::Rgb([255, 255, 255]))
            .save(&path)
            .unwrap();
        let path = path.to_string_lossy().into_owned();
        let mut active = pair(7);
        active.path = path.clone();
        active.file_identity =
            WallpaperFileIdentity::from_metadata(&std::fs::symlink_metadata(&path).unwrap());
        let preview = Preview::default();
        let generation = preview.select("boot-a".into(), path.clone(), "image".into(), None);
        let selection = preview.selected().unwrap();
        let target = PredecodeTarget {
            boot: "boot-a".into(),
            pair: Arc::new(active.clone()),
            width: 2,
            height: 1,
        };
        let job = make_predecode_job(selection.clone(), target.clone()).unwrap();
        assert_eq!(
            confirmed_predecode_key(&selection, &active, 2, 1),
            Some(job.key.clone())
        );
        assert!(confirmed_predecode_key(&selection, &pair(1), 2, 1).is_none());
        let mut changed = selection.clone();
        changed.boot = "boot-b".into();
        assert_ne!(
            confirmed_predecode_key(&changed, &active, 2, 1),
            Some(job.key.clone())
        );
        changed = selection.clone();
        changed.generation += 1;
        assert_ne!(
            confirmed_predecode_key(&changed, &active, 2, 1),
            Some(job.key.clone())
        );
        assert_ne!(
            confirmed_predecode_key(&selection, &active, 4, 1),
            Some(job.key.clone())
        );

        let ready = Predecoder::default();
        ready.submit(selection.clone(), target);
        let (guard, _) = ready
            .signal
            .changed
            .wait_timeout_while(ready.signal.lock(), Duration::from_secs(2), |state| {
                state.cached.is_none() && state.running
            })
            .unwrap();
        drop(guard);
        assert_eq!(
            ready.take_confirmed(&selection, "boot-a", &active, 2, 1),
            Some([255, 255, 255, 255].repeat(2))
        );
        assert!(
            ready.signal.lock().cached.is_none(),
            "confirmed pixels transfer instead of being copied"
        );
        drop(ready);

        let mut state = preview.predecode.signal.lock();
        state.desired = Some(job.key.clone());
        state.pending = Some(job);
        state.cached = Some(CachedPredecode {
            key: confirmed_predecode_key(&selection, &active, 2, 1).unwrap(),
            rgba: vec![0; 8],
            expires: Instant::now() + PREDECODE_LIFETIME,
        });
        drop(state);
        assert_eq!(
            preview.select("boot-a".into(), path.clone(), "image".into(), None),
            generation
        );
        assert_eq!(
            preview.selected().unwrap().requested_at,
            selection.requested_at
        );
        assert!(
            preview.predecode.signal.lock().cached.is_some(),
            "duplicate activation preserves prepared pixels"
        );
        assert!(
            preview.predecode.signal.lock().pending.is_some(),
            "duplicate activation does not cancel the in-flight decode"
        );
        assert!(preview.cancel(generation));
        assert!(preview.predecode.signal.lock().cached.is_none());
        assert!(preview.predecode.signal.lock().pending.is_none());

        let predecoder = Predecoder::default();
        let key = confirmed_predecode_key(&selection, &active, 2, 1).unwrap();
        {
            let mut state = predecoder.signal.lock();
            state.desired = Some(key.clone());
            state.cached = Some(CachedPredecode {
                key,
                rgba: vec![0; 8],
                expires: Instant::now() - Duration::from_millis(1),
            });
        }
        assert!(
            predecoder
                .take_confirmed(&selection, "boot-a", &active, 2, 1)
                .is_none()
        );
        assert!(predecoder.signal.lock().cached.is_none());
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        assert!(confirmed_predecode_key(&selection, &active, 2, 1).is_none());
        drop(preview);
        drop(predecoder);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn video_predecode_requires_advertised_poster_and_both_confirmed_identities() {
        let root = std::env::temp_dir().join(format!(
            "bead-wallpaper-video-predecode-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let video = root.join("movie.mp4");
        let poster = root.join("poster.png");
        std::fs::write(&video, b"video bytes").unwrap();
        image::RgbImage::from_pixel(2, 1, image::Rgb([10, 20, 30]))
            .save(&poster)
            .unwrap();
        let video = video.to_string_lossy().into_owned();
        let poster = poster.to_string_lossy().into_owned();
        let source_identity = local_file_identity(&video).unwrap();
        let poster_identity = local_file_identity(&poster).unwrap();
        let descriptor = nacre_protocol::wallpaper::WallpaperVideoPredecode {
            poster_path: poster.clone(),
            poster_file_identity: poster_identity.clone(),
            source_file_identity: source_identity.clone(),
        };
        let preview = Preview::default();
        let generation = preview.select(
            "boot-video".into(),
            video.clone(),
            "video".into(),
            Some(descriptor.clone()),
        );
        let selection = preview.selected().unwrap();
        let mut active = pair(3);
        active.path = poster.clone();
        active.file_identity = poster_identity.clone();
        active.motion = Some(nacre_protocol::wallpaper::WallpaperMotion {
            kind: nacre_protocol::wallpaper::MotionKind::Video,
            path: video.clone(),
            file_identity: source_identity.clone(),
        });
        let target = PredecodeTarget {
            boot: "boot-video".into(),
            pair: Arc::new(active.clone()),
            width: 2,
            height: 1,
        };
        let job = make_predecode_job(selection.clone(), target.clone()).unwrap();
        assert_eq!(
            job.key.decode_path, poster,
            "decode the advertised engine poster, never the UI thumbnail"
        );
        assert_eq!(
            confirmed_predecode_key(&selection, &active, 2, 1),
            Some(job.key.clone())
        );

        let mut wrong_motion = active.clone();
        wrong_motion.motion.as_mut().unwrap().file_identity.size += 1;
        assert!(confirmed_predecode_key(&selection, &wrong_motion, 2, 1).is_none());
        let mut wrong_poster = active.clone();
        wrong_poster.path.push_str(".other");
        assert!(confirmed_predecode_key(&selection, &wrong_poster, 2, 1).is_none());
        let mut absent_descriptor = selection.clone();
        absent_descriptor.video_predecode = None;
        assert!(
            make_predecode_job(absent_descriptor, target).is_none(),
            "old daemon entries fail closed to normal confirmed decode"
        );
        assert!(
            preview
                .generation_for("boot-video", &video)
                .is_some_and(|current| current == generation)
        );
        drop(preview);
        let _ = std::fs::remove_dir_all(root);
    }

    fn pair(revision: u64) -> ActiveWallpaper {
        serde_json::from_value(serde_json::json!({"revision":revision,"palette_revision":revision,"path":"/wall.png","file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":0,"mtime_nsec":0,"ctime_sec":0,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}})).unwrap()
    }
    fn active(pair: ActiveWallpaper) -> WallpaperActive {
        WallpaperActive {
            available: true,
            stale: false,
            previous_available: false,
            revision: pair.revision,
            current: Some(pair.clone()),
            outputs: BTreeMap::from([("DP-1".into(), pair)]),
            error: None,
        }
    }
}
