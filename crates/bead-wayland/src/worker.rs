// SPDX-License-Identifier: MIT
//! Script/CPU preparation actor; validated candidates cross the draw thread by value.
use crate::{InputRegion, PanelAnchor, SceneSource, UiEvent};
use bead_render::{Rect, Scene};
use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, Weak, mpsc},
    thread::JoinHandle,
};
const STACK_BYTES: usize = 2 * bead_core::cache::MIB;
fn stack_charge(requested: Option<&str>) -> usize {
    requested
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(STACK_BYTES)
        .max(STACK_BYTES)
}
pub type Factory = Box<dyn FnOnce() -> Result<Box<dyn SceneSource>, String> + Send>;
#[derive(Default)]
struct Mirror {
    retained_bytes: Option<usize>,
    dormant_ready: bool,
    wakeup_at: Option<std::time::Instant>,
    drag_visuals: Vec<crate::DragVisual>,
    focused_dismiss: bool,
    tracks_origin: bool,
    outside_dismiss: bool,
    scroll_regions: Vec<Rect>,
    scroll_steps: Vec<crate::StepRegion>,
    regions: Vec<InputRegion>,
    surrounding: Option<(String, i32, i32)>,
    sensitive: bool,
    /// `SceneSource::state_revision` when the three shared copies below were read.
    revision: Option<u64>,
    tree: Option<Arc<accesskit::TreeUpdate>>,
    summary: Arc<str>,
    resources: String,
    saved_state: Arc<serde_json::Value>,
}
impl Mirror {
    fn region_at(&self, x: f32, y: f32) -> Option<&InputRegion> {
        self.regions.iter().rev().find(|r| r.contains(x, y))
    }

    /// Read `source` into a new `last`, sharing `last`'s accessibility tree, summary and
    /// saved state while the source's state revision is unchanged.
    fn read(
        source: &dyn SceneSource,
        cache: &bead_render::tree::TreeCache,
        stack_bytes: usize,
        last: &mut Arc<Self>,
    ) -> Arc<Self> {
        let _profile = bead_core::profile::Span::new("mirror");
        let revision = source.state_revision();
        let same = revision.is_some() && revision == last.revision;
        *last = Arc::new(Self {
            dormant_ready: source.dormant_ready(),
            retained_bytes: source.warm_retained_bytes().and_then(|bytes| {
                bytes
                    .checked_add(stack_bytes)?
                    .checked_add(cache.retained_bytes())?
                    .checked_add(4096)
            }),
            wakeup_at: source.wakeup_at(),
            drag_visuals: source.drag_visuals(),
            focused_dismiss: source.focused_dismiss(),
            tracks_origin: source.tracks_origin(),
            outside_dismiss: source.outside_dismiss(),
            scroll_regions: source.scroll_regions(),
            scroll_steps: source.scroll_step_regions(),
            regions: source.input_regions(),
            surrounding: source.surrounding_text(),
            sensitive: source.sensitive_input(),
            tree: if same {
                last.tree.clone()
            } else {
                source.accessibility_tree().map(Arc::new)
            },
            summary: if same {
                last.summary.clone()
            } else {
                source.state_summary().into()
            },
            resources: source.resource_counts(),
            saved_state: if same {
                last.saved_state.clone()
            } else {
                Arc::new(source.saved_state())
            },
            revision,
        });
        last.clone()
    }
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Ready,
    Accepted,
    Settled,
    Failed,
}
pub struct Packet {
    resume: bool,
    animation_frame: bool,
    phase: Phase,
    token: u64,
    scene: Option<Arc<Scene>>,
    mirror: Option<Arc<Mirror>>,
    error: Option<String>,
    reload: bool,
    /// The staged palette last, after the queued ones it replaced (B0332).
    wallpaper_palette: Option<Vec<std::collections::BTreeMap<String, [u8; 4]>>>,
}
impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerPacket")
            .field("phase", &self.phase)
            .field("token", &self.token)
            .finish()
    }
}
impl Packet {
    pub fn resumed(&self) -> bool {
        self.resume && matches!(self.phase, Phase::Accepted) && self.error.is_none()
    }
    pub fn resume_failed(&self) -> bool {
        self.resume && (matches!(self.phase, Phase::Failed) || self.error.is_some())
    }
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }
    pub fn is_reload(&self) -> bool {
        self.reload
    }
    pub fn wallpaper_palette(&self) -> Option<&std::collections::BTreeMap<String, [u8; 4]>> {
        self.wallpaper_palette.as_ref()?.last()
    }
    /// Every palette this packet settles, in send order: a replaced one settles with its
    /// replacement, just before it, so a fence sees the same acks, later but never earlier.
    pub(crate) fn wallpaper_palettes(&self) -> &[std::collections::BTreeMap<String, [u8; 4]>] {
        self.wallpaper_palette.as_deref().unwrap_or_default()
    }
    pub fn is_accepted(&self) -> bool {
        matches!(self.phase, Phase::Accepted)
    }
    pub fn is_settled(&self) -> bool {
        matches!(self.phase, Phase::Settled)
    }
    pub fn is_failed(&self) -> bool {
        matches!(self.phase, Phase::Failed)
    }
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
#[derive(Debug)]
pub struct ResourceRetirement(std::sync::Mutex<Option<mpsc::Sender<Result<(), String>>>>);
impl ResourceRetirement {
    pub fn finish(&self, result: Result<(), String>) {
        if let Ok(mut response) = self.0.lock()
            && let Some(response) = response.take()
        {
            let _ = response.send(result);
        }
    }
}
enum Job {
    PrepareImages(Vec<Arc<bead_render::tree::ImagePayload>>),
    PrepareCpu(Arc<bead_render::tree::CpuPreparation>),
    ReleasePrepared(Vec<Weak<bead_render::tree::Frame>>),
    RestoreImagePolicy,
    PanelCenter(Option<f32>),
    InitialModel(std::collections::BTreeMap<String, bead_core::Value>),
    PanelAnchor(Option<PanelAnchor>),
    Presented(u64),
    Idle(bool),
    FrameInterval(std::time::Duration),
    Stage(UiEvent, Vec<f32>),
    /// A held key's repeat (B0332): dropped while an earlier one is still queued.
    Repeat(UiEvent, Vec<f32>),
    Resume(Vec<f32>),
    ContentScroll(UiEvent, Vec<f32>),
    Viewport(u32, u32),
    Watch(Box<dyn Fn(Result<(), String>) + Send>),
    Ack(u64, Result<(), String>),
    Stop,
}
/// One window of worker CPU, printed as `WORKER_BUSY` only when it ran hot (B0296).
struct BusyWindow {
    seconds: u64,
    min_pct: f64,
    start: Option<std::time::Instant>,
    cpu_ns: u64,
    jobs: u32,
    animation: u32,
    kinds: std::collections::HashMap<String, (u32, u64)>,
    keys: std::collections::HashMap<String, u32>,
    /// B0332: queued palettes replaced by a newer one, and key repeats dropped.
    palettes: u32,
    repeats: u32,
}
impl BusyWindow {
    fn new(seconds: u64, min_pct: f64) -> Self {
        Self {
            seconds,
            min_pct,
            start: None,
            cpu_ns: 0,
            jobs: 0,
            animation: 0,
            kinds: Default::default(),
            keys: Default::default(),
            palettes: 0,
            repeats: 0,
        }
    }
    /// Add one finished job that began at `at`; the line body when its window closed hot.
    fn record(
        &mut self,
        at: std::time::Instant,
        kind: &str,
        cpu_ns: u64,
        animation: bool,
        keys: &[String],
    ) -> Option<String> {
        let start = *self.start.get_or_insert(at);
        let wall = at.saturating_duration_since(start);
        let line = (wall.as_secs() >= self.seconds).then(|| {
            let pct = self.cpu_ns as f64 / wall.as_nanos() as f64 * 100.;
            let mut top = self.kinds.drain().collect::<Vec<_>>();
            top.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(&b.0)));
            let top = top
                .iter()
                .take(3)
                .map(|(kind, (n, ns))| format!("{kind}:{n}:{}", ns / 1_000_000))
                .collect::<Vec<_>>()
                .join(",");
            let key = self
                .keys
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(key, n)| format!(" top_key={key}:{n}"))
                .unwrap_or_default();
            let absorbed = if self.palettes + self.repeats > 0 {
                format!(
                    " coalesced_palette={} dropped_repeats={}",
                    self.palettes, self.repeats
                )
            } else {
                String::new()
            };
            (pct >= self.min_pct).then(|| {
                format!(
                    "window_s={} cpu_pct={pct:.0} jobs={} top={top} animation_packets={}{key}{}",
                    wall.as_secs(),
                    self.jobs,
                    self.animation,
                    absorbed
                )
            })
        });
        if line.is_some() {
            *self = Self::new(self.seconds, self.min_pct);
            self.start = Some(at);
        }
        let entry = self.kinds.entry(kind.into()).or_default();
        for key in keys {
            *self.keys.entry(key.clone()).or_default() += 1;
        }
        entry.0 += 1;
        entry.1 += cpu_ns;
        self.cpu_ns += cpu_ns;
        self.jobs += 1;
        self.animation += u32::from(animation);
        line.flatten()
    }
}
/// The leading identifier of a `Debug` value: formatting stops at the first other character.
struct Ident(String);
impl std::fmt::Write for Ident {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        for c in text.chars() {
            if !(c.is_alphanumeric() || c == '_') {
                return Err(std::fmt::Error);
            }
            self.0.push(c);
        }
        Ok(())
    }
}
fn event_kind(event: &UiEvent) -> String {
    let mut name = Ident(String::new());
    let _ = std::fmt::write(&mut name, format_args!("{event:?}"));
    match event {
        UiEvent::Output(_, _, inner)
        | UiEvent::Panel(_, inner)
        | UiEvent::Window(_, inner)
        | UiEvent::Overlay(_, inner)
        | UiEvent::Canvas(_, inner) => format!("{}/{}", name.0, event_kind(inner)),
        _ => name.0,
    }
}
/// Store keys a model update names (keys only, never values).
fn model_keys(job: &Job) -> Vec<String> {
    match job {
        Job::Stage(UiEvent::Model(values), _) => values.keys().cloned().collect(),
        _ => Vec::new(),
    }
}
fn job_kind(job: &Job) -> String {
    match job {
        Job::Stage(event, _) | Job::Repeat(event, _) => format!("Stage/{}", event_kind(event)),
        Job::ContentScroll(event, _) => format!("ContentScroll/{}", event_kind(event)),
        Job::Resume(_) => "Resume".into(),
        Job::PrepareImages(_) => "PrepareImages".into(),
        Job::PrepareCpu(_) => "PrepareCpu".into(),
        _ => "Other".into(),
    }
}
// Pointer positions are replaceable only within the same gesture. Every other
// job is an ordering barrier (especially press, release and model updates).
fn enqueue(queue: &mut VecDeque<Job>, mut job: Job) -> bool {
    let counted = matches!(
        &job,
        Job::Stage(..) | Job::ContentScroll(..) | Job::Resume(..) | Job::Repeat(..)
    );
    let mut combined = false;
    // Combine content travel before doing layout, without combining wheel actions
    // on volume/workspace/tray controls or crossing an input/acknowledgement barrier.
    if let (Some(Job::ContentScroll(previous, old_scales)), Job::ContentScroll(next, scales)) =
        (queue.back(), &mut job)
        && old_scales == scales
    {
        let pair = match (previous, next) {
            (
                UiEvent::Scroll {
                    x: ax,
                    y: ay,
                    delta: a,
                },
                UiEvent::Scroll {
                    x: bx,
                    y: by,
                    delta: b,
                },
            )
            | (
                UiEvent::HorizontalScroll {
                    x: ax,
                    y: ay,
                    delta: a,
                },
                UiEvent::HorizontalScroll {
                    x: bx,
                    y: by,
                    delta: b,
                },
            ) if ax == bx && ay == by && a.signum() == b.signum() => Some((*a, b)),
            _ => None,
        };
        if let Some((previous, next)) = pair {
            *next += previous;
            queue.pop_back();
            combined = true;
        }
    }
    let replace = match (queue.back(), &job) {
        (Some(Job::FrameInterval(_)), Job::FrameInterval(_)) => true,
        (Some(Job::Stage(UiEvent::Hover { .. }, _)), Job::Stage(UiEvent::Hover { .. }, _))
        | (Some(Job::Stage(UiEvent::Drag { .. }, _)), Job::Stage(UiEvent::Drag { .. }, _))
        | (
            Some(Job::Stage(UiEvent::LocalDrag { .. }, _)),
            Job::Stage(UiEvent::LocalDrag { .. }, _),
        ) => true,
        (
            Some(Job::Stage(UiEvent::MotionAction { name: previous, .. }, _)),
            Job::Stage(UiEvent::MotionAction { name, .. }, _),
        ) => previous == name,
        _ => false,
    };
    if replace {
        queue.pop_back();
    }
    queue.push_back(job);
    counted && (replace || combined)
}
fn complete_pending(pending: &std::sync::atomic::AtomicUsize) {
    let _ = pending.try_update(
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
        |count| Some(count.saturating_sub(1)),
    );
}
/// What the queue absorbed (B0332): the palettes a newer queued one replaced (they settle with
/// it), and the `WORKER_BUSY` counts of those and of dropped key repeats.
#[derive(Default)]
struct Absorbed {
    palettes: Vec<std::collections::BTreeMap<String, [u8; 4]>>,
    replaced: u32,
    repeats: u32,
}
fn enqueue_pending(
    queue: &mut VecDeque<Job>,
    job: Job,
    pending: &std::sync::atomic::AtomicUsize,
    absorbed: &mut Absorbed,
) {
    match &job {
        // A held key waits behind at most one repeat; presses and other jobs are never dropped.
        Job::Repeat(..) if queue.iter().any(|queued| matches!(queued, Job::Repeat(..))) => {
            absorbed.repeats += 1;
            complete_pending(pending);
            return;
        }
        // A palette is a full overlay on the base palette: only the newest queued one is staged.
        Job::Stage(UiEvent::WallpaperPalette(_), _) => queue.retain_mut(|queued| {
            let Job::Stage(UiEvent::WallpaperPalette(palette), _) = queued else {
                return true;
            };
            absorbed.palettes.push(std::mem::take(palette));
            absorbed.replaced += 1;
            complete_pending(pending);
            false
        }),
        _ => {}
    }
    if enqueue(queue, job) {
        complete_pending(pending);
    }
}

pub struct Source {
    factory: Option<Factory>,
    sender: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    mirror: Arc<Mirror>,
    scales: Vec<f32>,
    image_policy: bead_render::tree::ImagePolicy,
    preparation_retry: bool,
    retired: Arc<std::sync::atomic::AtomicBool>,
    /// Set by the worker thread before it reports a failed factory or a panic (B0345).
    dead: Arc<std::sync::atomic::AtomicBool>,
    token: u64,
    accepted: bool,
    frame_pending: bool,
    pending: Arc<std::sync::atomic::AtomicUsize>,
    poisoned: bool,
    reported: Option<String>,
    asynchronous_resources: bool,
    current: Scene,
    candidate: Option<Scene>,
    viewport: Option<(u32, u32)>,
    frame_interval: Option<std::time::Duration>,
    label: String,
}
impl Source {
    pub fn new(factory: Factory, fallback: Scene) -> Self {
        Self {
            factory: Some(factory),
            sender: None,
            thread: None,
            mirror: Arc::new(Mirror::default()),
            image_policy: bead_render::tree::ImagePolicy::Dense,
            preparation_retry: false,
            retired: Default::default(),
            dead: Default::default(),
            scales: vec![1.],
            token: 0,
            accepted: false,
            frame_pending: false,
            pending: Default::default(),
            poisoned: false,
            reported: None,
            asynchronous_resources: false,
            current: fallback,
            candidate: None,
            viewport: None,
            frame_interval: None,
            label: "worker".into(),
        }
    }
    /// Names this worker in its `WORKER_BUSY` line.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
    pub fn with_image_policy(mut self, policy: bead_render::tree::ImagePolicy) -> Self {
        self.image_policy = policy;
        self
    }
    /// Ready and Accepted packets both carry a staging error, and a broken view fails again
    /// on later events: report each distinct error once until a clean frame is accepted.
    /// The host reports `Failed` errors itself when `stage` returns them.
    fn report(&mut self, packet: &Packet) -> bool {
        let Some(error) = &packet.error else {
            if packet.is_accepted() {
                self.reported = None;
            }
            return false;
        };
        let new = self.reported.as_ref() != Some(error);
        self.reported = Some(error.clone());
        new && !packet.is_failed()
    }
    fn send(&self, job: Job) -> Result<(), Box<dyn Error>> {
        self.sender
            .as_ref()
            .ok_or("script worker not started")?
            .send(job)
            .map_err(|_| "script worker disconnected".into())
    }
    /// Send a job whose packet (or its absorption into the queue) releases a pending credit.
    fn send_counted(&self, job: Job) -> Result<Option<Scene>, Box<dyn Error>> {
        self.pending
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if let Err(error) = self.send(job) {
            complete_pending(&self.pending);
            return Err(error);
        }
        Ok(None)
    }
}
impl Source {
    /// Stop the worker; the returned handle finishes once its last job and session drop are done.
    fn stop(&mut self) -> Option<JoinHandle<()>> {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Job::Stop);
        }
        self.thread.take()
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        // B0205: a worker only sees Stop between jobs, and dropping its session frees the
        // whole view. Joining here blocked the UI thread for up to ~0.5 s live when a panel
        // was replaced mid-stage; a reaper thread joins and returns the freed heap instead.
        if let Some(worker) = self.stop() {
            let _ = std::thread::Builder::new()
                .name("bead-reap".into())
                .spawn(move || {
                    let _ = worker.join();
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    unsafe {
                        libc::malloc_trim(0);
                    }
                });
        }
    }
}
fn bake(
    cache: &mut bead_render::tree::TreeCache,
    scene: &mut Scene,
    scales: &[f32],
) -> Result<(), String> {
    scene.prepared.clear();
    cache.retain_scales(scales);
    for &scale in scales {
        cache.prepare(scene, scale).map_err(|e| e.to_string())?;
        if let Some(frame) = cache.frame.clone() {
            if bead_core::profile::enabled() {
                let image_bytes: usize = frame
                    .items
                    .iter()
                    .filter(|item| matches!(item.primitive, bead_render::Primitive::Image { .. }))
                    .filter_map(|item| item.raster.as_ref())
                    .map(|raster| raster.pixels.data().len())
                    .sum();
                let encoded_bytes: usize = scene.asset_data.values().map(|bytes| bytes.len()).sum();
                let images = frame
                    .items
                    .iter()
                    .filter(|item| matches!(item.primitive, bead_render::Primitive::Image { .. }))
                    // Fully clipped descriptions intentionally need no raster.
                    .filter(|item| {
                        item.clip.is_none_or(|clip| {
                            let visible = bead_render::tree::rect(&item.primitive).intersect(clip);
                            visible.w > 0. && visible.h > 0.
                        })
                    });
                let image_count = images.clone().count();
                let missing_images = images
                    .filter(|item| item.raster.is_none() && item.image.is_none())
                    .count();
                bead_core::vprintln!(
                    "UI_IMAGE_BYTES scale={scale} raster_bytes={image_bytes} encoded_bytes={encoded_bytes} images={image_count} missing={missing_images} thread={:?}",
                    std::thread::current().id()
                );
            }
            scene.prepared.insert(scale.to_bits(), frame);
        }
    }
    Ok(())
}
fn prepare_resource_job(
    job: Job,
    cache: &mut bead_render::tree::TreeCache,
    policy: bead_render::tree::ImagePolicy,
) -> Result<Result<(), String>, Job> {
    match job {
        Job::PrepareImages(images) => {
            let mut result = Ok(());
            for image in images {
                let error = image.prepare().err().map(|error| error.to_string());
                image.finish_request(error.clone());
                if let Some(error) = error {
                    result = Err(error);
                }
            }
            Ok(result)
        }
        Job::PrepareCpu(request) => {
            let result = request.prepare();
            let frame = cache.frame.take();
            // B0332: an empty cache keeps the worker's own image policy. B0336: an adopted CPU
            // cache stays Dense, so the next bakes give the CPU presenter rasters it can paint;
            // `RestoreImagePolicy` brings the worker's policy back once the companion is on GL.
            *cache = bead_render::tree::TreeCache::default().with_image_policy(policy);
            let result = result.map(|prepared| {
                if let Some(mut prepared) = prepared {
                    // ReleasePrepared names the source frame, which can still
                    // be a native recipe while its CPU pixels are already warm.
                    prepared.frame = frame;
                    *cache = prepared;
                }
            });
            Ok(result)
        }
        job => Err(job),
    }
}
/// The next job. B0377: between frames, while none waits, hydrate the frame's zero-opacity images
/// one by one, so they are ready before they show and never hold the frame that hides them.
fn next_job(
    receiver: &mpsc::Receiver<Job>,
    cache: &bead_render::tree::TreeCache,
) -> Result<Job, mpsc::RecvError> {
    while let Some(image) = cache.next_prefetch() {
        match receiver.try_recv() {
            Err(mpsc::TryRecvError::Empty) => {
                if let Err(error) = image.prepare() {
                    image.finish_request(Some(error.to_string()));
                }
            }
            job => return job.map_err(|_| mpsc::RecvError),
        }
    }
    receiver.recv()
}
fn release_cached_prepared(
    cache: &mut bead_render::tree::TreeCache,
    expected: &[Weak<bead_render::tree::Frame>],
) -> bool {
    let Some(frame) = cache.frame.as_ref() else {
        return false;
    };
    let current = Arc::downgrade(frame);
    if !expected.iter().any(|frame| frame.ptr_eq(&current)) {
        return false;
    }
    cache.release_pixel_storage();
    true
}
impl SceneSource for Source {
    fn warm_retained_bytes(&self) -> Option<usize> {
        self.mirror.retained_bytes
    }
    fn dormant_ready(&self) -> bool {
        !self.poisoned
            && !self.asynchronous_resources
            && self.mirror.dormant_ready
            && self.pending.load(std::sync::atomic::Ordering::Acquire) == 0
            && self.candidate.is_none()
            && !self.retired.load(std::sync::atomic::Ordering::Acquire)
    }
    fn resume(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        self.send_counted(Job::Resume(self.scales.clone()))
    }

    fn wakeup_at(&self) -> Option<std::time::Instant> {
        if self.frame_pending {
            None
        } else {
            self.mirror.wakeup_at
        }
    }
    fn resources_retired(&self) -> bool {
        self.retired.load(std::sync::atomic::Ordering::Acquire)
    }
    fn preparation_retry_pending(&self) -> bool {
        self.preparation_retry
    }
    fn prepare_cpu(
        &mut self,
        request: Arc<bead_render::tree::CpuPreparation>,
    ) -> Result<(), Box<dyn Error>> {
        self.preparation_retry |= self.candidate.is_some();
        if request.mark_requested() {
            // Resource jobs can grow the baker after its mirrored byte snapshot.
            // Until those jobs have byte-completion fences, use the cold path.
            self.asynchronous_resources = true;
            self.send(Job::PrepareCpu(request))?;
        }
        Ok(())
    }

    fn release_prepared(&mut self, scene: &Scene) {
        let matching = scene
            .prepared
            .iter()
            .filter_map(|(scale, frame)| {
                self.current
                    .prepared
                    .get(scale)
                    .filter(|current| Arc::ptr_eq(current, frame))
                    .map(|current| (*scale, Arc::downgrade(current)))
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return;
        }
        for (scale, _) in &matching {
            self.current.prepared.remove(scale);
        }
        let _ = self.send(Job::ReleasePrepared(
            matching.into_iter().map(|(_, frame)| frame).collect(),
        ));
    }

    fn prepare_images(
        &mut self,
        images: Vec<Arc<bead_render::tree::ImagePayload>>,
    ) -> Result<(), Box<dyn Error>> {
        self.preparation_retry |= self.candidate.is_some();
        let images = images
            .into_iter()
            .filter(|image| image.pending() && image.mark_requested())
            .collect::<Vec<_>>();
        if images.is_empty() {
            return Ok(());
        }
        self.asynchronous_resources = true;
        if let Err(error) = self.send(Job::PrepareImages(images.clone())) {
            for image in images {
                image.finish_request(None);
            }
            return Err(error);
        }
        Ok(())
    }

    fn deferred_updates(&self) -> bool {
        true
    }
    fn wallpaper_palette_pending(&self) -> bool {
        true
    }
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        let Some(factory) = self.factory.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.sender = Some(sender);
        bead_core::profile::mark("worker_spawn");
        let image_policy = self.image_policy;
        let label = self.label.clone();
        let retired = self.retired.clone();
        let dead = self.dead.clone();
        let pending = self.pending.clone();
        // Leave Rust's stack-size policy intact; only admission accounting reads
        // the request, conservatively charging at least the ordinary default.
        let stack_bytes = stack_charge(std::env::var("RUST_MIN_STACK").ok().as_deref());
        self.thread = Some(std::thread::Builder::new().name("bead-worker".into()).spawn(move || {
            bead_core::profile::mark("worker_started");
            let wakeup: Arc<dyn Fn(UiEvent) + Send + Sync> = Arc::from(wakeup);
            let report = wakeup.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let factory_started = bead_core::profile::monotonic_ns();
                let mut source = match factory() {
                    Ok(source) => source,
                    Err(error) => {
                        dead.store(true, std::sync::atomic::Ordering::Release);
                        wakeup(UiEvent::Worker(Arc::new(Packet {
                            resume: false,
                            animation_frame: false,
                            phase: Phase::Failed,
                            token: 0,
                            scene: None,
                            mirror: None,
                            error: Some(error),
                            reload: false,
                            wallpaper_palette: None,
                        })));
                        return;
                    }
                };
                bead_core::profile::mark("runtime_ready");
                bead_core::profile::trace("worker_session", || {
                    format!(
                        "wall_ms={:.3} thread_cpu_ms={:.3}",
                        bead_core::profile::monotonic_ns().saturating_sub(factory_started) as f64
                            / 1e6,
                        bead_core::profile::thread_cpu_ns() as f64 / 1e6
                    )
                });
                let service_wakeup = wakeup.clone();
                source.services(Box::new(move |event| service_wakeup(event)));
                let mut cache =
                    bead_render::tree::TreeCache::default().with_image_policy(image_policy);
                let mut queue = VecDeque::new();
                let mut token = 0;
                let mut idle = false;
                // B0363: once an open scene was staged, a close must reach the host before stages settle.
                let mut opened = false;
                let mut needs_trim = false;
                let mut accepted_resource_revision = None;
                let mut mirrored = Arc::new(Mirror::default());
                // Stretches (30 s at 25%) and the steady idle tick (10 min at 1%).
                let mut busy = [BusyWindow::new(30, 25.), BusyWindow::new(600, 1.)];
                let mut absorbed = Absorbed::default();
                let mut running: Option<(std::time::Instant, u64, String, bool, Vec<String>)> = None;
                'actor: loop {
                    if let Some((at, cpu, kind, animation, keys)) = running.take() {
                        let cpu = bead_core::profile::thread_cpu_ns().saturating_sub(cpu);
                        for window in &mut busy {
                            window.palettes += absorbed.replaced;
                            window.repeats += absorbed.repeats;
                            if let Some(line) = window.record(at, kind.as_str(), cpu, animation, keys.as_slice()) {
                                eprintln!("WORKER_BUSY session={label} {line}");
                            }
                        }
                        (absorbed.replaced, absorbed.repeats) = (0, 0);
                    }
                    for job in receiver.try_iter() {
                        match job {
                            Job::Stop => break 'actor,
                            job => enqueue_pending(&mut queue, job, &pending, &mut absorbed),
                        }
                    }
                    let job = if let Some(job) = queue.pop_front() {
                        job
                    } else if idle && needs_trim {
                        match receiver.recv_timeout(std::time::Duration::from_millis(250)) {
                            Ok(job) => job,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                cache.trim_idle();
                                #[cfg(all(target_os = "linux", target_env = "gnu"))]
                                unsafe {
                                    libc::malloc_trim(0);
                                }
                                needs_trim = false;
                                continue;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match next_job(&receiver, &cache) {
                            Ok(job) => job,
                            Err(_) => break,
                        }
                    };
                    running = Some((
                        std::time::Instant::now(),
                        bead_core::profile::thread_cpu_ns(),
                        job_kind(&job),
                        matches!(&job, Job::Stage(UiEvent::Model(v), _) if v.is_empty()),
                        model_keys(&job),
                    ));
                    let job = match prepare_resource_job(job, &mut cache, image_policy) {
                        Ok(result) => {
                            wakeup(UiEvent::ResourcesPrepared(result));
                            continue;
                        }
                        Err(job) => job,
                    };
                    let resume = matches!(&job, Job::Resume(_));
                    let job = match job {
                        Job::Resume(scales) => Job::Stage(UiEvent::Relayout, scales),
                        Job::Repeat(event, scales) => Job::Stage(event, scales),
                        job => job,
                    };
                    match job {
                        Job::Resume(_) | Job::Repeat(..) => unreachable!("converted above"),
                        Job::PrepareImages(_) | Job::PrepareCpu(_) => {
                            unreachable!("resource jobs handled above")
                        }
                        Job::ReleasePrepared(frames) => {
                            release_cached_prepared(&mut cache, &frames);
                        }
                        Job::RestoreImagePolicy => cache.set_image_policy(image_policy),
                        Job::PanelCenter(center) => source.panel_center(center),
                        Job::InitialModel(values) => {
                            if let Err(error) = source.initial_model(values) {
                                wakeup(UiEvent::WatchError(error.to_string()));
                            }
                        }
                        Job::PanelAnchor(anchor) => source.panel_anchor(anchor),
                        Job::Stop => break,
                        Job::Presented(revision) => source.presented(revision),
                        Job::Idle(value) => {
                            idle = value;
                            source.set_idle(value);
                            if !idle {
                                continue;
                            }
                            needs_trim = false;
                            cache.trim_idle();
                            #[cfg(all(target_os = "linux", target_env = "gnu"))]
                            unsafe {
                                libc::malloc_trim(0);
                            }
                        }
                        Job::FrameInterval(interval) => source.frame_interval(interval),
                        Job::Viewport(w, h) => {
                            source.viewport(w, h);
                        }
                        Job::Watch(callback) => {
                            if let Err(error) = source.watch(callback) {
                                wakeup(UiEvent::WatchError(error.to_string()));
                            }
                        }
                        Job::Ack(_, _) => {}
                        Job::Stage(event, scales) | Job::ContentScroll(event, scales) => {
                            source.scales(scales.clone());
                            match &event {
                                UiEvent::Hover { x, y } => {
                                    bead_core::profile::position("worker_hover", *x, *y)
                                }
                                UiEvent::Drag { x, y } | UiEvent::LocalDrag { x, y } => {
                                    bead_core::profile::position("worker_drag", *x, *y)
                                }
                                UiEvent::Click { x, y } => {
                                    bead_core::profile::position("worker_click", *x, *y)
                                }
                                _ => {}
                            }
                            bead_core::profile::mark(match &event {
                                UiEvent::Hover { .. } => "worker_hover",
                                UiEvent::Drag { .. } | UiEvent::LocalDrag { .. } => "worker_drag",
                                UiEvent::Click { .. } => "worker_click",
                                _ => "worker_input",
                            });
                            let animation_frame =
                                matches!(&event, UiEvent::Model(values) if values.is_empty());
                            let reload = matches!(event, UiEvent::Reload);
                            // Not empty-Model frames: a closing animation runs on them.
                            let closed_settles = matches!(&event, UiEvent::Model(values) if !values.is_empty())
                                || matches!(&event, UiEvent::WallpaperPalette(_));
                            // Detaching or docking gives the panel another look: once it
                            // is accepted, the rasters only the old look used can go.
                            let look_change = matches!(event, UiEvent::PanelDetached(_));
                            let baked_after = cache.serial();
                            let wallpaper_palette =
                                if let UiEvent::WallpaperPalette(values) = &event {
                                    let mut palettes = std::mem::take(&mut absorbed.palettes);
                                    palettes.push(values.clone());
                                    Some(palettes)
                                } else {
                                    None
                                };
                            let _profile = bead_core::profile::Span::new("worker");
                            let started = std::time::Instant::now();
                            token += 1;
                            let staged = if resume {
                                source.resume().and_then(|scene| {
                                    scene
                                        .map(Some)
                                        .ok_or_else(|| "resume must return a current scene".into())
                                })
                            } else {
                                source.stage(event)
                            };
                            let (mut scene, fault) = match staged {
                                Ok(Some(scene)) => (scene, None),
                                Ok(None) => {
                                    wakeup(UiEvent::Worker(Arc::new(Packet {
                                        resume,
                                        animation_frame,
                                        phase: Phase::Settled,
                                        token,
                                        scene: None,
                                        mirror: Some(Mirror::read(
                                            source.as_ref(),
                                            &cache,
                                            stack_bytes,
                                            &mut mirrored,
                                        )),
                                        error: None,
                                        reload,
                                        wallpaper_palette,
                                    })));
                                    continue;
                                }
                                Err(error) if resume => {
                                    wakeup(UiEvent::Worker(Arc::new(Packet {
                                        resume,
                                        animation_frame: false,
                                        phase: Phase::Failed,
                                        token,
                                        scene: None,
                                        mirror: None,
                                        error: Some(error.to_string()),
                                        reload: false,
                                        wallpaper_palette: None,
                                    })));
                                    continue;
                                }
                                Err(error) => {
                                    let error = error.to_string();
                                    (source.error_scene(error.clone()), Some(error))
                                }
                            };
                            let resource_revision = scene.resource_revision;
                            let look_change = look_change
                                || accepted_resource_revision != Some(resource_revision);
                            bead_core::profile::mark("worker_layout_ready");
                            // B0351: nobody sees a closed panel's pixels, and its next open re-stages
                            // anyway. A data update or palette only advances the source:
                            // no bake, no host commit. Closed means the host already holds a closed
                            // scene (idle), or the panel never opened; the stage that closes an open
                            // panel still bakes, or the host never runs its close.
                            opened |= scene.motion.as_ref().is_some_and(|motion| motion.open);
                            if closed_settles
                                && (idle || !opened)
                                && fault.is_none()
                                && !resume
                                && scene.motion.as_ref().is_some_and(|motion| !motion.open)
                                && source.validate().is_ok()
                            {
                                source.commit();
                                wakeup(UiEvent::Worker(Arc::new(Packet {
                                    resume,
                                    animation_frame,
                                    phase: Phase::Settled,
                                    token,
                                    scene: None,
                                    mirror: Some(Mirror::read(
                                        source.as_ref(),
                                        &cache,
                                        stack_bytes,
                                        &mut mirrored,
                                    )),
                                    error: None,
                                    reload,
                                    wallpaper_palette,
                                })));
                                continue;
                            }
                            needs_trim = true;
                            let bake_profile = bead_core::profile::Span::new("bake");
                            let mut baked = bake(&mut cache, &mut scene, &scales);
                            if baked
                                .as_ref()
                                .err()
                                .is_some_and(|error| error.contains("cache byte budget exceeded"))
                                && image_policy != bead_render::tree::ImagePolicy::Dense
                            {
                                // Retire textures, never the last presented compositor buffer.
                                retired.store(true, std::sync::atomic::Ordering::Release);
                                let (response, released) = mpsc::channel();
                                wakeup(UiEvent::RetireResources(Arc::new(ResourceRetirement(
                                    std::sync::Mutex::new(Some(response)),
                                ))));
                                let release = loop {
                                    match released.recv_timeout(std::time::Duration::from_millis(8))
                                    {
                                        Ok(result) => break result,
                                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                                            break Err("resource retirement disconnected".into());
                                        }
                                        Err(mpsc::RecvTimeoutError::Timeout) => {
                                            for job in receiver.try_iter() {
                                                match job {
                                                    Job::Stop => break 'actor,
                                                    job => enqueue_pending(
                                                        &mut queue,
                                                        job,
                                                        &pending,
                                                        &mut absorbed,
                                                    ),
                                                }
                                            }
                                        }
                                    }
                                };
                                baked =
                                    release.and_then(|()| bake(&mut cache, &mut scene, &scales));
                            }
                            if let Err(error) = baked {
                                wakeup(UiEvent::Worker(Arc::new(Packet {
                                    resume,
                                    animation_frame,
                                    phase: Phase::Failed,
                                    token,
                                    scene: None,
                                    mirror: None,
                                    error: Some(error),
                                    reload,
                                    wallpaper_palette: wallpaper_palette.clone(),
                                })));
                                continue;
                            }
                            drop(bake_profile);
                            bead_core::profile::mark("worker_paint_ready");
                            bead_core::profile::trace("worker_scene", || {
                                format!(
                                    "token={token} reload={reload} wall_ms={:.3} thread_cpu_ms={:.3} builds={}",
                                    started.elapsed().as_secs_f64() * 1000.,
                                    bead_core::profile::thread_cpu_ns() as f64 / 1e6,
                                    cache.builds
                                )
                            });
                            bead_core::vprintln!(
                                "WORKER_PREPARE token={token} reload={reload} total_ms={:.3} builds={}",
                                started.elapsed().as_secs_f64() * 1000.,
                                cache.builds
                            );
                            wakeup(UiEvent::Worker(Arc::new(Packet {
                                resume,
                                animation_frame,
                                phase: Phase::Ready,
                                token,
                                // The UI keeps this as its candidate; Accepted does not resend it.
                                scene: Some(Arc::new(scene)),
                                mirror: None,
                                error: fault.clone(),
                                reload,
                                wallpaper_palette: wallpaper_palette.clone(),
                            })));
                            // B0337: the host may withhold the Ack until a resource job queued
                            // behind this stage is done: run those now, in send order.
                            for job in std::mem::take(&mut queue) {
                                match job {
                                    Job::RestoreImagePolicy => cache.set_image_policy(image_policy),
                                    job => match prepare_resource_job(job, &mut cache, image_policy) {
                                        Ok(result) => wakeup(UiEvent::ResourcesPrepared(result)),
                                        Err(job) => queue.push_back(job),
                                    },
                                }
                            }
                            // No later script event may overwrite this staged transaction before its GPU acknowledgement.
                            loop {
                                match receiver.recv() {
                                    Ok(Job::Stop) | Err(_) => break 'actor,
                                    Ok(job @ (Job::PrepareImages(_) | Job::PrepareCpu(_))) => {
                                        wakeup(UiEvent::ResourcesPrepared(
                                            prepare_resource_job(job, &mut cache, image_policy)
                                                .unwrap_or_else(|_| unreachable!()),
                                        ));
                                    }
                                    // In send order with PrepareCpu, which runs here too.
                                    Ok(Job::RestoreImagePolicy) => {
                                        cache.set_image_policy(image_policy)
                                    }
                                    Ok(Job::Ack(id, result)) if id == token => {
                                        let validated = result.and_then(|()| {
                                            if fault.is_some() {
                                                Ok(())
                                            } else {
                                                source.validate().map_err(|e| e.to_string())
                                            }
                                        });
                                        match validated {
                                            Ok(()) => {
                                                if fault.is_none() {
                                                    source.commit();
                                                    accepted_resource_revision =
                                                        Some(resource_revision);
                                                }
                                                if look_change {
                                                    cache.drop_unused_since(baked_after);
                                                    #[cfg(all(
                                                        target_os = "linux",
                                                        target_env = "gnu"
                                                    ))]
                                                    unsafe {
                                                        libc::malloc_trim(0);
                                                    }
                                                }
                                                wakeup(UiEvent::Worker(Arc::new(Packet {
                                                    resume,
                                                    animation_frame,
                                                    phase: Phase::Accepted,
                                                    token,
                                                    scene: None,
                                                    mirror: Some(Mirror::read(
                                                        source.as_ref(),
                                                        &cache,
                                                        stack_bytes,
                                                        &mut mirrored,
                                                    )),
                                                    error: fault,
                                                    reload,
                                                    wallpaper_palette: wallpaper_palette.clone(),
                                                })));
                                            }
                                            Err(error) => {
                                                let mut overlay = source.error_scene(error.clone());
                                                if bake(&mut cache, &mut overlay, &scales).is_ok() {
                                                    wakeup(UiEvent::Worker(Arc::new(Packet {
                                                        resume,
                                                        animation_frame,
                                                        phase: Phase::Accepted,
                                                        token,
                                                        scene: Some(Arc::new(overlay)),
                                                        mirror: Some(Mirror::read(
                                                            source.as_ref(),
                                                            &cache,
                                                            stack_bytes,
                                                            &mut mirrored,
                                                        )),
                                                        error: Some(error),
                                                        reload,
                                                        wallpaper_palette: wallpaper_palette
                                                            .clone(),
                                                    })));
                                                } else {
                                                    wakeup(UiEvent::WatchError(error));
                                                }
                                            }
                                        }
                                        break;
                                    }
                                    Ok(job) => {
                                        enqueue_pending(&mut queue, job, &pending, &mut absorbed)
                                    }
                                }
                            }
                        }
                    }
                }
            }));
            if result.is_err() {
                dead.store(true, std::sync::atomic::Ordering::Release);
                report(UiEvent::Fatal("script worker panicked".into()));
            }
        }).expect("spawn bead-worker"));
        if let Some(interval) = self.frame_interval {
            let _ = self.send(Job::FrameInterval(interval));
        }
    }
    fn frame_interval(&mut self, interval: std::time::Duration) {
        if self.frame_interval == Some(interval) {
            return;
        }
        self.frame_interval = Some(interval);
        if self.sender.is_some() {
            let _ = self.send(Job::FrameInterval(interval));
        }
    }
    fn set_idle(&mut self, idle: bool) {
        let _ = self.send(Job::Idle(idle));
    }
    fn panel_center(&mut self, center: Option<f32>) {
        let _ = self.send(Job::PanelCenter(center));
    }
    fn restore_image_policy(&mut self) {
        if self.image_policy != bead_render::tree::ImagePolicy::Dense {
            let _ = self.send(Job::RestoreImagePolicy);
        }
    }
    fn initial_model(
        &mut self,
        values: std::collections::BTreeMap<String, bead_core::Value>,
    ) -> Result<(), Box<dyn Error>> {
        self.send(Job::InitialModel(values))
    }
    fn panel_anchor(&mut self, anchor: Option<PanelAnchor>) {
        let _ = self.send(Job::PanelAnchor(anchor));
    }
    fn scales(&mut self, scales: Vec<f32>) {
        self.scales = scales;
    }
    fn viewport(&mut self, w: u32, h: u32) -> bool {
        if self.viewport == Some((w, h)) {
            return false;
        }
        self.viewport = Some((w, h));
        let _ = self.send(Job::Viewport(w, h));
        true
    }
    fn watch(
        &mut self,
        callback: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        // A fast-failing factory may already be gone: its Failed packet reports it (B0345).
        self.send(Job::Watch(callback))
            .or_else(|error| if self.alive() { Err(error) } else { Ok(()) })
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        if let UiEvent::ResourcesPrepared(result) = event {
            result.map_err(|error| -> Box<dyn Error> { error.into() })?;
            self.preparation_retry = false;
            return Ok(self.candidate.clone());
        }
        if let UiEvent::Worker(packet) = event {
            if packet.has_error() {
                self.poisoned = true;
            } else if packet.is_accepted() {
                self.poisoned = false;
            }
            if matches!(
                packet.phase,
                Phase::Accepted | Phase::Settled | Phase::Failed
            ) {
                complete_pending(&self.pending);
            }
            self.token = packet.token;
            if packet.animation_frame
                && matches!(
                    packet.phase,
                    Phase::Accepted | Phase::Settled | Phase::Failed
                )
            {
                self.frame_pending = false;
            }
            if let Some(mirror) = &packet.mirror {
                self.mirror = mirror.clone();
            }
            if self.report(&packet) {
                eprintln!("UI_ERROR {}", packet.error.as_deref().unwrap_or_default());
            }
            match packet.phase {
                Phase::Failed => {
                    self.retired
                        .store(false, std::sync::atomic::Ordering::Release);
                    return Err(packet
                        .error
                        .clone()
                        .unwrap_or_else(|| "worker failed".into())
                        .into());
                }
                Phase::Settled => return Ok(None),
                Phase::Ready => self.accepted = false,
                Phase::Accepted => self.accepted = true,
            }
            let scene = Arc::try_unwrap(packet)
                .map(|packet| packet.scene)
                .unwrap_or_else(|packet| packet.scene.clone())
                .map(Arc::unwrap_or_clone);
            if scene.is_none() && self.accepted {
                // Accepting the Ready packet's candidate: it was delivered once already.
                return Ok(self.candidate.clone());
            }
            self.candidate = scene.clone();
            return Ok(scene);
        }
        if matches!(&event, UiEvent::Model(values) if values.is_empty())
            && self.mirror.wakeup_at.is_some()
        {
            if self.frame_pending {
                return Ok(None);
            }
            self.frame_pending = true;
        }
        let content_scroll = match &event {
            UiEvent::Scroll { x, y, .. } | UiEvent::HorizontalScroll { x, y, .. } => self
                .mirror
                .scroll_regions
                .iter()
                .any(|r| *x >= r.x && *y >= r.y && *x < r.x + r.w && *y < r.y + r.h),
            _ => false,
        };
        self.send_counted(if content_scroll {
            Job::ContentScroll(event, self.scales.clone())
        } else {
            Job::Stage(event, self.scales.clone())
        })
    }
    fn stage_repeat(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        self.send_counted(Job::Repeat(event, self.scales.clone()))
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn presented(&mut self, revision: u64) {
        let _ = self.send(Job::Presented(revision));
    }
    fn commit(&mut self) -> bool {
        if self.accepted {
            self.retired
                .store(false, std::sync::atomic::Ordering::Release);
            self.accepted = false;
            if let Some(scene) = self.candidate.take() {
                self.current = scene;
            }
            true
        } else {
            let _ = self.send(Job::Ack(self.token, Ok(())));
            false
        }
    }
    fn error_scene(&mut self, error: String) -> Scene {
        let _ = self.send(Job::Ack(self.token, Err(error)));
        self.current.clone()
    }
    fn state_summary(&self) -> String {
        self.mirror.summary.to_string()
    }
    fn saved_state(&self) -> serde_json::Value {
        (*self.mirror.saved_state).clone()
    }
    fn resource_counts(&self) -> String {
        self.mirror.resources.clone()
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        self.mirror.tree.as_deref().cloned()
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.mirror.surrounding.clone()
    }
    fn sensitive_input(&self) -> bool {
        self.mirror.sensitive
    }
    fn input_regions(&self) -> Vec<InputRegion> {
        self.mirror.regions.clone()
    }
    fn drag_visuals(&self) -> Vec<crate::DragVisual> {
        self.mirror.drag_visuals.clone()
    }
    fn scroll_regions(&self) -> Vec<Rect> {
        self.mirror.scroll_regions.clone()
    }
    fn scroll_step_regions(&self) -> Vec<crate::StepRegion> {
        self.mirror.scroll_steps.clone()
    }
    fn outside_dismiss(&self) -> bool {
        self.mirror.outside_dismiss
    }
    fn alive(&self) -> bool {
        !self.dead.load(std::sync::atomic::Ordering::Acquire)
    }
    fn dismiss_at(&self, x: f32, y: f32) -> bool {
        self.mirror.region_at(x, y).is_some_and(|r| r.dismiss)
    }
    fn tracks_origin(&self) -> bool {
        self.mirror.tracks_origin
    }
    fn focused_dismiss(&self) -> bool {
        self.mirror.focused_dismiss
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.mirror.region_at(x, y).map_or("default", |r| r.cursor)
    }
    fn allows_edge_hover(&self, x: f32, y: f32) -> bool {
        self.mirror
            .region_at(x, y)
            .is_none_or(|r| r.edge_hover || r.cursor == "default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_window_prints_only_hot_windows_with_top_three_by_cpu() {
        let t0 = std::time::Instant::now();
        let at = |s| t0 + Duration::from_secs(s);
        let ms = 1_000_000;
        let mut busy = BusyWindow::new(30, 25.);
        for (s, kind, cpu) in [
            (0, "A", 2),
            (1, "B", 3000),
            (2, "C", 2000),
            (3, "D", 1000),
            (4, "A", 4000),
        ] {
            assert!(busy.record(at(s), kind, cpu * ms, s == 4, &[]).is_none());
        }
        let line = busy
            .record(
                at(30),
                "E",
                0,
                false,
                &["bead.desktop.clock.seconds".into()],
            )
            .expect("hot window");
        assert_eq!(
            line,
            "window_s=30 cpu_pct=33 jobs=5 top=A:2:4002,B:1:3000,C:1:2000 animation_packets=1"
        );
        // The next window starts empty and stays under the threshold: no line.
        assert!(busy.record(at(31), "A", 100 * ms, false, &[]).is_none());
        assert!(busy.record(at(61), "A", 0, false, &[]).is_none());
    }
    #[test]
    fn idle_tick_window_prints_a_steady_one_percent_with_the_top_key() {
        let t0 = std::time::Instant::now();
        let mut busy = BusyWindow::new(600, 1.);
        let key = ["bead.desktop.clock.seconds".to_string()];
        // A 1 Hz store update costing 20 ms: 2% of a core, silent under the 30 s tier.
        for s in 0..600 {
            let at = t0 + Duration::from_secs(s);
            assert!(
                busy.record(at, "Stage/Model", 20_000_000, false, &key)
                    .is_none()
            );
        }
        let line = busy
            .record(t0 + Duration::from_secs(600), "Stage/Model", 0, false, &key)
            .expect("steady tick");
        assert_eq!(
            line,
            "window_s=600 cpu_pct=2 jobs=600 top=Stage/Model:600:12000 animation_packets=0 top_key=bead.desktop.clock.seconds:600"
        );
        assert!(
            BusyWindow::new(30, 25.)
                .record(t0, "A", 1, false, &[])
                .is_none()
        );
    }
    #[test]
    fn event_kind_stops_at_the_first_non_identifier_and_names_nested_events() {
        let inner = UiEvent::LivePalette(BTreeMap::from([("k".into(), [0; 4])]));
        assert_eq!(event_kind(&inner), "LivePalette");
        let nested = UiEvent::Output(
            1,
            "DP-1".into(),
            Box::new(UiEvent::Panel(2, Box::new(inner))),
        );
        assert_eq!(event_kind(&nested), "Output/Panel/LivePalette");
        assert_eq!(event_kind(&UiEvent::Reload), "Reload");
    }
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    fn scene() -> Scene {
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: BTreeMap::new(),
            clips: BTreeMap::new(),
            layers: vec![],
            prepared: BTreeMap::new(),
            font_files: vec![],
            width: 16.,
            slide: None,
            primitives: vec![bead_render::Primitive::Rounded {
                rect: bead_render::Rect::new(0., 0., 16., 16.),
                radius: 0.,
                fill: [20, 30, 40, 255],
                border: None,
                radial: None,
            }],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 16., 16.),
            input_islands: None,
            key_repeat: None,
            palette_at_rest: false,
        }
    }
    fn prepared_frame(scene: &mut Scene) -> Arc<bead_render::tree::Frame> {
        let mut cache = bead_render::tree::TreeCache::default();
        cache.prepare(scene, 1.).unwrap();
        cache.frame.take().unwrap()
    }

    #[test]
    fn prepared_release_clears_only_matching_current_scene_and_uses_weak_job() {
        let mut current = scene();
        let current_frame = prepared_frame(&mut current);
        let current_weak = Arc::downgrade(&current_frame);
        current
            .prepared
            .insert(1_f32.to_bits(), current_frame.clone());
        drop(current_frame);

        let mut candidate = scene();
        candidate.resource_revision = 1;
        let candidate_frame = prepared_frame(&mut candidate);
        let candidate_weak = Arc::downgrade(&candidate_frame);
        candidate
            .prepared
            .insert(1_f32.to_bits(), candidate_frame.clone());
        drop(candidate_frame);

        let mut source = Source::new(Box::new(|| Err("unused".into())), current);
        source.candidate = Some(candidate);
        source.mirror = Arc::new(Mirror {
            summary: "unchanged model".into(),
            ..Default::default()
        });
        let (sender, receiver) = mpsc::channel();
        source.sender = Some(sender);
        let expected = source.current.clone();
        source.release_prepared(&expected);
        drop(expected);

        assert!(source.current.prepared.is_empty());
        assert_eq!(source.current.resource_revision, 0);
        assert!(!source.accepted, "release never commits a candidate");
        assert_eq!(source.state_summary(), "unchanged model");
        assert!(
            source
                .candidate
                .as_ref()
                .unwrap()
                .prepared
                .contains_key(&1_f32.to_bits())
        );
        assert!(current_weak.upgrade().is_none());
        assert!(candidate_weak.upgrade().is_some());
        let Ok(Job::ReleasePrepared(frames)) = receiver.try_recv() else {
            panic!("release must enqueue an actor barrier");
        };
        assert_eq!(frames.len(), 1);
        assert!(frames[0].ptr_eq(&current_weak));
    }

    #[test]
    fn stale_prepared_release_does_not_drop_a_newer_actor_frame() {
        let old_scene = scene();
        let mut cache = bead_render::tree::TreeCache::default();
        cache.prepare(&old_scene, 1.).unwrap();
        let old_frame = cache.frame.as_ref().unwrap().clone();
        let old = Arc::downgrade(&old_frame);

        let mut newer_scene = scene();
        newer_scene.primitives[0] = bead_render::Primitive::Rounded {
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            radius: 0.,
            fill: [80, 30, 40, 255],
            border: None,
            radial: None,
        };
        cache.prepare(&newer_scene, 1.).unwrap();
        let newer = cache.frame.as_ref().unwrap().clone();
        assert!(!Arc::ptr_eq(cache.frame.as_ref().unwrap(), &old_frame));
        drop(old_scene);
        drop(old_frame);

        assert!(!release_cached_prepared(&mut cache, &[old]));
        assert!(Arc::ptr_eq(cache.frame.as_ref().unwrap(), &newer));
        let current = Arc::downgrade(&newer);
        drop(newer);
        assert!(release_cached_prepared(
            &mut cache,
            std::slice::from_ref(&current)
        ));
        assert!(cache.frame.is_none());
        assert!(current.upgrade().is_none());
    }

    #[test]
    fn cpu_preparation_keeps_dense_pixels_warm_and_source_release_identity() {
        use bead_render::Renderer;
        for changed_before_close in [false, true] {
            let mut fixture = scene();
            fixture.primitives[0] = bead_render::Primitive::Image {
                opacity: 1.,
                rect: bead_render::Rect::new(0., 0., 16., 16.),
                path: "proof.svg".into(),
                blur: 0.,
                fit: bead_render::ImageFit::Fill,
                clip_polygon: None,
                bake_size: None,
                radius: 0.,
                tint: None,
            };
            fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
            // A raster item under every policy: the warm CPU pixels the close keeps and releases.
            fixture.primitives.push(bead_render::Primitive::Shadow {
                rect: bead_render::Rect::new(2., 2., 12., 12.),
                radius: 4.,
                blur: 2.,
                spread: 0.,
                dy: 0.,
                alpha: 1.,
            });
            let mut cache = bead_render::tree::TreeCache::default()
                .with_image_policy(bead_render::tree::ImagePolicy::UploadOnly);
            bake(&mut cache, &mut fixture, &[1.]).unwrap();
            let source_frame = fixture.prepared[&1_f32.to_bits()].clone();
            assert!(source_frame.items[0].image.is_some());
            let mut renderer = bead_render::tree::TreeCpuRenderer::default();
            let error = renderer.prepare_on_ui(&fixture, 1.).unwrap_err();
            let request = error
                .downcast_ref::<bead_render::tree::CpuResourcesPending>()
                .unwrap()
                .request
                .clone();
            assert!(matches!(
                prepare_resource_job(
                    Job::PrepareCpu(request),
                    &mut cache,
                    bead_render::tree::ImagePolicy::UploadOnly
                ),
                Ok(Ok(()))
            ));
            assert!(Arc::ptr_eq(cache.frame.as_ref().unwrap(), &source_frame));
            renderer.prepare_on_ui(&fixture, 1.).unwrap();
            drop(renderer);
            let builds = cache.builds;
            assert!(builds > 0);
            fixture.prepared.clear();
            if changed_before_close {
                fixture.resource_revision += 1;
                cache.prepare(&fixture, 1.).unwrap();
                assert_eq!(
                    cache.builds, builds,
                    "the first scene update reuses baked CPU pixels"
                );
                // B0336: the adopted cache is Dense for the CPU presenter.
                assert!(cache.frame.as_ref().unwrap().items[0].raster.is_some());
                assert!(cache.frame.as_ref().unwrap().items[0].image.is_none());
            }
            let current = Arc::downgrade(cache.frame.as_ref().unwrap());
            assert!(release_cached_prepared(&mut cache, &[current]));
            assert!(cache.frame.is_none());
            cache.prepare(&fixture, 1.).unwrap();
            assert!(
                cache.builds > builds,
                "close releases the warm pixels even without an intervening scene update"
            );
        }
    }
    #[test]
    fn cpu_preparation_keeps_one_upload_payload_for_a_bake_size_image() {
        use bead_render::Renderer;
        let sized = |w: f32| {
            let mut fixture = scene();
            fixture.primitives[0] = bead_render::Primitive::Image {
                opacity: 1.,
                rect: bead_render::Rect::new(0., 0., w, w),
                path: "proof.svg".into(),
                blur: 0.,
                fit: bead_render::ImageFit::Fill,
                clip_polygon: None,
                bake_size: Some((16., 16.)),
                radius: 0.,
                tint: None,
            };
            fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
            fixture
        };
        let policy = bead_render::tree::ImagePolicy::UploadOnly;
        let mut cache = bead_render::tree::TreeCache::default().with_image_policy(policy);
        let mut first = sized(8.);
        bake(&mut cache, &mut first, &[1.]).unwrap();
        let error = bead_render::tree::TreeCpuRenderer::default()
            .prepare_on_ui(&first, 1.)
            .unwrap_err();
        let request = error
            .downcast_ref::<bead_render::tree::CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        assert!(matches!(
            prepare_resource_job(Job::PrepareCpu(request), &mut cache, policy),
            Ok(Ok(()))
        ));
        // B0336: the adopted cache is Dense while the companion paints on CPU.
        cache.prepare(&sized(20.), 1.).unwrap();
        let item = &cache.frame.as_ref().unwrap().items[0];
        assert!(
            item.raster.is_some() && item.image.is_none(),
            "dense for the CPU presenter"
        );
        // The companion is on GL: what Job::RestoreImagePolicy runs.
        cache.set_image_policy(policy);
        let builds = cache.builds;
        let payloads: Vec<_> = (9..13)
            .map(|w| {
                cache.prepare(&sized(w as f32), 1.).unwrap();
                let item = &cache.frame.as_ref().unwrap().items[0];
                assert!(item.raster.is_none(), "a GL worker's image stays a payload");
                item.image.clone().unwrap()
            })
            .collect();
        assert!(
            payloads.iter().all(|p| Arc::ptr_eq(p, &payloads[0])),
            "one build"
        );
        assert_eq!((payloads[0].width, payloads[0].height), (16, 16));
        assert_eq!(cache.builds, builds, "no image raster");
    }
    #[test]
    fn a_cpu_companion_paints_the_next_bake_after_adoption_without_another_preparation() {
        use bead_render::Renderer;
        let image = |w: f32| {
            let mut fixture = scene();
            fixture.primitives[0] = bead_render::Primitive::Image {
                opacity: 1.,
                rect: bead_render::Rect::new(0., 0., w, w),
                path: "proof.svg".into(),
                blur: 0.,
                fit: bead_render::ImageFit::Fill,
                clip_polygon: None,
                bake_size: None,
                radius: 0.,
                tint: None,
            };
            fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
            fixture
        };
        // A femtovg-gl panel's worker opens on a CPU companion (lazy GL).
        let policy = bead_render::tree::ImagePolicy::UploadOnly;
        let mut cache = bead_render::tree::TreeCache::default().with_image_policy(policy);
        let mut first = image(8.);
        bake(&mut cache, &mut first, &[1.]).unwrap();
        let mut presenter = bead_render::tree::TreeCpuRenderer::default();
        let request = presenter
            .prepare_on_ui(&first, 1.)
            .unwrap_err()
            .downcast_ref::<bead_render::tree::CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        assert!(matches!(
            prepare_resource_job(Job::PrepareCpu(request), &mut cache, policy),
            Ok(Ok(()))
        ));
        // The next scene's bake is paintable as is: no second PrepareCpu.
        let mut next = image(12.);
        bake(&mut cache, &mut next, &[1.]).unwrap();
        presenter.prepare_on_ui(&next, 1.).unwrap();
    }
    #[test]
    fn only_a_gl_policy_worker_is_told_to_restore_its_image_policy() {
        for (policy, sent) in [
            (bead_render::tree::ImagePolicy::UploadOnly, true),
            (bead_render::tree::ImagePolicy::Dense, false),
        ] {
            let mut source =
                Source::new(Box::new(|| Err("unused".into())), scene()).with_image_policy(policy);
            let (sender, receiver) = mpsc::channel();
            source.sender = Some(sender);
            source.restore_image_policy();
            assert_eq!(
                matches!(receiver.try_recv(), Ok(Job::RestoreImagePolicy)),
                sent
            );
        }
    }

    #[test]
    fn prepared_release_is_deferred_to_the_ordered_actor_loop() {
        let mut cache = bead_render::tree::TreeCache::default();
        let job = Job::ReleasePrepared(Vec::new());
        assert!(matches!(
            prepare_resource_job(job, &mut cache, Default::default()),
            Err(Job::ReleasePrepared(_))
        ));
    }

    #[test]
    fn compositor_frame_coalesces_until_its_own_transaction_settles() {
        let mut source = Source::new(Box::new(|| Err("unused".into())), scene());
        let deadline = std::time::Instant::now();
        source.mirror = Arc::new(Mirror {
            wakeup_at: Some(deadline),
            ..Default::default()
        });
        let (sender, receiver) = mpsc::channel();
        source.sender = Some(sender);
        assert_eq!(source.wakeup_at(), Some(deadline));
        source.stage(UiEvent::Model(Default::default())).unwrap();
        assert!(source.wakeup_at().is_none());
        source.stage(UiEvent::Model(Default::default())).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(Job::Stage(UiEvent::Model(_), _))
        ));
        assert!(receiver.try_recv().is_err());
        let packet = |animation_frame, phase, mirror| {
            UiEvent::Worker(Arc::new(Packet {
                resume: false,
                animation_frame,
                phase,
                token: 1,
                scene: None,
                mirror,
                error: None,
                reload: false,
                wallpaper_palette: None,
            }))
        };
        source.stage(packet(false, Phase::Accepted, None)).unwrap();
        assert!(
            source.wakeup_at().is_none(),
            "an earlier input cannot release frame credit"
        );
        source.stage(packet(true, Phase::Ready, None)).unwrap();
        assert!(
            source.wakeup_at().is_none(),
            "GPU preparation is still in flight"
        );
        source
            .stage(packet(
                true,
                Phase::Settled,
                Some(Arc::new(Mirror::default())),
            ))
            .unwrap();
        assert!(!source.frame_pending);
        assert!(
            source.wakeup_at().is_none(),
            "settled idle view has no more callback work"
        );
    }

    #[test]
    fn unchanged_frame_interval_sends_no_worker_job_and_queued_changes_coalesce() {
        let mut source = Source::new(Box::new(|| Err("unused".into())), scene());
        let (sender, receiver) = mpsc::channel();
        source.sender = Some(sender);
        let slow = Duration::from_nanos(1_000_000_000_000 / 60_000);
        let fast = Duration::from_nanos(1_000_000_000_000 / 120_000);
        source.frame_interval(slow);
        source.frame_interval(slow);
        assert!(
            matches!(receiver.try_recv(), Ok(Job::FrameInterval(interval)) if interval == slow)
        );
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        let mut queue = VecDeque::new();
        enqueue(&mut queue, Job::FrameInterval(slow));
        enqueue(&mut queue, Job::FrameInterval(fast));
        assert_eq!(queue.len(), 1);
        assert!(
            matches!(queue.pop_front(), Some(Job::FrameInterval(interval)) if interval == fast)
        );
        source.frame_interval(fast);
        assert!(
            matches!(receiver.try_recv(), Ok(Job::FrameInterval(interval)) if interval == fast)
        );
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    #[test]
    fn worker_dismissal_and_cursor_use_the_topmost_clipped_polygon_hit() {
        let background = InputRegion {
            include_in_bounds: true,
            id: "background".into(),
            rect: bead_render::Rect::new(0., 0., 100., 100.),
            clip: None,
            hit_polygon: None,
            cursor: "default",
            edge_hover: false,
            dismiss: true,
            prepare_panel: false,
            prepare_view: None,
            prepare_center: None,
        };
        let mut card = background.clone();
        card.id = "card".into();
        card.rect = bead_render::Rect::new(10., 10., 80., 80.);
        card.hit_polygon = Some(vec![
            [0.25, 0.],
            [0.75, 0.],
            [1., 0.5],
            [0.75, 1.],
            [0.25, 1.],
            [0., 0.5],
        ]);
        card.dismiss = false;
        card.cursor = "pointer";
        let mut source = Source::new(Box::new(|| Err("unused".into())), scene());
        source.mirror = Arc::new(Mirror {
            regions: vec![background, card],
            ..Default::default()
        });
        assert!(
            !source.dismiss_at(50., 50.),
            "a card press must not activate its background"
        );
        assert!(
            source.dismiss_at(11., 11.),
            "a hexagon corner belongs to the background"
        );
        assert_eq!(source.pointer_cursor(50., 50.), "pointer");
        assert_eq!(source.pointer_cursor(11., 11.), "default");
        Arc::get_mut(&mut source.mirror).unwrap().regions[1].clip =
            Some(bead_render::Rect::new(20., 20., 60., 60.));
        assert!(
            source.dismiss_at(15., 50.),
            "clipped-out card pixels belong to the background"
        );
        assert!(!source.dismiss_at(50., 50.));
    }
    struct Mock {
        commits: Arc<AtomicUsize>,
        panic: bool,
        scales: Vec<f32>,
        expected_scales: Option<Vec<f32>>,
    }
    impl SceneSource for Mock {
        fn scales(&mut self, scales: Vec<f32>) {
            self.scales = scales;
        }
        fn input_regions(&self) -> Vec<InputRegion> {
            vec![InputRegion {
                include_in_bounds: true,
                hit_polygon: None,
                id: "button".into(),
                rect: bead_render::Rect::new(0., 0., 16., 16.),
                clip: None,
                cursor: "pointer",
                edge_hover: true,
                dismiss: false,
                prepare_panel: false,
                prepare_view: None,
                prepare_center: None,
            }]
        }
        fn watch(
            &mut self,
            _: Box<dyn Fn(Result<(), String>) + Send>,
        ) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn stage(&mut self, _: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
            if self.panic {
                panic!("worker proof panic");
            }
            if let Some(expected) = &self.expected_scales {
                assert_eq!(&self.scales, expected, "scale hint must precede projection");
            }
            Ok(Some(scene()))
        }
        fn validate(&self) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn state_summary(&self) -> String {
            "accepted".into()
        }
        fn saved_state(&self) -> serde_json::Value {
            serde_json::json!({"page":"details"})
        }
        fn commit(&mut self) -> bool {
            self.commits.fetch_add(1, Ordering::SeqCst);
            true
        }
        fn error_scene(&mut self, _: String) -> Scene {
            scene()
        }
    }
    #[test]
    fn settled_packets_share_state_copies_until_the_source_revision_changes() {
        struct Settled {
            revision: Option<Arc<std::sync::atomic::AtomicU64>>,
            reads: Arc<AtomicUsize>,
        }
        impl SceneSource for Settled {
            fn watch(
                &mut self,
                _: Box<dyn Fn(Result<(), String>) + Send>,
            ) -> Result<(), Box<dyn Error>> {
                Ok(())
            }
            fn stage(&mut self, _: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
                Ok(None)
            }
            fn validate(&self) -> Result<(), Box<dyn Error>> {
                Ok(())
            }
            fn state_revision(&self) -> Option<u64> {
                self.revision.as_ref().map(|r| r.load(Ordering::SeqCst))
            }
            fn state_summary(&self) -> String {
                let read = self.reads.fetch_add(1, Ordering::SeqCst);
                format!("read {read}")
            }
            fn saved_state(&self) -> serde_json::Value {
                serde_json::json!({"page": "details"})
            }
            fn commit(&mut self) -> bool {
                true
            }
            fn error_scene(&mut self, _: String) -> Scene {
                scene()
            }
        }
        for revisioned in [true, false] {
            let revision = Arc::new(std::sync::atomic::AtomicU64::new(7));
            let reads = Arc::new(AtomicUsize::new(0));
            let source_revision = revisioned.then(|| revision.clone());
            let source_reads = reads.clone();
            let mut source = Source::new(
                Box::new(move || {
                    Ok(Box::new(Settled {
                        revision: source_revision,
                        reads: source_reads,
                    }))
                }),
                scene(),
            );
            let (sender, receiver) = mpsc::channel();
            source.services(Box::new(move |event| {
                let _ = sender.send(event);
            }));
            let settle = |source: &mut Source| {
                source.stage(UiEvent::Hover { x: 1., y: 1. }).unwrap();
                let event = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
                let UiEvent::Worker(packet) = &event else {
                    panic!("worker packet expected")
                };
                assert!(packet.is_settled());
                let mirror = packet.mirror.clone().unwrap();
                assert!(source.stage(event).unwrap().is_none());
                assert_eq!(source.state_summary(), *mirror.summary);
                assert_eq!(source.saved_state(), serde_json::json!({"page": "details"}));
                mirror
            };
            let first = settle(&mut source);
            let second = settle(&mut source);
            assert_eq!(&*first.summary, "read 0");
            if revisioned {
                assert_eq!(&*second.summary, "read 0");
                assert_eq!(
                    reads.load(Ordering::SeqCst),
                    1,
                    "no-change packet reuses its copies"
                );
                assert!(Arc::ptr_eq(&first.summary, &second.summary));
                assert!(Arc::ptr_eq(&first.saved_state, &second.saved_state));
                revision.store(8, Ordering::SeqCst);
                let third = settle(&mut source);
                assert_eq!(&*third.summary, "read 1", "a new revision reads again");
            } else {
                assert_eq!(
                    &*second.summary, "read 1",
                    "an unknown revision always reads"
                );
            }
        }
    }
    #[test]
    fn accepted_packets_accept_the_ready_candidate_without_resending_it() {
        let commits = Arc::new(AtomicUsize::new(0));
        let count = commits.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Mock {
                    commits: count,
                    panic: false,
                    scales: vec![],
                    expected_scales: None,
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.stage(UiEvent::Reload).unwrap();
        let ready = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let staged = source.stage(ready).unwrap().unwrap();
        assert!(!source.commit(), "Ready only acknowledges");
        let accepted = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let UiEvent::Worker(packet) = &accepted else {
            panic!("worker packet expected")
        };
        assert!(packet.is_accepted() && packet.scene.is_none());
        let shared = source
            .stage(accepted)
            .unwrap()
            .expect("the candidate is accepted");
        let json = |scene: &Scene| serde_json::to_value(&scene.primitives).unwrap();
        assert_eq!(json(&shared), json(&staged));
        let frame = |scene: &Scene| scene.prepared[&1_f32.to_bits()].clone();
        assert!(Arc::ptr_eq(&frame(&shared), &frame(&staged)));
        assert!(source.commit());
        assert_eq!(json(&source.current), json(&staged));
        assert!(Arc::ptr_eq(&frame(&source.current), &frame(&staged)));
        assert_eq!(commits.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn content_scroll_queue_preserves_travel_without_replaying_every_sample() {
        let mut queue = VecDeque::new();
        for delta in [8., 8., -3.] {
            enqueue(
                &mut queue,
                Job::ContentScroll(
                    UiEvent::Scroll {
                        x: 10.,
                        y: 20.,
                        delta,
                    },
                    vec![1.],
                ),
            );
        }
        assert_eq!(
            queue.len(),
            2,
            "a reversal preserves clamping at the content edge"
        );
        assert!(matches!(
            queue.pop_front(),
            Some(Job::ContentScroll(UiEvent::Scroll { delta: 16., .. }, _))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::ContentScroll(UiEvent::Scroll { delta: -3., .. }, _))
        ));
        for _ in 0..2 {
            enqueue(
                &mut queue,
                Job::Stage(
                    UiEvent::Scroll {
                        x: 10.,
                        y: 20.,
                        delta: 1.,
                    },
                    vec![1.],
                ),
            );
        }
        assert_eq!(
            queue.len(),
            2,
            "action scrolls must retain every physical event"
        );
        enqueue(&mut queue, Job::Stage(UiEvent::Release, vec![1.]));
        enqueue(
            &mut queue,
            Job::ContentScroll(
                UiEvent::Scroll {
                    x: 10.,
                    y: 20.,
                    delta: -1.,
                },
                vec![1.],
            ),
        );
        assert_eq!(queue.len(), 4, "release is an ordering barrier");
    }

    #[test]
    fn pointer_queue_keeps_latest_position_and_gesture_barriers() {
        let mut queue = VecDeque::new();
        for x in 0..100 {
            enqueue(
                &mut queue,
                Job::Stage(UiEvent::Hover { x: x as f32, y: 1. }, vec![1.5]),
            );
        }
        enqueue(
            &mut queue,
            Job::Stage(UiEvent::Click { x: 99., y: 1. }, vec![1.5]),
        );
        for x in 100..200 {
            enqueue(
                &mut queue,
                Job::Stage(UiEvent::Drag { x: x as f32, y: 1. }, vec![1.5]),
            );
        }
        enqueue(&mut queue, Job::Stage(UiEvent::Release, vec![1.5]));
        enqueue(
            &mut queue,
            Job::Stage(UiEvent::Hover { x: 200., y: 1. }, vec![1.5]),
        );
        assert_eq!(queue.len(), 5);
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Hover { x: 99., .. }, _))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Click { .. }, _))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Drag { x: 199., .. }, _))
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Release, _))
        ));
    }
    #[test]
    fn budget_retirement_retries_once_and_shutdown_never_waits_for_ui_ack() {
        struct Large(Scene);
        impl SceneSource for Large {
            fn watch(
                &mut self,
                _: Box<dyn Fn(Result<(), String>) + Send>,
            ) -> Result<(), Box<dyn Error>> {
                Ok(())
            }
            fn stage(&mut self, _: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
                Ok(Some(self.0.clone()))
            }
            fn validate(&self) -> Result<(), Box<dyn Error>> {
                Ok(())
            }
            fn state_summary(&self) -> String {
                String::new()
            }
            fn commit(&mut self) -> bool {
                panic!("failed resources must never commit")
            }
            fn error_scene(&mut self, _: String) -> Scene {
                scene()
            }
        }
        let mut fixture = scene();
        fixture.asset_data.insert(
            "too-large-unused".into(),
            Arc::from(vec![0u8; 40 * bead_core::cache::MIB]),
        );
        for acknowledge in [true, false] {
            let fixture = fixture.clone();
            let mut source = Source::new(Box::new(move || Ok(Box::new(Large(fixture)))), scene())
                .with_image_policy(bead_render::tree::ImagePolicy::UploadOnly);
            let (sender, receiver) = mpsc::channel();
            source.services(Box::new(move |event| {
                let _ = sender.send(event);
            }));
            source.stage(UiEvent::Reload).unwrap();
            let event = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            let UiEvent::RetireResources(request) = event else {
                panic!("retirement request expected")
            };
            assert!(source.resources_retired());
            if acknowledge {
                request.finish(Ok(()));
                let failed = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(matches!(&failed, UiEvent::Worker(packet) if packet.is_failed()));
                assert!(source.stage(failed).is_err());
                assert!(source.alive(), "a failed bake keeps its worker");
                assert!(!source.resources_retired());
                assert!(
                    receiver.try_recv().is_err(),
                    "one retirement/retry, no busy loop"
                );
            }
            let (done, completed) = mpsc::channel();
            let dropper = std::thread::spawn(move || {
                if let Some(worker) = source.stop() {
                    let _ = worker.join();
                }
                let _ = done.send(());
            });
            completed
                .recv_timeout(Duration::from_secs(2))
                .expect("shutdown must service Stop while retirement Ack is missing");
            dropper.join().unwrap();
        }
    }
    #[test]
    fn cpu_resource_job_completes_while_worker_waits_for_gpu_ack() {
        let mut fixture = scene();
        fixture.primitives[0] = bead_render::Primitive::Image {
            opacity: 1.,
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
            bake_size: None,
            radius: 0.,
            tint: None,
        };
        fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
        let mut renderer = bead_render::renderer(bead_render::RendererKind::Cpu).unwrap();
        let error = renderer.prepare_on_ui(&fixture, 1.).unwrap_err();
        let request = error
            .downcast_ref::<bead_render::tree::CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        let commits = Arc::new(AtomicUsize::new(0));
        let count = commits.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Mock {
                    commits: count,
                    panic: false,
                    scales: vec![],
                    expected_scales: None,
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send((event, std::thread::current().id()));
        }));
        source.stage(UiEvent::Reload).unwrap();
        let (ready, _) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let staged = source.stage(ready).unwrap().unwrap();
        source.prepare_cpu(request.clone()).unwrap();
        assert!(source.preparation_retry_pending());
        let (event, worker) = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("resource job must not wait for Ack");
        assert_ne!(worker, std::thread::current().id());
        assert!(matches!(event, UiEvent::ResourcesPrepared(Ok(()))));
        let retry = source.stage(event).unwrap().unwrap();
        assert_eq!(retry.resource_revision, staged.resource_revision);
        assert!(!source.preparation_retry_pending());
        renderer.prepare_on_ui(&fixture, 1.).unwrap();
        assert!(!source.commit());
        let (accepted, _) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        source.stage(accepted).unwrap();
        assert!(source.commit());
        assert_eq!(
            commits.load(Ordering::SeqCst),
            1,
            "hydration never duplicates source commit"
        );
    }
    #[test]
    fn image_hydration_is_deduplicated_and_runs_on_the_source_worker() {
        let mut fixture = scene();
        fixture.primitives[0] = bead_render::Primitive::Image {
            opacity: 1.,
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
            bake_size: None,
            radius: 0.,
            tint: None,
        };
        fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
        let mut cache = bead_render::tree::TreeCache::default()
            .with_image_policy(bead_render::tree::ImagePolicy::DeferredUpload);
        cache.prepare(&fixture, 1.).unwrap();
        let payload = cache.frame.as_ref().unwrap().items[0]
            .image
            .clone()
            .unwrap();
        assert!(payload.pending());
        let commits = Arc::new(AtomicUsize::new(0));
        let count = commits.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Mock {
                    commits: count,
                    panic: false,
                    scales: vec![],
                    expected_scales: None,
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send((event, std::thread::current().id()));
        }));
        let caller = std::thread::current().id();
        source
            .prepare_images(vec![payload.clone(), payload.clone()])
            .unwrap();
        let (event, worker) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(event, UiEvent::ResourcesPrepared(Ok(()))));
        assert_ne!(caller, worker);
        assert!(!payload.pending());
        assert!(
            payload.mark_requested(),
            "worker completes the in-flight flag"
        );
        payload.finish_request(None);
        assert_eq!(
            commits.load(Ordering::SeqCst),
            0,
            "hydration never commits source state"
        );
        assert!(
            receiver.try_recv().is_err(),
            "duplicate payload produces one hydration job"
        );
    }
    #[test]
    fn an_idle_worker_hydrates_the_zero_opacity_images_its_frame_skipped() {
        // B0377: a far pick's new pearls start at opacity 0; they bake behind the frame.
        let mut fixture = scene();
        fixture.primitives[0] = bead_render::Primitive::Image {
            opacity: 0.,
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
            bake_size: None,
            radius: 0.,
            tint: None,
        };
        fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
        let mut cache = bead_render::tree::TreeCache::default()
            .with_image_policy(bead_render::tree::ImagePolicy::UploadOnly);
        cache.prepare(&fixture, 1.).unwrap();
        let payload = cache.next_prefetch().expect("skipped by the frame");
        let (sender, receiver) = mpsc::channel();
        sender.send(Job::Idle(false)).unwrap();
        assert!(matches!(next_job(&receiver, &cache), Ok(Job::Idle(false))));
        assert!(payload.pending(), "a waiting job goes first");
        let worker =
            std::thread::spawn(move || matches!(next_job(&receiver, &cache), Ok(Job::Stop)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while payload.pending() {
            assert!(std::time::Instant::now() < deadline, "hydrated unasked");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        sender.send(Job::Stop).unwrap();
        assert!(worker.join().unwrap(), "then it waits for the next job");
    }
    #[test]
    fn motion_actions_keep_latest_same_name_and_preserve_action_barriers() {
        let mut queue = VecDeque::new();
        let motion = |name: &str, x| {
            Job::Stage(
                UiEvent::MotionAction {
                    name: name.into(),
                    argument: bead_core::Value::list(vec![serde_json::json!({
                        "phase":"move", "x":x
                    })]),
                },
                vec![1.5],
            )
        };
        let action = |phase: &str| {
            Job::Stage(
                UiEvent::Action {
                    name: "test.drag".into(),
                    argument: bead_core::Value::list(vec![serde_json::json!({
                        "phase":phase
                    })]),
                },
                vec![1.5],
            )
        };
        for x in 0..100 {
            enqueue(&mut queue, motion("test.drag", x));
        }
        enqueue(&mut queue, action("start"));
        enqueue(&mut queue, motion("test.other", 100));
        enqueue(&mut queue, motion("test.drag", 101));
        enqueue(&mut queue, action("drop"));
        enqueue(&mut queue, motion("test.drag", 102));

        assert_eq!(queue.len(), 6);
        let Some(Job::Stage(UiEvent::MotionAction { name, argument }, _)) = queue.pop_front()
        else {
            panic!("latest matching motion was not retained")
        };
        assert_eq!(name, "test.drag");
        assert_eq!(
            argument,
            bead_core::Value::list(vec![serde_json::json!({
                "phase":"move", "x":99
            })])
        );
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Action { argument, .. }, _))
                if argument == bead_core::Value::list(vec![serde_json::json!({"phase":"start"})])
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::MotionAction { name, .. }, _)) if name == "test.other"
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::MotionAction { name, argument, .. }, _))
                if name == "test.drag" && argument == bead_core::Value::list(vec![serde_json::json!({"phase":"move","x":101})])
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::Action { argument, .. }, _))
                if argument == bead_core::Value::list(vec![serde_json::json!({"phase":"drop"})])
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(Job::Stage(UiEvent::MotionAction { argument, .. }, _))
                if argument == bead_core::Value::list(vec![serde_json::json!({"phase":"move","x":102})])
        ));
    }
    #[test]
    fn worker_requires_gpu_ack_before_store_commit_and_supplies_prebaked_pixels() {
        let commits = Arc::new(AtomicUsize::new(0));
        let count = commits.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Mock {
                    commits: count,
                    panic: false,
                    scales: Vec::new(),
                    expected_scales: Some(vec![1.5]),
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.scales(vec![1.5]);
        assert!(source.stage(UiEvent::Reload).unwrap().is_none());
        let ready = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let prepared = source.stage(ready).unwrap().unwrap();
        assert!(prepared.prepared.contains_key(&1.5_f32.to_bits()));
        assert_eq!(commits.load(Ordering::SeqCst), 0);
        assert!(!source.commit());
        let accepted = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        source.stage(accepted).unwrap();
        assert!(source.commit());
        assert_eq!(commits.load(Ordering::SeqCst), 1);
        assert_eq!(source.state_summary(), "accepted");
        assert_eq!(source.saved_state(), serde_json::json!({"page":"details"}));
        assert_eq!(source.pointer_cursor(8., 8.), "pointer");
        assert!(
            source.allows_edge_hover(8., 8.),
            "the accepted worker mirror must preserve edge-hover opt-in"
        );
    }
    #[test]
    fn worker_stop_in_initial_drain_cancels_unacknowledged_stage() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut source = Source::new(
            Box::new(move || {
                let _ = started_tx.send(());
                release_rx.recv().map_err(|error| error.to_string())?;
                Ok(Box::new(Mock {
                    commits: Arc::new(AtomicUsize::new(0)),
                    panic: false,
                    scales: Vec::new(),
                    expected_scales: None,
                }) as Box<dyn SceneSource>)
            }),
            scene(),
        );
        let (wakeup, _events) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = wakeup.send(event);
        }));
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker factory should start");

        assert!(source.stage(UiEvent::Reload).unwrap().is_none());
        // Send exactly one Stop and take the sender so Source::drop only joins;
        // otherwise a second Stop could conceal the local-queue deadlock.
        let stop_sender = source.sender.take().expect("worker sender");
        assert!(stop_sender.send(Job::Stop).is_ok());

        let (dropped_tx, dropped_rx) = mpsc::channel();
        let dropper = std::thread::spawn(move || {
            drop(source);
            let _ = dropped_tx.send(());
        });
        release_tx.send(()).expect("release worker factory");
        dropped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Stop queued behind Stage must not wait for an acknowledgement");
        // Keep the channel connected until the assertion: disconnecting it
        // would also unblock the old ack loop and mask the regression.
        drop(stop_sender);
        dropper.join().expect("source drop thread should finish");
    }
    #[test]
    fn worker_panic_becomes_fatal_event_instead_of_silent_frozen_ui() {
        let mut source = Source::new(
            Box::new(|| {
                Ok(Box::new(Mock {
                    commits: Arc::new(AtomicUsize::new(0)),
                    panic: true,
                    scales: Vec::new(),
                    expected_scales: None,
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        assert!(source.alive());
        assert!(source.stage(UiEvent::Reload).unwrap().is_none());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            UiEvent::Fatal(_)
        ));
        assert!(!source.alive(), "dead before its Fatal arrives (B0345)");
    }
    #[test]
    fn failed_factory_is_dead_before_its_failed_packet_arrives() {
        let mut source = Source::new(Box::new(|| Err("boom".into())), scene());
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        let failed = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(&failed, UiEvent::Worker(packet) if packet.is_failed() && packet.error() == Some("boom"))
        );
        assert!(!source.alive());
        // The host still installs it; the queued Failed packet then tears its panel down.
        assert!(source.watch(Box::new(|_| {})).is_ok());
    }
    struct Warm {
        stages: Arc<AtomicUsize>,
        fail: Arc<std::sync::atomic::AtomicBool>,
    }
    impl SceneSource for Warm {
        fn warm_retained_bytes(&self) -> Option<usize> {
            Some(16 * 1024)
        }
        fn resume(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("stale source".into());
            }
            Ok(Some(scene()))
        }
        fn watch(
            &mut self,
            _: Box<dyn Fn(Result<(), String>) + Send>,
        ) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn stage(&mut self, _: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
            self.stages.fetch_add(1, Ordering::SeqCst);
            Ok(Some(scene()))
        }
        fn validate(&self) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn state_summary(&self) -> String {
            String::new()
        }
        fn commit(&mut self) -> bool {
            true
        }
        fn error_scene(&mut self, _: String) -> Scene {
            scene()
        }
    }
    fn warm_worker(
        factories: Arc<AtomicUsize>,
        stages: Arc<AtomicUsize>,
        fail: Arc<std::sync::atomic::AtomicBool>,
    ) -> (Box<dyn SceneSource>, mpsc::Receiver<UiEvent>) {
        let mut source: Box<dyn SceneSource> = Box::new(Source::new(
            Box::new(move || {
                factories.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(Warm { stages, fail }))
            }),
            scene(),
        ));
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.stage(UiEvent::Reload).unwrap();
        settle(source.as_mut(), &receiver);
        (source, receiver)
    }
    fn settle(source: &mut dyn SceneSource, receiver: &mpsc::Receiver<UiEvent>) {
        let ready = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(&ready, UiEvent::Worker(packet) if matches!(packet.phase, Phase::Ready)));
        assert!(source.stage(ready).unwrap().is_some());
        assert!(!source.dormant_ready(), "Ready is not committed");
        assert!(!source.commit());
        let accepted = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(&accepted, UiEvent::Worker(packet) if packet.is_accepted()));
        source.stage(accepted).unwrap();
        assert!(source.commit());
        assert!(source.dormant_ready());
    }
    #[test]
    fn six_dormant_cycles_reuse_one_factory_and_resume_through_ack() {
        let factories = Arc::new(AtomicUsize::new(0));
        let stages = Arc::new(AtomicUsize::new(0));
        let (mut source, receiver) =
            warm_worker(factories.clone(), stages.clone(), Default::default());
        let charge = source.warm_retained_bytes().unwrap();
        assert!(
            charge >= STACK_BYTES,
            "charge includes the reserved stack, not an RSS claim"
        );
        let mut cache = crate::warm::Cache::default();
        for _ in 0..6 {
            cache.park(false, "view".into(), 7, source, charge + 1024);
            let entry = cache.take(false, "view").expect("admitted settled worker");
            assert_eq!(entry.epoch, 7);
            source = entry.source;
            assert!(
                source.resume().unwrap().is_none(),
                "cached pixels wait for Accepted"
            );
            assert!(!source.dormant_ready(), "queued resume cannot park");
            settle(source.as_mut(), &receiver);
        }
        assert_eq!(factories.load(Ordering::SeqCst), 1);
        assert_eq!(
            stages.load(Ordering::SeqCst),
            1,
            "resume does not replay initial stage"
        );
        println!("warm_cycles=6 factories=1 initial_stages=1 admission_charge_bytes={charge}");
    }
    #[test]
    fn dormant_admission_rejects_queued_transactions_and_over_budget_sources() {
        let (mut source, receiver) =
            warm_worker(Default::default(), Default::default(), Default::default());
        assert!(source.dormant_ready());
        source.stage(UiEvent::Relayout).unwrap();
        assert!(
            !source.dormant_ready(),
            "counter covers the gap before Ready arrives"
        );
        let mut cache = crate::warm::Cache::default();
        cache.park(false, "pending".into(), 1, source, usize::MAX);
        assert!(cache.take(false, "pending").is_none());
        drop(receiver);
        let (source, _receiver) =
            warm_worker(Default::default(), Default::default(), Default::default());
        let charge = source.warm_retained_bytes().unwrap();
        cache.park(false, "large".into(), 2, source, charge - 1);
        assert!(cache.take(false, "large").is_none());
    }
    #[test]
    fn dormant_budget_evicts_oldest_and_wakeups_match_slot_and_original_epoch() {
        let mut cache = crate::warm::Cache::default();
        for epoch in [1, 2] {
            let (source, _receiver) =
                warm_worker(Default::default(), Default::default(), Default::default());
            let budget = source.warm_retained_bytes().unwrap() + 1024;
            cache.park(false, epoch.to_string(), epoch, source, budget);
        }
        assert!(
            cache.take(false, "1").is_none(),
            "oldest source evicted before admission"
        );
        assert!(
            !cache.invalidate(true, 2),
            "overlay epochs cannot evict panels"
        );
        assert!(
            !cache.invalidate(false, 1),
            "stale epochs cannot evict a replacement"
        );
        assert!(
            cache.invalidate(false, 2),
            "any dormant source wakeup retires the worker"
        );
        assert!(cache.take(false, "2").is_none());
    }
    #[test]
    fn rejected_resume_returns_no_cached_pixels_and_cannot_be_reparked() {
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (mut source, receiver) =
            warm_worker(Default::default(), Default::default(), fail.clone());
        fail.store(true, Ordering::SeqCst);
        assert!(source.resume().unwrap().is_none());
        let failed = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(&failed, UiEvent::Worker(packet) if packet.resume_failed() && packet.scene.is_none())
        );
        assert!(source.stage(failed).is_err());
        assert!(source.alive(), "a rejected resume keeps its worker");
        assert!(!source.dormant_ready());
        let mut cache = crate::warm::Cache::default();
        cache.park(false, "failed".into(), 3, source, usize::MAX);
        assert!(cache.take(false, "failed").is_none());
    }
    #[test]
    fn coalesced_pointer_jobs_release_pending_credit_before_parking() {
        let stages = Arc::new(AtomicUsize::new(0));
        let (mut source, receiver) =
            warm_worker(Default::default(), stages.clone(), Default::default());
        source.stage(UiEvent::Relayout).unwrap();
        let ready = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        source.stage(ready).unwrap();
        // Hold Ack until all samples are queued, so the actor combines them while
        // waiting for this first transaction's GPU acceptance.
        for x in 0..100 {
            source.stage(UiEvent::Hover { x: x as f32, y: 1. }).unwrap();
        }
        assert!(!source.dormant_ready());
        assert!(!source.commit());
        let accepted = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        source.stage(accepted).unwrap();
        assert!(source.commit());
        settle(source.as_mut(), &receiver);
        assert_eq!(
            stages.load(Ordering::SeqCst),
            3,
            "initial + relayout + one combined hover"
        );
        let budget = source.warm_retained_bytes().unwrap() + 1024;
        let mut cache = crate::warm::Cache::default();
        cache.park(false, "coalesced".into(), 8, source, budget);
        assert!(cache.take(false, "coalesced").is_some());
    }
    #[test]
    fn six_alternating_named_cycles_keep_two_factories_with_one_dormant_slot() {
        for overlay in [false, true] {
            let factories = Arc::new(AtomicUsize::new(0));
            let stages = Arc::new(AtomicUsize::new(0));
            let (mut active, a_events) =
                warm_worker(factories.clone(), stages.clone(), Default::default());
            let (mut other, b_events) =
                warm_worker(factories.clone(), stages.clone(), Default::default());
            let budget = active
                .warm_retained_bytes()
                .unwrap()
                .max(other.warm_retained_bytes().unwrap())
                + 1024;
            other.stage(UiEvent::PanelVisibility(false)).unwrap();
            settle(other.as_mut(), &b_events);
            let mut cache = crate::warm::Cache::default();
            cache.park(overlay, "B".into(), 2, other, budget);
            let (mut active_name, mut active_epoch) = ("A", 1);
            for requested in ["B", "A"].into_iter().cycle().take(12) {
                active.stage(UiEvent::PanelVisibility(false)).unwrap();
                settle(
                    active.as_mut(),
                    if active_name == "A" {
                        &a_events
                    } else {
                        &b_events
                    },
                );
                // Match both slot lifecycles: reserve requested source FIRST,
                // then retire the current source into the one-entry budget.
                let next = cache
                    .take(overlay, requested)
                    .expect("alternating target stays warm");
                cache.park(overlay, active_name.into(), active_epoch, active, budget);
                active = next.source;
                active_name = requested;
                active_epoch = next.epoch;
                active.stage(UiEvent::PanelVisibility(true)).unwrap();
                settle(
                    active.as_mut(),
                    if requested == "A" {
                        &a_events
                    } else {
                        &b_events
                    },
                );
                assert!(active.resume().unwrap().is_none());
                settle(
                    active.as_mut(),
                    if requested == "A" {
                        &a_events
                    } else {
                        &b_events
                    },
                );
            }
            assert_eq!(
                factories.load(Ordering::SeqCst),
                2,
                "one initial factory per named view"
            );
            assert_eq!(
                stages.load(Ordering::SeqCst),
                27,
                "two initial stages, one initial close, and close/open for twelve swaps"
            );
            assert!(
                cache.take(overlay, "B").is_some(),
                "inactive B still occupies the single budget slot"
            );
        }
    }
    #[test]
    fn requested_stack_changes_admission_charge_without_overriding_rust_policy() {
        assert_eq!(stack_charge(None), STACK_BYTES);
        assert_eq!(stack_charge(Some("invalid")), STACK_BYTES);
        assert_eq!(stack_charge(Some("1048576")), STACK_BYTES);
        assert_eq!(stack_charge(Some("8388608")), 8 * bead_core::cache::MIB);
    }
    #[test]
    fn dormant_empty_model_hints_wait_for_resume_but_payloads_and_reload_evict() {
        for overlay in [false, true] {
            let factories = Arc::new(AtomicUsize::new(0));
            let stages = Arc::new(AtomicUsize::new(0));
            let (source, events) =
                warm_worker(factories.clone(), stages.clone(), Default::default());
            let budget = source.warm_retained_bytes().unwrap() + 1024;
            let mut cache = crate::warm::Cache::default();
            cache.park(overlay, "hinted".into(), 4, source, budget);
            assert!(!cache.handle_event(overlay, 3, &UiEvent::Model(Default::default())));
            for _ in 0..100 {
                assert!(cache.handle_event(overlay, 4, &UiEvent::Model(Default::default())));
            }
            let mut source = cache
                .take(overlay, "hinted")
                .expect("empty hints retain source")
                .source;
            assert!(source.resume().unwrap().is_none());
            settle(source.as_mut(), &events);
            assert_eq!(factories.load(Ordering::SeqCst), 1);
            assert_eq!(
                stages.load(Ordering::SeqCst),
                1,
                "no hidden model evaluations"
            );
            cache.park(overlay, "hinted".into(), 4, source, budget);
            let payload = UiEvent::Model(std::collections::BTreeMap::from([(
                "changed".into(),
                bead_core::Value::Bool(true),
            )]));
            assert!(cache.handle_event(overlay, 4, &payload));
            assert!(
                cache.take(overlay, "hinted").is_none(),
                "payload requires cold refresh"
            );
            assert!(
                matches!(
                    events.recv_timeout(Duration::from_secs(2)),
                    Err(mpsc::RecvTimeoutError::Disconnected)
                ),
                "eviction sends Stop and ends the actual worker"
            );
            let (source, _events) =
                warm_worker(Default::default(), Default::default(), Default::default());
            let budget = source.warm_retained_bytes().unwrap() + 1024;
            cache.park(overlay, "reload".into(), 5, source, budget);
            assert!(cache.handle_event(overlay, 5, &UiEvent::Reload));
            assert!(
                cache.take(overlay, "reload").is_none(),
                "explicit reload keeps force-refresh semantics"
            );
        }
    }

    #[test]
    fn repeated_reload_failures_are_reported_once_until_a_clean_frame() {
        let mut source = Source::new(Box::new(|| Err("unused".into())), scene());
        let packet = |phase, error: Option<&str>| Packet {
            resume: false,
            animation_frame: false,
            phase,
            token: 1,
            scene: None,
            mirror: None,
            error: error.map(Into::into),
            reload: true,
            wallpaper_palette: None,
        };
        let broken = Some("main.luau:2:1: syntax error");
        assert!(source.report(&packet(Phase::Ready, broken)));
        assert!(!source.report(&packet(Phase::Accepted, broken)));
        assert!(!source.report(&packet(Phase::Ready, broken)));
        assert!(source.report(&packet(Phase::Ready, Some("main.luau:3:1: other"))));
        assert!(!source.report(&packet(Phase::Ready, None)));
        assert!(!source.report(&packet(Phase::Accepted, None)));
        assert!(source.report(&packet(Phase::Ready, broken)));
        assert!(!source.report(&packet(Phase::Failed, Some("bake failed"))));
    }

    fn palette(red: u8) -> BTreeMap<String, [u8; 4]> {
        BTreeMap::from([("surface".into(), [red, 0, 0, 255])])
    }
    fn command(name: &str) -> UiEvent {
        UiEvent::Command {
            command: name.into(),
            shift: false,
        }
    }
    #[test]
    fn queue_keeps_the_newest_palette_and_one_repeat_and_releases_their_credit() {
        let (pending, mut absorbed) = (AtomicUsize::new(0), Absorbed::default());
        let mut queue = VecDeque::new();
        for job in [
            Job::Stage(UiEvent::WallpaperPalette(palette(1)), vec![1.]),
            Job::Stage(command("tab"), vec![1.]),
            Job::Stage(UiEvent::WallpaperPalette(palette(2)), vec![1.]),
            Job::Repeat(command("left"), vec![1.]),
            Job::Repeat(command("left"), vec![1.]),
            Job::Stage(command("left"), vec![1.]),
            Job::Repeat(command("left"), vec![1.]),
        ] {
            pending.fetch_add(1, Ordering::AcqRel);
            enqueue_pending(&mut queue, job, &pending, &mut absorbed);
        }
        assert!(matches!(
            queue.make_contiguous(),
            [
                Job::Stage(UiEvent::Command { .. }, _),
                Job::Stage(UiEvent::WallpaperPalette(newest), _),
                Job::Repeat(..),
                Job::Stage(UiEvent::Command { .. }, _),
            ] if *newest == palette(2)
        ));
        assert_eq!(pending.load(Ordering::Acquire), queue.len());
        assert_eq!(absorbed.palettes, [palette(1)]);
        assert_eq!((absorbed.replaced, absorbed.repeats), (1, 2));
    }
    /// Logs each staged event and stages `scene`; the first stage (or `presented`) waits for
    /// the gate, so later jobs queue.
    struct Gated {
        log: Arc<std::sync::Mutex<Vec<String>>>,
        gate: Option<mpsc::Receiver<()>>,
        scene: Scene,
    }
    impl SceneSource for Gated {
        fn watch(
            &mut self,
            _: Box<dyn Fn(Result<(), String>) + Send>,
        ) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
            if let Some(gate) = self.gate.take() {
                let _ = gate.recv();
            }
            self.log.lock().unwrap().push(match event {
                UiEvent::WallpaperPalette(values) => format!("palette {}", values["surface"][0]),
                UiEvent::Command { command, .. } => command,
                event => event_kind(&event),
            });
            Ok(Some(self.scene.clone()))
        }
        fn presented(&mut self, _: u64) {
            if let Some(gate) = self.gate.take() {
                let _ = gate.recv();
            }
        }
        fn validate(&self) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn state_summary(&self) -> String {
            String::new()
        }
        fn commit(&mut self) -> bool {
            true
        }
        fn error_scene(&mut self, _: String) -> Scene {
            scene()
        }
    }
    #[test]
    fn stalled_worker_settles_replaced_palettes_with_the_newest_and_runs_one_repeat() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (open, gate) = mpsc::channel();
        let staged = log.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Gated {
                    log: staged,
                    gate: Some(gate),
                    scene: scene(),
                }))
            }),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.stage(UiEvent::Reload).unwrap();
        // The view stalls: a fenced palette, a press, a tween step and a held key queue up.
        source.stage(UiEvent::WallpaperPalette(palette(1))).unwrap();
        source.stage(command("tab")).unwrap();
        source.stage(UiEvent::WallpaperPalette(palette(2))).unwrap();
        for _ in 0..5 {
            source.stage_repeat(command("left")).unwrap();
        }
        source.stage(command("right")).unwrap();
        source.stage_repeat(command("left")).unwrap();
        open.send(()).unwrap();
        let mut settled = Vec::new();
        while log.lock().unwrap().len() < 5 || source.pending.load(Ordering::Acquire) > 0 {
            let Ok(UiEvent::Worker(packet)) = receiver.recv_timeout(Duration::from_secs(2)) else {
                panic!("worker packet");
            };
            if packet.is_accepted() && !packet.wallpaper_palettes().is_empty() {
                // The ack comes after the newest palette staged, never at the replaced one.
                assert!(log.lock().unwrap().contains(&"palette 2".into()));
                settled.push(packet.wallpaper_palettes().to_vec());
                assert_eq!(packet.wallpaper_palette(), Some(&palette(2)));
            }
            source.stage(UiEvent::Worker(packet)).unwrap();
            source.commit();
        }
        assert_eq!(settled, [[palette(1), palette(2)]], "both settle, once");
        // One repeat ran after the stall; the press behind it still did.
        assert_eq!(
            *log.lock().unwrap(),
            ["Reload", "tab", "palette 2", "left", "right"]
        );
    }

    fn image_scene(size: f32) -> Scene {
        let mut fixture = scene();
        fixture.primitives[0] = bead_render::Primitive::Image {
            opacity: 1.,
            rect: bead_render::Rect::new(0., 0., size, size),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
            bake_size: None,
            radius: 0.,
            tint: None,
        };
        fixture.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
        fixture
    }
    /// B0337: an UploadOnly worker holds in `presented` while a stage, a CPU preparation and
    /// (when `restore_queued`) a policy restore queue behind it, in that order; returns once
    /// the host holds the stage's Ready.
    fn ready_with_a_queued_preparation(
        view: Scene,
        restore_queued: bool,
    ) -> (Source, mpsc::Receiver<UiEvent>) {
        let (open, gate) = mpsc::channel();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Gated {
                    log: Default::default(),
                    gate: Some(gate),
                    scene: view,
                }))
            }),
            scene(),
        )
        .with_image_policy(bead_render::tree::ImagePolicy::UploadOnly);
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.presented(1);
        source.stage(UiEvent::Reload).unwrap();
        let request = bead_render::renderer(bead_render::RendererKind::Cpu)
            .unwrap()
            .prepare_on_ui(&image_scene(16.), 1.)
            .unwrap_err()
            .downcast_ref::<bead_render::tree::CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        source.prepare_cpu(request).unwrap();
        if restore_queued {
            source.restore_image_policy();
        }
        open.send(()).unwrap();
        let ready = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(&ready, UiEvent::Worker(packet) if matches!(packet.phase, Phase::Ready)));
        assert!(source.stage(ready).unwrap().is_some());
        (source, receiver)
    }
    struct ClosedPanel(Arc<AtomicUsize>, Arc<std::sync::atomic::AtomicBool>);
    impl SceneSource for ClosedPanel {
        fn watch(
            &mut self,
            _: Box<dyn Fn(Result<(), String>) + Send>,
        ) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn stage(&mut self, _: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let mut view = scene();
            view.motion = Some(bead_render::Motion {
                id: "panel".into(),
                open: self.1.load(Ordering::SeqCst),
                duration: 0.3,
                return_duration: None,
                return_curve: None,
                auto_open: false,
                spring: false,
                curve: None,
                edge: bead_core::Edge::Top,
                slide_distance: None,
                scale_from: None,
                fade: false,
                reveal: false,
                progress: 0.,
                parts: Vec::new(),
                art: None,
                body: None,
            });
            Ok(Some(view))
        }
        fn validate(&self) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn state_summary(&self) -> String {
            String::new()
        }
        fn commit(&mut self) -> bool {
            true
        }
        fn error_scene(&mut self, _: String) -> Scene {
            scene()
        }
    }
    #[test]
    fn a_closed_panel_settles_data_and_palette_but_bakes_animation_frames() {
        let stages = Arc::new(AtomicUsize::new(0));
        let open = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (counter, flag) = (stages.clone(), open.clone());
        let mut source = Source::new(
            Box::new(move || Ok(Box::new(ClosedPanel(counter, flag)))),
            scene(),
        );
        let (sender, receiver) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        let model = || UiEvent::Model([("tick".to_string(), bead_core::Value::Bool(true))].into());
        let events: [fn() -> UiEvent; 3] = [
            || UiEvent::Model([("tick".to_string(), bead_core::Value::Bool(true))].into()),
            || UiEvent::Model(Default::default()),
            || UiEvent::WallpaperPalette([("surface".to_string(), [1, 2, 3, 4])].into()),
        ];
        let next = || match receiver.recv_timeout(Duration::from_secs(2)).unwrap() {
            UiEvent::Worker(packet) => packet,
            _ => panic!("a worker packet"),
        };
        // Never told idle: the staged scene alone says closed.
        for n in [0, 2] {
            source.stage(events[n]()).unwrap();
            let packet = next();
            assert!(matches!(packet.phase, Phase::Settled) && packet.scene.is_none());
            assert_eq!(
                packet.wallpaper_palette().is_some(),
                n == 2,
                "palette rides the Settled packet"
            );
        }
        assert_eq!(stages.load(Ordering::SeqCst), 2, "the source still ran");
        // An empty-Model frame drives a closing animation: it bakes although the scene is closed.
        source.stage(events[1]()).unwrap();
        let packet = next();
        assert!(matches!(packet.phase, Phase::Ready));
        source.stage(UiEvent::Worker(packet)).unwrap();
        source.commit();
        let accepted = next();
        source.stage(UiEvent::Worker(accepted)).unwrap();
        assert!(source.commit());
        // Open: every stage bakes and hands the host its candidate.
        open.store(true, Ordering::SeqCst);
        for event in events {
            source.stage(event()).unwrap();
            let packet = next();
            assert!(matches!(packet.phase, Phase::Ready));
            assert!(!packet.scene.as_ref().unwrap().prepared.is_empty());
            assert!(source.stage(UiEvent::Worker(packet)).unwrap().is_some());
            source.commit();
            let accepted = next();
            assert!(accepted.is_accepted());
            source.stage(UiEvent::Worker(accepted)).unwrap();
            assert!(source.commit());
        }
        // B0363: the data update that closes an open panel bakes, so the host runs the close.
        open.store(false, Ordering::SeqCst);
        source.stage(model()).unwrap();
        let packet = next();
        assert!(matches!(packet.phase, Phase::Ready));
        source.stage(UiEvent::Worker(packet)).unwrap();
        source.commit();
        let accepted = next();
        source.stage(UiEvent::Worker(accepted)).unwrap();
        assert!(source.commit());
        // Once the host holds the closed scene it says idle, and later updates settle.
        source.set_idle(true);
        source.stage(model()).unwrap();
        assert!(matches!(next().phase, Phase::Settled));
        // Reopen after closed settles: the first scene is baked.
        open.store(true, Ordering::SeqCst);
        source.stage(UiEvent::Relayout).unwrap();
        assert!(matches!(next().phase, Phase::Ready));
    }
    #[test]
    fn a_preparation_queued_behind_a_stage_runs_before_that_stage_waits_for_its_ack() {
        let (_source, receiver) = ready_with_a_queued_preparation(scene(), false);
        // A host whose upload waits for this preparation would never send the Ack.
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(2)),
            Ok(UiEvent::ResourcesPrepared(Ok(())))
        ));
    }
    #[test]
    fn a_policy_restore_after_a_queued_cpu_preparation_leaves_the_worker_upload_only() {
        for restore_queued in [true, false] {
            let (mut source, receiver) =
                ready_with_a_queued_preparation(image_scene(12.), restore_queued);
            if !restore_queued {
                // Sent after the PrepareCpu, it arrives while the worker waits for the Ack.
                source.restore_image_policy();
            }
            assert!(!source.commit(), "acknowledges the Ready");
            let (mut prepared, mut accepted) = (false, false);
            while !(prepared && accepted) {
                match receiver.recv_timeout(Duration::from_secs(2)).unwrap() {
                    UiEvent::ResourcesPrepared(result) => prepared = result.is_ok(),
                    UiEvent::Worker(packet) if packet.is_accepted() => {
                        source.stage(UiEvent::Worker(packet)).unwrap();
                        accepted = source.commit();
                    }
                    _ => {}
                }
            }
            source.stage(command("left")).unwrap();
            let Ok(UiEvent::Worker(ready)) = receiver.recv_timeout(Duration::from_secs(2)) else {
                panic!("the next stage's Ready");
            };
            let frame = &ready.scene.as_ref().unwrap().prepared[&1f32.to_bits()];
            assert!(
                frame.items[0].image.is_some() && frame.items[0].raster.is_none(),
                "the Dense adoption ran first, the restore last (restore_queued={restore_queued})"
            );
        }
    }
}
