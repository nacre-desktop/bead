// SPDX-License-Identifier: MIT OR Apache-2.0
//! Script/CPU preparation actor; validated candidates cross the draw thread by value.
use crate::{InputRegion, SceneSource, UiEvent};
use bead_render::Scene;
use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};
pub type Factory = Box<dyn FnOnce() -> Result<Box<dyn SceneSource>, String> + Send>;
#[derive(Default)]
struct Mirror {
    focused_dismiss: bool,
    regions: Vec<InputRegion>,
    surrounding: Option<(String, i32, i32)>,
    sensitive: bool,
    tree: Option<accesskit::TreeUpdate>,
    summary: String,
    resources: String,
}
impl Mirror {
    fn read(source: &dyn SceneSource) -> Self {
        let _profile = bead_core::profile::Span::new("mirror");
        Self {
            focused_dismiss: source.focused_dismiss(),
            regions: source.input_regions(),
            surrounding: source.surrounding_text(),
            sensitive: source.sensitive_input(),
            tree: source.accessibility_tree(),
            summary: source.state_summary(),
            resources: source.resource_counts(),
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
enum Job {
    PanelCenter(Option<f32>),
    Presented(u64),
    Idle(bool),
    Stage(UiEvent, Vec<f32>),
    Viewport(u32, u32),
    Watch(Box<dyn Fn(Result<(), String>) + Send>),
    Ack(u64, Result<(), String>),
    Stop,
}
pub struct Source {
    factory: Option<Factory>,
    sender: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    mirror: Arc<Mirror>,
    scales: Vec<f32>,
    token: u64,
    accepted: bool,
    current: Scene,
    candidate: Option<Scene>,
    viewport: Option<(u32, u32)>,
}
impl Source {
    pub fn new(factory: Factory, fallback: Scene) -> Self {
        Self {
            factory: Some(factory),
            sender: None,
            thread: None,
            mirror: Arc::new(Mirror::default()),
            scales: vec![1.],
            token: 0,
            accepted: false,
            current: fallback,
            candidate: None,
            viewport: None,
        }
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
            scene.prepared.insert(scale.to_bits(), frame);
        }
    }
    Ok(())
}
impl SceneSource for Source {
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
        self.thread = Some(std::thread::spawn(move || {
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
                let service_wakeup = wakeup.clone();
                source.services(Box::new(move |event| service_wakeup(event)));
                let mut cache = bead_render::tree::TreeCache::default();
                let mut queue = VecDeque::new();
                let mut token = 0;
                let mut idle = false;
                let mut needs_trim = false;
                'actor: loop {
                    let job = if let Some(job) = queue.pop_front() {
                        job
                    } else {
                        if idle && needs_trim {
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
                    match job {
                        Job::PanelCenter(center) => source.panel_center(center),
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
                        Job::Viewport(w, h) => {
                            source.viewport(w, h);
                        }
                        Job::Watch(callback) => {
                            if let Err(error) = source.watch(callback) {
                                wakeup(UiEvent::WatchError(error.to_string()));
                            }
                        }
                        Job::Ack(_, _) => {}
                        Job::Stage(event, scales) => {
                            let reload = matches!(event, UiEvent::Reload);
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
                            needs_trim = true;
                            let bake_profile = bead_core::profile::Span::new("bake");
                            if let Err(error) = bake(&mut cache, &mut scene, &scales) {
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
                            println!(
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
                                    Ok(job) => queue.push_back(job),
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
    }
    fn set_idle(&mut self, idle: bool) {
        let _ = self.send(Job::Idle(idle));
    }
    fn panel_center(&mut self, center: Option<f32>) {
        let _ = self.send(Job::PanelCenter(center));
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
        self.send(Job::Stage(event, self.scales.clone()))?;
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
    fn focused_dismiss(&self) -> bool {
        self.mirror.focused_dismiss
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.mirror
            .regions
            .iter()
            .rev()
            .find(|r| {
                x >= r.rect.x
                    && y >= r.rect.y
                    && x < r.rect.x + r.rect.w
                    && y < r.rect.y + r.rect.h
                    && r.clip
                        .is_none_or(|c| x >= c.x && y >= c.y && x < c.x + c.w && y < c.y + c.h)
            })
            .map_or("default", |r| r.cursor)
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
    struct Mock {
        commits: Arc<AtomicUsize>,
        panic: bool,
    }
    impl SceneSource for Mock {
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
            Ok(Some(scene()))
        }
        fn validate(&self) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
        fn state_summary(&self) -> String {
            "accepted".into()
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
    fn worker_requires_gpu_ack_before_store_commit_and_supplies_prebaked_pixels() {
        let commits = Arc::new(AtomicUsize::new(0));
        let count = commits.clone();
        let mut source = Source::new(
            Box::new(move || {
                Ok(Box::new(Mock {
                    commits: count,
                    panic: false,
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
    }
    #[test]
    fn worker_panic_becomes_fatal_event_instead_of_silent_frozen_ui() {
        let mut source = Source::new(
            Box::new(|| {
                Ok(Box::new(Mock {
                    commits: Arc::new(AtomicUsize::new(0)),
                    panic: true,
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
