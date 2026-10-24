// SPDX-License-Identifier: MIT OR Apache-2.0
//! B0090: leased, rate-limited toplevel capture (ext-image-copy-capture-v1 with
//! ext-foreign-toplevel-image-capture-source-v1). A product holds a [`CaptureHub`], leases
//! windows on it while a view is open, and reads small PNG thumbnails back; the protocol
//! objects live only on the Wayland thread and exist only while a lease does.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1 as handle, ext_foreign_toplevel_list_v1 as list,
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1 as sources,
        ext_image_capture_source_v1 as source,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1 as frame, ext_image_copy_capture_manager_v1 as copy,
        ext_image_copy_capture_session_v1 as session,
    },
};
const TICK: Duration = Duration::from_millis(50);
const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
pub type Wake = Arc<dyn Fn() + Send + Sync>;
/// A compositor window as the product knows it. The foreign-toplevel protocol carries no
/// compositor window id, so a window is paired with a toplevel by exact app id and title;
/// an ambiguous pair is not captured (the product keeps its fallback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub key: String,
    pub app_id: String,
    pub title: String,
}
/// The newest thumbnail of one window; the file is replaced (new name) per frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}
struct Entry {
    windows: Vec<Window>,
    interval: Duration,
    bound: (u32, u32),
    wake: Wake,
}
#[derive(Default)]
struct Shared {
    supported: bool,
    next: u64,
    leases: BTreeMap<u64, Entry>,
    frames: BTreeMap<String, (Preview, u64)>,
    last: BTreeMap<String, Instant>,
    ping: Option<calloop::ping::Ping>,
    sequence: u64,
}
pub struct CaptureHub {
    dir: PathBuf,
    shared: Mutex<Shared>,
}
/// Dropping a lease stops capturing for it and removes thumbnails no other lease wants.
pub struct Lease {
    hub: Arc<CaptureHub>,
    id: u64,
}
impl CaptureHub {
    /// `dir` is private runtime storage; thumbnails are written there only while leased.
    pub fn new(dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            dir,
            shared: Mutex::new(Shared::default()),
        })
    }
    /// Live leases (a closed view holds none).
    pub fn lease_count(&self) -> usize {
        self.lock().leases.len()
    }
    /// Products' tests: a hub that reports support and accepts hand-made thumbnails.
    #[doc(hidden)]
    pub fn test_hub(dir: PathBuf) -> Arc<Self> {
        let hub = Self::new(dir);
        hub.lock().supported = true;
        hub
    }
    #[doc(hidden)]
    pub fn publish_for_test(&self, key: &str, preview: Preview) {
        self.lock().frames.insert(key.into(), (preview, 0));
    }
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Whether the compositor advertises every protocol the capture needs.
    pub fn supported(&self) -> bool {
        self.lock().supported
    }
    /// Lease `windows` at no more than one frame per `interval` each, thumbnails fitting
    /// `bound` pixels. `wake` runs (from the Wayland thread) after a frame lands.
    pub fn lease(
        self: &Arc<Self>,
        windows: Vec<Window>,
        interval: Duration,
        bound: (u32, u32),
        wake: Wake,
    ) -> Lease {
        let mut s = self.lock();
        s.next += 1;
        let id = s.next;
        s.leases.insert(
            id,
            Entry {
                windows,
                interval,
                bound,
                wake,
            },
        );
        let ping = s.ping.clone();
        drop(s);
        if let Some(ping) = ping {
            ping.ping();
        }
        Lease {
            hub: self.clone(),
            id,
        }
    }
    /// The newest thumbnail for `key`, if some lease wants it and one has landed.
    pub fn preview(&self, key: &str) -> Option<Preview> {
        self.lock().frames.get(key).map(|(p, _)| p.clone())
    }
    fn wanted(s: &Shared) -> BTreeSet<&str> {
        s.leases
            .values()
            .flat_map(|e| e.windows.iter().map(|w| w.key.as_str()))
            .collect()
    }
    /// Forget thumbnails (and their files) that no lease wants any more.
    fn prune(&self, s: &mut Shared) {
        let wanted: BTreeSet<String> = Self::wanted(s).into_iter().map(str::to_owned).collect();
        s.frames.retain(|key, (preview, _)| {
            let keep = wanted.contains(key);
            if !keep {
                let _ = std::fs::remove_file(&preview.path);
            }
            keep
        });
        s.last.retain(|key, _| wanted.contains(key));
        if s.leases.is_empty() {
            let _ = std::fs::remove_dir(&self.dir);
        }
    }
    /// The window to capture next: the most overdue one wanted at `now`.
    fn next_due(
        s: &Shared,
        now: Instant,
        ready: &dyn Fn(&str) -> bool,
    ) -> Option<(Window, (u32, u32))> {
        let mut best: Option<(Duration, Window, (u32, u32))> = None;
        for e in s.leases.values() {
            for w in e.windows.iter().filter(|w| ready(&w.key)) {
                let overdue = match s.last.get(&w.key) {
                    None => Duration::MAX,
                    Some(t) => match now.saturating_duration_since(*t).checked_sub(e.interval) {
                        Some(d) => d,
                        None => continue,
                    },
                };
                if best.as_ref().is_none_or(|(b, _, _)| overdue > *b) {
                    best = Some((overdue, w.clone(), e.bound));
                }
            }
        }
        best.map(|(_, w, b)| (w, b))
    }
}
impl Lease {
    /// Replace the leased windows (the product re-lists them as the compositor changes).
    pub fn set_windows(&self, windows: Vec<Window>) {
        let mut s = self.hub.lock();
        if let Some(e) = s.leases.get_mut(&self.id)
            && e.windows != windows
        {
            e.windows = windows;
            self.hub.prune(&mut s);
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut s = self.hub.lock();
        s.leases.remove(&self.id);
        self.hub.prune(&mut s);
        let ping = s.ping.clone();
        drop(s);
        if let Some(ping) = ping {
            ping.ping();
        }
    }
}
struct Toplevel {
    handle: handle::ExtForeignToplevelHandleV1,
    app_id: String,
    title: String,
    pending: (Option<String>, Option<String>),
    id: u64,
}
struct Live {
    id: u64,
    key: String,
    toplevel: u64,
    session: session::ExtImageCopyCaptureSessionV1,
    source: source::ExtImageCaptureSourceV1,
    size: Option<(u32, u32)>,
    format: Option<wl_shm::Format>,
    constrained: bool,
}
struct Flight {
    live: u64,
    frame: frame::ExtImageCopyCaptureFrameV1,
    buffer: smithay_client_toolkit::shm::slot::Buffer,
    bound: (u32, u32),
    format: wl_shm::Format,
    size: (u32, u32),
}
/// Wayland-thread half: bound globals, sessions and the one frame in flight.
pub(super) struct Capturer {
    hub: Arc<CaptureHub>,
    copy: Option<copy::ExtImageCopyCaptureManagerV1>,
    sources: Option<sources::ExtForeignToplevelImageCaptureSourceManagerV1>,
    list: Option<list::ExtForeignToplevelListV1>,
    toplevels: Vec<Toplevel>,
    lives: Vec<Live>,
    flight: Option<Flight>,
    pool: Option<smithay_client_toolkit::shm::slot::SlotPool>,
    timer: Option<calloop::RegistrationToken>,
    serial: u64,
}
impl Capturer {
    pub(super) fn new(
        hub: Arc<CaptureHub>,
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<App>,
    ) -> Self {
        let copy = globals.bind(qh, 1..=1, ()).ok();
        let sources = globals.bind(qh, 1..=1, ()).ok();
        let has_list = globals.contents().with_list(|l| {
            l.iter()
                .any(|g| g.interface == "ext_foreign_toplevel_list_v1")
        });
        hub.lock().supported = copy.is_some() && sources.is_some() && has_list;
        Self {
            hub,
            copy,
            sources,
            list: None,
            toplevels: vec![],
            lives: vec![],
            flight: None,
            pool: None,
            timer: None,
            serial: 0,
        }
    }
    fn next_id(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }
    /// Drop every protocol object and buffer; the next lease starts from nothing.
    fn teardown(&mut self) {
        if let Some(f) = self.flight.take() {
            f.frame.destroy();
        }
        for l in self.lives.drain(..) {
            l.session.destroy();
            l.source.destroy();
        }
        if let Some(list) = self.list.take() {
            list.stop();
            list.destroy();
        }
        for t in self.toplevels.drain(..) {
            t.handle.destroy();
        }
        self.pool = None;
    }
}
impl App {
    /// Install the lease ping; the tick timer then runs only while leases exist.
    pub(super) fn capture_init(&mut self, hub: &Arc<CaptureHub>) {
        let Ok((ping, source)) = calloop::ping::make_ping() else {
            return;
        };
        hub.lock().ping = Some(ping);
        let _ = self
            .loop_handle
            .insert_source(source, |_, _, app| app.capture_sync());
    }
    fn capture_sync(&mut self) {
        let Some(c) = &mut self.capture else { return };
        let active = !c.hub.lock().leases.is_empty();
        if active && c.timer.is_none() && c.hub.supported() {
            if c.list.is_none() {
                c.list = self.registry.bind_one(&self.qh, 1..=1, ()).ok();
            }
            c.timer = self
                .loop_handle
                .insert_source(calloop::timer::Timer::from_duration(TICK), |_, _, app| {
                    app.capture_tick()
                })
                .ok();
        } else if !active && c.timer.is_some() {
            if let Some(token) = c.timer.take() {
                self.loop_handle.remove(token);
            }
            c.teardown();
        }
    }
    fn capture_tick(&mut self) -> calloop::timer::TimeoutAction {
        let qh = self.qh.clone();
        let Some(c) = &mut self.capture else {
            return calloop::timer::TimeoutAction::Drop;
        };
        if c.hub.lock().leases.is_empty() {
            c.timer = None;
            c.teardown();
            return calloop::timer::TimeoutAction::Drop;
        }
        c.reconcile(&qh);
        if c.flight.is_none() {
            let shm = &self.shm;
            c.start(shm, &qh);
        }
        calloop::timer::TimeoutAction::ToDuration(TICK)
    }
}
impl Capturer {
    fn toplevel_of(&self, w: &Window) -> Option<u64> {
        unique_match(
            self.toplevels
                .iter()
                .map(|t| (t.id, t.app_id.as_str(), t.title.as_str())),
            w,
        )
    }
    /// Pair wanted windows with toplevels; open sessions for new pairs, close stale ones.
    fn reconcile(&mut self, qh: &QueueHandle<App>) {
        let windows: Vec<Window> = {
            let s = self.hub.lock();
            s.leases.values().flat_map(|e| e.windows.clone()).collect()
        };
        let pairs: BTreeMap<String, u64> = windows
            .iter()
            .filter_map(|w| Some((w.key.clone(), self.toplevel_of(w)?)))
            .collect();
        let stale: Vec<u64> = self
            .lives
            .iter()
            .filter(|l| pairs.get(&l.key) != Some(&l.toplevel))
            .map(|l| l.id)
            .collect();
        for id in stale {
            self.drop_live(id);
        }
        let (Some(copy), Some(sources)) = (self.copy.clone(), self.sources.clone()) else {
            return;
        };
        for (key, toplevel) in pairs {
            if self.lives.iter().any(|l| l.key == key) {
                continue;
            }
            let Some(handle) = self
                .toplevels
                .iter()
                .find(|t| t.id == toplevel)
                .map(|t| t.handle.clone())
            else {
                continue;
            };
            let id = self.next_id();
            let src = sources.create_source(&handle, qh, ());
            let session = copy.create_session(&src, copy::Options::empty(), qh, id);
            self.lives.push(Live {
                id,
                key,
                toplevel,
                session,
                source: src,
                size: None,
                format: None,
                constrained: false,
            });
        }
    }
    fn drop_live(&mut self, id: u64) {
        if self.flight.as_ref().is_some_and(|f| f.live == id)
            && let Some(f) = self.flight.take()
        {
            f.frame.destroy();
        }
        if let Some(i) = self.lives.iter().position(|l| l.id == id) {
            let l = self.lives.remove(i);
            l.session.destroy();
            l.source.destroy();
        }
    }
    fn start(&mut self, shm: &smithay_client_toolkit::shm::Shm, qh: &QueueHandle<App>) {
        let now = Instant::now();
        let candidate = {
            let s = self.hub.lock();
            // Only windows with a ready session compete for the single frame slot.
            let lives = &self.lives;
            CaptureHub::next_due(&s, now, &|key| {
                lives.iter().any(|l| {
                    l.key == key && l.size.is_some() && l.format.is_some() && !l.constrained
                })
            })
        };
        let Some((window, bound)) = candidate else {
            return;
        };
        self.hub.lock().last.insert(window.key.clone(), now);
        let Some(l) = self.lives.iter().find(|l| l.key == window.key) else {
            return;
        };
        let (Some((w, h)), Some(format)) = (l.size, l.format) else {
            return;
        };
        if u64::from(w) * u64::from(h) > MAX_SOURCE_PIXELS || w == 0 || h == 0 {
            return;
        }
        let pool = match &mut self.pool {
            Some(p) => p,
            None => {
                match smithay_client_toolkit::shm::slot::SlotPool::new((w * h * 4) as usize, shm) {
                    Ok(p) => self.pool.insert(p),
                    Err(_) => return,
                }
            }
        };
        let Ok((buffer, _)) = pool.create_buffer(w as i32, h as i32, (w * 4) as i32, format) else {
            return;
        };
        let live = l.id;
        let frame = l.session.create_frame(qh, live);
        frame.attach_buffer(buffer.wl_buffer());
        frame.damage_buffer(0, 0, w as i32, h as i32);
        frame.capture();
        self.flight = Some(Flight {
            live,
            frame,
            buffer,
            bound,
            format,
            size: (w, h),
        });
    }
    fn finish(&mut self, live: u64, ok: bool) {
        let Some(f) = self.flight.take().filter(|f| f.live == live) else {
            return;
        };
        f.frame.destroy();
        let Some(key) = self
            .lives
            .iter()
            .find(|l| l.id == live)
            .map(|l| l.key.clone())
        else {
            return;
        };
        if !ok {
            return;
        }
        let Some(pool) = &mut self.pool else { return };
        let Some(canvas) = f.buffer.canvas(pool) else {
            return;
        };
        let Some((w, h, rgba)) = thumbnail(canvas, f.size, f.format, f.bound) else {
            return;
        };
        let hub = self.hub.clone();
        let mut s = hub.lock();
        if !CaptureHub::wanted(&s).contains(key.as_str()) {
            return;
        }
        s.sequence += 1;
        let path = hub.dir.join(format!("w{}-{}.png", live, s.sequence));
        if std::fs::create_dir_all(&hub.dir).is_err() || write_png(&path, w, h, &rgba).is_err() {
            return;
        }
        if let Some((old, _)) = s.frames.insert(
            key.clone(),
            (
                Preview {
                    path,
                    width: w,
                    height: h,
                },
                live,
            ),
        ) {
            let _ = std::fs::remove_file(old.path);
        }
        let wakes: Vec<Wake> = s
            .leases
            .values()
            .filter(|e| e.windows.iter().any(|w| w.key == key))
            .map(|e| e.wake.clone())
            .collect();
        drop(s);
        for wake in wakes {
            wake();
        }
    }
}
/// The one toplevel with exactly this app id and title; none when absent or ambiguous.
fn unique_match<'a>(
    mut tops: impl Iterator<Item = (u64, &'a str, &'a str)>,
    w: &Window,
) -> Option<u64> {
    let mut found = tops.by_ref().filter(|t| t.1 == w.app_id && t.2 == w.title);
    let one = found.next()?.0;
    found.next().is_none().then_some(one)
}
/// Contain-fit box downscale of an shm frame to RGBA no larger than `bound`.
fn thumbnail(
    src: &[u8],
    (sw, sh): (u32, u32),
    format: wl_shm::Format,
    (bw, bh): (u32, u32),
) -> Option<(u32, u32, Vec<u8>)> {
    let bgr = match format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => true,
        wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => false,
        _ => return None,
    };
    if src.len() < (sw as usize) * (sh as usize) * 4 || bw == 0 || bh == 0 {
        return None;
    }
    let scale = (bw as f32 / sw as f32).min(bh as f32 / sh as f32).min(1.0);
    let (dw, dh) = (
        ((sw as f32 * scale).round() as u32).max(1),
        ((sh as f32 * scale).round() as u32).max(1),
    );
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let (y0, y1) = (
            y * sh / dh,
            ((y + 1) * sh / dh).max(y * sh / dh + 1).min(sh),
        );
        let ystep = ((y1 - y0) / 3).max(1);
        for x in 0..dw {
            let (x0, x1) = (
                x * sw / dw,
                ((x + 1) * sw / dw).max(x * sw / dw + 1).min(sw),
            );
            let xstep = ((x1 - x0) / 3).max(1);
            let (mut acc, mut n) = ([0u32; 3], 0u32);
            let mut sy = y0;
            while sy < y1 {
                let mut sx = x0;
                while sx < x1 {
                    let i = ((sy * sw + sx) * 4) as usize;
                    acc[0] += u32::from(src[i + usize::from(bgr) * 2]);
                    acc[1] += u32::from(src[i + 1]);
                    acc[2] += u32::from(src[i + usize::from(!bgr) * 2]);
                    n += 1;
                    sx += xstep;
                }
                sy += ystep;
            }
            let o = ((y * dw + x) * 4) as usize;
            out[o] = (acc[0] / n) as u8;
            out[o + 1] = (acc[1] / n) as u8;
            out[o + 2] = (acc[2] / n) as u8;
            out[o + 3] = 255;
        }
    }
    Some((dw, dh, out))
}
fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    use image::ImageEncoder;
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    )
    .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
    .map_err(|e| e.to_string())
}
impl Dispatch<copy::ExtImageCopyCaptureManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &copy::ExtImageCopyCaptureManagerV1,
        _: copy::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<sources::ExtForeignToplevelImageCaptureSourceManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &sources::ExtForeignToplevelImageCaptureSourceManagerV1,
        _: sources::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<source::ExtImageCaptureSourceV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &source::ExtImageCaptureSourceV1,
        _: source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<list::ExtForeignToplevelListV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &list::ExtForeignToplevelListV1,
        event: list::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let (list::Event::Toplevel { toplevel }, Some(c)) = (event, &mut state.capture) {
            let id = c.next_id();
            c.toplevels.push(Toplevel {
                handle: toplevel,
                app_id: String::new(),
                title: String::new(),
                pending: (None, None),
                id,
            });
        }
    }
    wayland_client::event_created_child!(App, list::ExtForeignToplevelListV1, [
        list::EVT_TOPLEVEL_OPCODE => (handle::ExtForeignToplevelHandleV1, ())
    ]);
}
impl Dispatch<handle::ExtForeignToplevelHandleV1, ()> for App {
    fn event(
        state: &mut Self,
        proxy: &handle::ExtForeignToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(c) = &mut state.capture else { return };
        let Some(i) = c.toplevels.iter().position(|t| &t.handle == proxy) else {
            return;
        };
        match event {
            handle::Event::Title { title } => c.toplevels[i].pending.0 = Some(title),
            handle::Event::AppId { app_id } => c.toplevels[i].pending.1 = Some(app_id),
            handle::Event::Done => {
                let t = &mut c.toplevels[i];
                if let Some(title) = t.pending.0.take() {
                    t.title = title;
                }
                if let Some(app_id) = t.pending.1.take() {
                    t.app_id = app_id;
                }
            }
            handle::Event::Closed => {
                let t = c.toplevels.remove(i);
                t.handle.destroy();
            }
            _ => {}
        }
    }
}
impl Dispatch<session::ExtImageCopyCaptureSessionV1, u64> for App {
    fn event(
        state: &mut Self,
        _: &session::ExtImageCopyCaptureSessionV1,
        event: session::Event,
        id: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(c) = &mut state.capture else { return };
        let Some(l) = c.lives.iter_mut().find(|l| l.id == *id) else {
            return;
        };
        match event {
            session::Event::BufferSize { width, height } => {
                l.size = Some((width, height));
                l.constrained = true;
            }
            session::Event::ShmFormat {
                format: wayland_client::WEnum::Value(f),
            } if l.format.is_none()
                && matches!(
                    f,
                    wl_shm::Format::Argb8888
                        | wl_shm::Format::Xrgb8888
                        | wl_shm::Format::Abgr8888
                        | wl_shm::Format::Xbgr8888
                ) =>
            {
                l.format = Some(f);
            }
            session::Event::Done => l.constrained = false,
            session::Event::Stopped => {
                let id = *id;
                c.drop_live(id);
            }
            _ => {}
        }
    }
}
impl Dispatch<frame::ExtImageCopyCaptureFrameV1, u64> for App {
    fn event(
        state: &mut Self,
        _: &frame::ExtImageCopyCaptureFrameV1,
        event: frame::Event,
        id: &u64,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(c) = &mut state.capture else { return };
        match event {
            frame::Event::Ready => c.finish(*id, true),
            frame::Event::Failed { reason } => {
                if let Some(l) = c.lives.iter_mut().find(|l| l.id == *id)
                    && matches!(
                        reason,
                        wayland_client::WEnum::Value(frame::FailureReason::BufferConstraints)
                    )
                {
                    l.constrained = true;
                }
                c.finish(*id, false);
            }
            _ => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hub() -> (Arc<CaptureHub>, PathBuf) {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bead-capture-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        (CaptureHub::new(dir.clone()), dir)
    }
    fn win(key: &str) -> Window {
        Window {
            key: key.into(),
            app_id: "foot".into(),
            title: key.into(),
        }
    }
    fn lease(hub: &Arc<CaptureHub>, keys: &[&str]) -> Lease {
        hub.lease(
            keys.iter().map(|k| win(k)).collect(),
            Duration::from_millis(350),
            (320, 200),
            Arc::new(|| {}),
        )
    }
    fn land(hub: &CaptureHub, dir: &std::path::Path, key: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let mut s = hub.lock();
        s.sequence += 1;
        let path = dir.join(format!("{key}-{}.png", s.sequence));
        std::fs::write(&path, b"png").unwrap();
        s.frames.insert(
            key.into(),
            (
                Preview {
                    path: path.clone(),
                    width: 1,
                    height: 1,
                },
                1,
            ),
        );
        path
    }
    #[test]
    fn release_removes_thumbnails_and_the_directory() {
        let (hub, dir) = hub();
        let a = lease(&hub, &["a", "b"]);
        let path = land(&hub, &dir, "a");
        assert!(hub.preview("a").is_some() && path.exists());
        drop(a);
        assert!(hub.preview("a").is_none());
        assert!(!path.exists() && !dir.exists());
    }
    #[test]
    fn a_shared_window_survives_one_release_and_unwanted_ones_do_not() {
        let (hub, dir) = hub();
        let a = lease(&hub, &["x", "y"]);
        let b = lease(&hub, &["y"]);
        let (x, y) = (land(&hub, &dir, "x"), land(&hub, &dir, "y"));
        drop(a);
        assert!(!x.exists() && y.exists());
        assert!(hub.preview("y").is_some());
        b.set_windows(vec![win("z")]);
        assert!(!y.exists() && hub.preview("y").is_none());
        drop(b);
        assert!(!dir.exists());
    }
    #[test]
    fn each_window_is_rate_limited_and_the_most_overdue_goes_first() {
        let (hub, _) = hub();
        let _l = lease(&hub, &["a", "b"]);
        let t0 = Instant::now();
        let mut s = hub.lock();
        // Never captured: due at once.
        assert!(CaptureHub::next_due(&s, t0, &|_| true).is_some());
        s.last.insert("a".into(), t0);
        s.last.insert("b".into(), t0 + Duration::from_millis(100));
        assert!(CaptureHub::next_due(&s, t0 + Duration::from_millis(349), &|_| true).is_none());
        let due = CaptureHub::next_due(&s, t0 + Duration::from_millis(360), &|_| true).unwrap();
        assert_eq!(due.0.key, "a", "b is still inside its 350 ms interval");
        let due = CaptureHub::next_due(&s, t0 + Duration::from_millis(900), &|_| true).unwrap();
        assert_eq!(due.0.key, "a", "a is the most overdue");
        assert_eq!(due.1, (320, 200));
        let only_b =
            CaptureHub::next_due(&s, t0 + Duration::from_millis(900), &|k| k == "b").unwrap();
        assert_eq!(
            only_b.0.key, "b",
            "unready windows do not take the frame slot"
        );
    }
    #[test]
    fn thumbnails_fit_the_bound_keep_aspect_and_swap_channels() {
        let (w, h) = (640u32, 400u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for p in px.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[10, 20, 30, 255]); // B, G, R, A in Argb8888
        }
        let (tw, th, out) = thumbnail(&px, (w, h), wl_shm::Format::Argb8888, (160, 160)).unwrap();
        assert_eq!((tw, th), (160, 100));
        assert_eq!(&out[..4], &[30, 20, 10, 255]);
        let (tw, th, _) = thumbnail(&px, (w, h), wl_shm::Format::Xbgr8888, (2000, 2000)).unwrap();
        assert_eq!((tw, th), (640, 400), "never upscaled");
        assert!(thumbnail(&px, (w, h), wl_shm::Format::Rgb565, (160, 160)).is_none());
    }
    #[test]
    fn pairing_needs_a_unique_app_id_and_title() {
        let tops = [(1, "foot", "one"), (2, "foot", "two"), (3, "foot", "two")];
        let m = |app: &str, title: &str| {
            unique_match(
                tops.iter().copied(),
                &Window {
                    key: "k".into(),
                    app_id: app.into(),
                    title: title.into(),
                },
            )
        };
        assert_eq!(m("foot", "one"), Some(1));
        assert_eq!(
            (m("foot", "two"), m("foot", "none"), m("kitty", "one")),
            (None, None, None)
        );
    }
}
