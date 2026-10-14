// SPDX-License-Identifier: GPL-3.0-only
//! Self-contained native adapters. Share these with Nacre compositor Wave 2.
pub mod hyprland;
pub mod niri;
pub mod probe;
use bead_backends::{Action, Backend, Error, Event, Snapshot, Subscription};
use std::{
    collections::BTreeMap,
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
};
type Callback = Arc<dyn Fn(Event) + Send + Sync>;
pub(crate) struct Shared {
    pub snapshot: Mutex<Arc<Snapshot>>,
    pub callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    pub stop: AtomicBool,
    pub event_socket: Mutex<Option<UnixStream>>,
}
impl Shared {
    pub fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Mutex::new(Arc::new(Snapshot {
                backend: name.into(),
                capabilities: bead_backends::Capabilities::empty(),
                ..Snapshot::default()
            })),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            event_socket: Mutex::new(None),
        })
    }
    pub fn emit(&self, event: Event) {
        let callbacks: Vec<_> = self.callbacks.lock().unwrap().values().cloned().collect();
        for callback in callbacks {
            callback(event.clone());
        }
    }
    pub fn publish(&self, snapshot: Snapshot) {
        let snapshot = Arc::new(snapshot);
        let mut old = self.snapshot.lock().unwrap();
        if **old == *snapshot {
            return;
        }
        *old = snapshot.clone();
        drop(old);
        trace("normalized_state");
        self.emit(Event::Snapshot(snapshot));
    }
    pub fn disconnected(&self, reason: String) {
        let mut snapshot = (**self.snapshot.lock().unwrap()).clone();
        snapshot.connected = false;
        self.publish(snapshot);
        self.emit(Event::Disconnected(reason));
    }
    pub fn subscribe(self: &Arc<Self>, callback: Box<dyn Fn(Event) + Send + Sync>) -> Subscription {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let callback: Callback = Arc::from(callback);
        self.callbacks.lock().unwrap().insert(id, callback.clone());
        let snapshot = self.snapshot.lock().unwrap().clone();
        callback(Event::Snapshot(snapshot));
        let shared = Arc::downgrade(self);
        Subscription::new(move || {
            if let Some(shared) = shared.upgrade() {
                shared.callbacks.lock().unwrap().remove(&id);
            }
        })
    }
}
pub(crate) fn clock_ns(clock: nix::libc::clockid_t) -> u64 {
    let mut ts = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: initialized writable timespec for a valid OS clock identifier.
    unsafe {
        nix::libc::clock_gettime(clock, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
pub(crate) fn trace(stage: &str) {
    if std::env::var_os("BEAD_TRACE_BACKEND").is_some() {
        println!(
            "BACKEND_TRACE {}",
            serde_json::json!({"stage":stage,"monotonic_ns":clock_ns(nix::libc::CLOCK_MONOTONIC),"thread_cpu_ns":clock_ns(nix::libc::CLOCK_THREAD_CPUTIME_ID),"thread":unsafe { nix::libc::pthread_self() }})
        );
    }
}
pub(crate) fn record(kind: &str, data: serde_json::Value) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    if let Some(path) = std::env::var_os("BEAD_RECORD_IPC") {
        use std::io::Write;
        let _guard = LOCK.lock().unwrap();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let mut bytes =
                serde_json::to_vec(&serde_json::json!({"kind":kind,"data":data})).unwrap();
            bytes.push(b'\n');
            let _ = file.write_all(&bytes);
        }
    }
}
pub(crate) fn worker_signals() {
    let mut signals = nix::sys::signal::SigSet::empty();
    signals.add(nix::sys::signal::Signal::SIGTERM);
    signals.add(nix::sys::signal::Signal::SIGINT);
    let _ = nix::sys::signal::pthread_sigmask(
        nix::sys::signal::SigmaskHow::SIG_BLOCK,
        Some(&signals),
        None,
    );
}
pub(crate) fn epoch(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|m| m.ino() ^ (m.ctime() as u64).rotate_left(17) ^ (m.ctime_nsec() as u64))
        .unwrap_or(0)
}
pub(crate) fn protocol(error: impl ToString) -> Error {
    Error::Protocol(error.to_string())
}
pub struct Native {
    kind: Kind,
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
enum Kind {
    Hyprland(hyprland::Transport),
    Niri(niri::Transport),
}
impl Native {
    pub fn hyprland(directory: PathBuf) -> Arc<Self> {
        let shared = Shared::new("hyprland");
        let transport = hyprland::Transport::new(directory);
        let worker = hyprland::spawn(transport.clone(), shared.clone());
        Arc::new(Self {
            kind: Kind::Hyprland(transport),
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn niri(path: PathBuf) -> Arc<Self> {
        let shared = Shared::new("niri");
        let transport = niri::Transport::new(path);
        let worker = niri::spawn(transport.clone(), shared.clone());
        Arc::new(Self {
            kind: Kind::Niri(transport),
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn from_env() -> Result<Arc<Self>, Error> {
        if let Some(path) = std::env::var_os("NIRI_SOCKET") {
            return Ok(Self::niri(path.into()));
        }
        if let Some(signature) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") {
            let runtime = std::env::var_os("XDG_RUNTIME_DIR")
                .ok_or_else(|| Error::Disconnected("XDG_RUNTIME_DIR absent".into()))?;
            return Ok(Self::hyprland(
                PathBuf::from(runtime).join("hypr").join(signature),
            ));
        }
        Err(Error::Disconnected(
            "no compositor IPC socket in this environment".into(),
        ))
    }
}
impl Backend for Native {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.shared.snapshot.lock().unwrap().clone()
    }
    fn action(&self, action: Action) -> Result<(), Error> {
        let snapshot = self.snapshot();
        if !snapshot.connected {
            return Err(Error::Disconnected(snapshot.backend.clone()));
        }
        match &self.kind {
            Kind::Hyprland(transport) => transport.action(&snapshot, action),
            Kind::Niri(transport) => transport.action(&snapshot, action),
        }
    }
    fn subscribe(&self, callback: Box<dyn Fn(Event) + Send + Sync>) -> Subscription {
        self.shared.subscribe(callback)
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(socket) = self.shared.event_socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;
