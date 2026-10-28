// SPDX-License-Identifier: MIT OR Apache-2.0
//! Script/CPU preparation actor; validated candidates cross the draw thread by value.
use crate::{InputRegion, PanelAnchor, SceneSource, UiEvent};
use bead_render::{Rect, Scene};
use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};
pub type Factory = Box<dyn FnOnce() -> Result<Box<dyn SceneSource>, String> + Send>;
#[derive(Default)]
struct Mirror {
    drag_visuals: Vec<crate::DragVisual>,
    focused_dismiss: bool,
    tracks_origin: bool,
    outside_dismiss: bool,
    wheel_step: f64,
    scroll_regions: Vec<Rect>,
    regions: Vec<InputRegion>,
    surrounding: Option<(String, i32, i32)>,
    sensitive: bool,
    tree: Option<accesskit::TreeUpdate>,
    summary: String,
    resources: String,
    saved_state: serde_json::Value,
}
impl Mirror {
    fn region_at(&self, x: f32, y: f32) -> Option<&InputRegion> {
        self.regions.iter().rev().find(|r| r.contains(x, y))
    }

    fn read(source: &dyn SceneSource) -> Self {
        let _profile = bead_core::profile::Span::new("mirror");
        Self {
            drag_visuals: source.drag_visuals(),
            focused_dismiss: source.focused_dismiss(),
            tracks_origin: source.tracks_origin(),
            outside_dismiss: source.outside_dismiss(),
            wheel_step: source.wheel_step(),
            scroll_regions: source.scroll_regions(),
            regions: source.input_regions(),
            surrounding: source.surrounding_text(),
            sensitive: source.sensitive_input(),
            tree: source.accessibility_tree(),
            summary: source.state_summary(),
            resources: source.resource_counts(),
            saved_state: source.saved_state(),
        }
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
    phase: Phase,
    token: u64,
    scene: Option<Scene>,
    mirror: Option<Arc<Mirror>>,
    error: Option<String>,
    reload: bool,
    wallpaper_palette: Option<std::collections::BTreeMap<String, [u8; 4]>>,
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
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }
    pub fn is_reload(&self) -> bool {
        self.reload
    }
    pub fn wallpaper_palette(&self) -> Option<&std::collections::BTreeMap<String, [u8; 4]>> {
        self.wallpaper_palette.as_ref()
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
    PanelCenter(Option<f32>),
    InitialModel(std::collections::BTreeMap<String, bead_core::Value>),
    PanelAnchor(Option<PanelAnchor>),
    Presented(u64),
    Idle(bool),
    FrameInterval(std::time::Duration),
    PresentationMotion(bool),
    Stage(UiEvent, Vec<f32>),
    ContentScroll(UiEvent, Vec<f32>),
    Viewport(u32, u32),
    Watch(Box<dyn Fn(Result<(), String>) + Send>),
    Ack(u64, Result<(), String>),
    Stop,
}
#[derive(Default)]
struct DeferredRefresh {
    model: BTreeMap<String, bead_core::Value>,
    reload: bool,
    desktop: Option<Arc<bead_services::Snapshot>>,
    backend: Option<Arc<bead_backends::Snapshot>>,
    wallpaper_palette: Option<BTreeMap<String, [u8; 4]>>,
    presented: Option<u64>,
}
impl DeferredRefresh {
    fn defer(&mut self, event: &UiEvent) -> bool {
        match event {
            UiEvent::Reload => {
                self.reload = true;
                true
            }
            UiEvent::Model(values) => {
                for (key, value) in values {
                    self.model.insert(key.clone(), value.clone());
                }
                true
            }
            UiEvent::Desktop(snapshot) => {
                if self
                    .desktop
                    .as_ref()
                    .is_none_or(|old| snapshot.revision >= old.revision)
                {
                    self.desktop = Some(snapshot.clone());
                }
                true
            }
            UiEvent::Backend(snapshot) => {
                if self
                    .backend
                    .as_ref()
                    .is_none_or(|old| snapshot.epoch >= old.epoch)
                {
                    self.backend = Some(snapshot.clone());
                }
                true
            }
            UiEvent::WallpaperPalette(palette) => {
                self.wallpaper_palette = Some(palette.clone());
                true
            }
            _ => false,
        }
    }

    fn defer_presented(&mut self, revision: u64) {
        self.presented = Some(self.presented.map_or(revision, |old| old.max(revision)));
    }
}
// Pointer positions are replaceable only within the same gesture. Every other
// job is an ordering barrier (especially press, release and model updates).
fn enqueue(queue: &mut VecDeque<Job>, mut job: Job) {
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
    token: u64,
    accepted: bool,
    current: Scene,
    candidate: Option<Scene>,
    viewport: Option<(u32, u32)>,
    frame_interval: Option<std::time::Duration>,
    presentation_motion: Option<bool>,
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
            scales: vec![1.],
            token: 0,
            accepted: false,
            current: fallback,
            candidate: None,
            viewport: None,
            frame_interval: None,
            presentation_motion: None,
        }
    }
    pub fn with_image_policy(mut self, policy: bead_render::tree::ImagePolicy) -> Self {
        self.image_policy = policy;
        self
    }
    fn send(&self, job: Job) -> Result<(), Box<dyn Error>> {
        self.sender
            .as_ref()
            .ok_or("script worker not started")?
            .send(job)
            .map_err(|_| "script worker disconnected".into())
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Job::Stop);
        }
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
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
            *cache = bead_render::tree::TreeCache::default();
            let result = request.prepare();
            Ok(result)
        }
        job => Err(job),
    }
}
impl SceneSource for Source {
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
            self.send(Job::PrepareCpu(request))?;
        }
        Ok(())
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
        let retired = self.retired.clone();
        self.thread = Some(std::thread::spawn(move || {
            bead_core::profile::mark("worker_started");
            let wakeup: Arc<dyn Fn(UiEvent) + Send + Sync> = Arc::from(wakeup);
            let report = wakeup.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut source = match factory() {
                    Ok(source) => source,
                    Err(error) => {
                        wakeup(UiEvent::Worker(Arc::new(Packet {
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
                let service_wakeup = wakeup.clone();
                source.services(Box::new(move |event| service_wakeup(event)));
                let mut cache =
                    bead_render::tree::TreeCache::default().with_image_policy(image_policy);
                let mut queue = VecDeque::new();
                let mut token = 0;
                let mut idle = false;
                let mut needs_trim = false;
                let mut active_scales = vec![1.];
                let mut accepted_resource_revision = None;
                let mut presentation_motion = false;
                let mut pending_refresh = DeferredRefresh::default();
                'actor: loop {
                    for job in receiver.try_iter() {
                        match job {
                            Job::Stop => break 'actor,
                            job => enqueue(&mut queue, job),
                        }
                    }
                    let job = if let Some(job) = queue.pop_front() {
                        job
                    } else if presentation_motion {
                        match receiver.recv() {
                            Ok(job) => job,
                            Err(_) => break,
                        }
                    } else {
                        if let Some(deadline) = source.wakeup_at() {
                            match receiver.recv_timeout(
                                deadline.saturating_duration_since(std::time::Instant::now()),
                            ) {
                                Ok(job) => job,
                                Err(mpsc::RecvTimeoutError::Timeout) => Job::Stage(
                                    UiEvent::Model(Default::default()),
                                    active_scales.clone(),
                                ),
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
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
                            match receiver.recv() {
                                Ok(job) => job,
                                Err(_) => break,
                            }
                        }
                    };
                    let job = match prepare_resource_job(job, &mut cache) {
                        Ok(result) => {
                            wakeup(UiEvent::ResourcesPrepared(result));
                            continue;
                        }
                        Err(job) => job,
                    };
                    match job {
                        Job::PrepareImages(_) | Job::PrepareCpu(_) => {
                            unreachable!("resource jobs handled above")
                        }
                        Job::PanelCenter(center) => source.panel_center(center),
                        Job::InitialModel(values) => {
                            if let Err(error) = source.initial_model(values) {
                                wakeup(UiEvent::WatchError(error.to_string()));
                            }
                        }
                        Job::PanelAnchor(anchor) => source.panel_anchor(anchor),
                        Job::Stop => break,
                        Job::Presented(revision) if presentation_motion => {
                            pending_refresh.defer_presented(revision);
                        }
                        Job::Presented(revision) => source.presented(revision),
                        Job::PresentationMotion(active) => {
                            presentation_motion = active;
                            if !active {
                                if std::mem::take(&mut pending_refresh.reload) {
                                    queue.push_back(Job::Stage(
                                        UiEvent::Reload,
                                        active_scales.clone(),
                                    ));
                                }
                                if let Some(palette) = pending_refresh.wallpaper_palette.take() {
                                    queue.push_back(Job::Stage(
                                        UiEvent::WallpaperPalette(palette),
                                        active_scales.clone(),
                                    ));
                                }
                                if let Some(snapshot) = pending_refresh.desktop.take() {
                                    queue.push_back(Job::Stage(
                                        UiEvent::Desktop(snapshot),
                                        active_scales.clone(),
                                    ));
                                }
                                if let Some(snapshot) = pending_refresh.backend.take() {
                                    queue.push_back(Job::Stage(
                                        UiEvent::Backend(snapshot),
                                        active_scales.clone(),
                                    ));
                                }
                                if let Some(revision) = pending_refresh.presented.take() {
                                    queue.push_back(Job::Presented(revision));
                                }
                                if !pending_refresh.model.is_empty() {
                                    queue.push_back(Job::Stage(
                                        UiEvent::Model(std::mem::take(&mut pending_refresh.model)),
                                        active_scales.clone(),
                                    ));
                                }
                            }
                        }
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
                            if presentation_motion && pending_refresh.defer(&event) {
                                continue;
                            }
                            source.scales(scales.clone());
                            active_scales.clone_from(&scales);
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
                            let reload = matches!(event, UiEvent::Reload);
                            // Detaching or docking gives the panel another look: once it
                            // is accepted, the rasters only the old look used can go.
                            let look_change = matches!(event, UiEvent::PanelDetached(_));
                            let baked_after = cache.serial();
                            let wallpaper_palette =
                                if let UiEvent::WallpaperPalette(values) = &event {
                                    Some(values.clone())
                                } else {
                                    None
                                };
                            let _profile = bead_core::profile::Span::new("worker");
                            let started = std::time::Instant::now();
                            token += 1;
                            let (mut scene, fault) = match source.stage(event) {
                                Ok(Some(scene)) => (scene, None),
                                Ok(None) => {
                                    wakeup(UiEvent::Worker(Arc::new(Packet {
                                        phase: Phase::Settled,
                                        token,
                                        scene: None,
                                        mirror: Some(Arc::new(Mirror::read(source.as_ref()))),
                                        error: None,
                                        reload,
                                        wallpaper_palette,
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
                                                    job => enqueue(&mut queue, job),
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
                            bead_core::vprintln!(
                                "WORKER_PREPARE token={token} reload={reload} total_ms={:.3} builds={}",
                                started.elapsed().as_secs_f64() * 1000.,
                                cache.builds
                            );
                            wakeup(UiEvent::Worker(Arc::new(Packet {
                                phase: Phase::Ready,
                                token,
                                scene: Some(scene.clone()),
                                mirror: None,
                                error: fault.clone(),
                                reload,
                                wallpaper_palette: wallpaper_palette.clone(),
                            })));
                            // No later script event may overwrite this staged transaction before its GPU acknowledgement.
                            loop {
                                match receiver.recv() {
                                    Ok(Job::Stop) | Err(_) => break 'actor,
                                    Ok(job @ (Job::PrepareImages(_) | Job::PrepareCpu(_))) => {
                                        wakeup(UiEvent::ResourcesPrepared(
                                            prepare_resource_job(job, &mut cache)
                                                .unwrap_or_else(|_| unreachable!()),
                                        ));
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
                                                    phase: Phase::Accepted,
                                                    token,
                                                    scene: Some(scene),
                                                    mirror: Some(Arc::new(Mirror::read(
                                                        source.as_ref(),
                                                    ))),
                                                    error: fault,
                                                    reload,
                                                    wallpaper_palette: wallpaper_palette.clone(),
                                                })));
                                            }
                                            Err(error) => {
                                                let mut overlay = source.error_scene(error.clone());
                                                if bake(&mut cache, &mut overlay, &scales).is_ok() {
                                                    wakeup(UiEvent::Worker(Arc::new(Packet {
                                                        phase: Phase::Accepted,
                                                        token,
                                                        scene: Some(overlay),
                                                        mirror: Some(Arc::new(Mirror::read(
                                                            source.as_ref(),
                                                        ))),
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
                                    Ok(job) => enqueue(&mut queue, job),
                                }
                            }
                        }
                    }
                }
            }));
            if result.is_err() {
                report(UiEvent::Fatal("script worker panicked".into()));
            }
        }));
        if let Some(interval) = self.frame_interval {
            let _ = self.send(Job::FrameInterval(interval));
        }
        if let Some(active) = self.presentation_motion {
            let _ = self.send(Job::PresentationMotion(active));
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
    fn presentation_motion(&mut self, active: bool) {
        if self.presentation_motion == Some(active) {
            return;
        }
        self.presentation_motion = Some(active);
        let _ = self.send(Job::PresentationMotion(active));
    }
    fn set_idle(&mut self, idle: bool) {
        let _ = self.send(Job::Idle(idle));
    }
    fn panel_center(&mut self, center: Option<f32>) {
        let _ = self.send(Job::PanelCenter(center));
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
        self.send(Job::Watch(callback))
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        if let UiEvent::ResourcesPrepared(result) = event {
            result.map_err(|error| -> Box<dyn Error> { error.into() })?;
            self.preparation_retry = false;
            return Ok(self.candidate.clone());
        }
        if let UiEvent::Worker(packet) = event {
            self.token = packet.token;
            if let Some(mirror) = &packet.mirror {
                self.mirror = mirror.clone();
            }
            if let Some(error) = &packet.error {
                eprintln!("UI_ERROR {error}");
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
            self.candidate = packet.scene.clone();
            return Ok(packet.scene.clone());
        }
        let content_scroll = match &event {
            UiEvent::Scroll { x, y, .. } | UiEvent::HorizontalScroll { x, y, .. } => self
                .mirror
                .scroll_regions
                .iter()
                .any(|r| *x >= r.x && *y >= r.y && *x < r.x + r.w && *y < r.y + r.h),
            _ => false,
        };
        self.send(if content_scroll {
            Job::ContentScroll(event, self.scales.clone())
        } else {
            Job::Stage(event, self.scales.clone())
        })?;
        Ok(None)
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
        self.mirror.summary.clone()
    }
    fn saved_state(&self) -> serde_json::Value {
        self.mirror.saved_state.clone()
    }
    fn resource_counts(&self) -> String {
        self.mirror.resources.clone()
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        self.mirror.tree.clone()
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
    fn wheel_step(&self) -> f64 {
        self.mirror.wheel_step
    }
    fn outside_dismiss(&self) -> bool {
        self.mirror.outside_dismiss
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
            }],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 16., 16.),
        }
    }
    #[test]
    fn slide_defers_and_coalesces_model_refreshes_and_file_reload() {
        let mut pending = DeferredRefresh::default();
        let first = UiEvent::Model(BTreeMap::from([
            (
                "dashboard.title".into(),
                bead_core::Value::Text("old".into()),
            ),
            ("dashboard.count".into(), bead_core::Value::Number(1.)),
        ]));
        assert!(pending.defer(&first));
        let latest = UiEvent::Model(BTreeMap::from([(
            "dashboard.title".into(),
            bead_core::Value::Text("new".into()),
        )]));
        assert!(pending.defer(&latest));
        assert!(pending.defer(&UiEvent::Reload));
        pending.defer_presented(18);
        pending.defer_presented(21);
        assert_eq!(pending.presented, Some(21));
        assert!(
            pending.defer(&UiEvent::Desktop(Arc::new(bead_services::Snapshot {
                revision: 4,
                ..Default::default()
            },)))
        );
        assert!(
            pending.defer(&UiEvent::Desktop(Arc::new(bead_services::Snapshot {
                revision: 5,
                ..Default::default()
            },)))
        );
        assert!(
            pending.defer(&UiEvent::Backend(Arc::new(bead_backends::Snapshot {
                epoch: 2,
                ..Default::default()
            },)))
        );
        assert!(
            pending.defer(&UiEvent::Backend(Arc::new(bead_backends::Snapshot {
                epoch: 3,
                ..Default::default()
            },)))
        );
        assert_eq!(
            pending.model.get("dashboard.title"),
            Some(&bead_core::Value::Text("new".into())),
        );
        assert_eq!(
            pending.model.get("dashboard.count"),
            Some(&bead_core::Value::Number(1.)),
        );
        assert!(pending.reload);
        assert_eq!(pending.desktop.as_ref().unwrap().revision, 5);
        assert_eq!(pending.backend.as_ref().unwrap().epoch, 3);
        assert!(
            !pending.defer(&UiEvent::Click { x: 3., y: 4. },),
            "input remains immediate during a slide"
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
                assert!(!source.resources_retired());
                assert!(
                    receiver.try_recv().is_err(),
                    "one retirement/retry, no busy loop"
                );
            }
            let (done, completed) = mpsc::channel();
            let dropper = std::thread::spawn(move || {
                drop(source);
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
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
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
            rect: bead_render::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: bead_render::ImageFit::Fill,
            clip_polygon: None,
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
        assert!(source.stage(UiEvent::Reload).unwrap().is_none());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            UiEvent::Fatal(_)
        ));
    }
}
