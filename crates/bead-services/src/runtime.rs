// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Snapshot;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub enum Action {
    Volume(f64),
    Mute(bool),
    TrayActivate {
        item: String,
        x: i32,
        y: i32,
    },
    TraySecondaryActivate {
        item: String,
        x: i32,
        y: i32,
    },
    TrayContextMenu {
        item: String,
        x: i32,
        y: i32,
    },
    TrayScroll {
        item: String,
        delta: i32,
        horizontal: bool,
    },
    TrayMenu {
        item: String,
        entry: i32,
    },
    TrayPrepare {
        item: String,
    },
    TrayPrepareEntry {
        item: String,
        entry: i32,
    },
}
#[derive(Debug)]
pub(crate) enum Command {
    Action(Action, std::sync::mpsc::Sender<Result<(), String>>),
    Tasks(std::sync::mpsc::Sender<(usize, usize)>),
    ClockResolutionChanged,
    Stop,
}
/// Immutable snapshots are published atomically; poisoned read/bookkeeping locks
/// can retain their last intact value, while action locks propagate an error.
pub(crate) fn poisoned<T>(error: std::sync::PoisonError<T>) -> T {
    eprintln!("LOCK_ERROR poisoned snapshot/bookkeeping; retaining last value");
    error.into_inner()
}
type Callback = Arc<dyn Fn(Arc<Snapshot>) + Send + Sync>;
pub(crate) struct Shared {
    pub snapshot: Mutex<Arc<Snapshot>>,
    callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    pub stop: AtomicBool,
    pub clock_seconds_leases: std::sync::atomic::AtomicUsize,
    pub audio_sender: Mutex<Option<pipewire::channel::Sender<super::audio::Command>>>,
    pub audio_wait: (Mutex<()>, std::sync::Condvar),
}
impl Shared {
    pub fn change(&self, mutate: impl FnOnce(&mut Snapshot)) {
        let mut old = self
            .snapshot
            .lock()
            .unwrap_or_else(crate::runtime::poisoned);
        let mut next = (**old).clone();
        mutate(&mut next);
        if **old == next {
            return;
        }
        next.revision = old.revision.saturating_add(1);
        let next = Arc::new(next);
        *old = next.clone();
        drop(old);
        let callbacks: Vec<_> = self
            .callbacks
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .values()
            .cloned()
            .collect();
        for callback in callbacks {
            callback(next.clone());
        }
    }
}
pub struct Subscription {
    shared: Weak<Shared>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared
                .callbacks
                .lock()
                .unwrap_or_else(crate::runtime::poisoned)
                .remove(&self.id);
        }
    }
}
/// Transitional composite service configuration; hosts can disable outside lanes.
#[derive(Clone, Debug)]
pub struct Config {
    pub audio: bool,
    pub bus: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            audio: true,
            bus: true,
        }
    }
}
pub struct Desktop {
    shared: Arc<Shared>,
    sender: mpsc::Sender<Command>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}
/// Keeps the shared desktop clock at second resolution while held.
pub struct ClockSecondsLease {
    shared: Weak<Shared>,
    sender: mpsc::Sender<Command>,
}
impl Drop for ClockSecondsLease {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.clock_seconds_leases.fetch_sub(1, Ordering::AcqRel);
            let _ = self.sender.try_send(Command::ClockResolutionChanged);
        }
    }
}
fn validate_volume(volume: f64) -> Result<(), String> {
    if !volume.is_finite() || !(0.0..=1.5).contains(&volume) {
        return Err("volume must be finite and in 0..1.5".into());
    }
    Ok(())
}
impl Desktop {
    pub fn start() -> Arc<Self> {
        Self::start_config(Config::default())
    }
    pub fn start_config(config: Config) -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(Snapshot::default())),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            clock_seconds_leases: std::sync::atomic::AtomicUsize::new(0),
            audio_sender: Mutex::new(None),
            audio_wait: (Mutex::new(()), std::sync::Condvar::new()),
        });
        let (sender, receiver) = mpsc::channel(64);
        let bus_shared = shared.clone();
        let mut workers = Vec::new();
        if config.bus {
            workers.push(std::thread::spawn(move || {
                super::bus::run(bus_shared, receiver)
            }));
        }
        if config.audio {
            workers.push(super::audio::spawn(shared.clone()));
        }
        Arc::new(Self {
            shared,
            sender,
            workers: Mutex::new(workers),
        })
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.shared
            .snapshot
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .clone()
    }
    /// Request 1 Hz clock updates for as long as the returned guard is held.
    pub fn clock_seconds_lease(&self) -> ClockSecondsLease {
        self.shared
            .clock_seconds_leases
            .fetch_add(1, Ordering::AcqRel);
        let _ = self.sender.try_send(Command::ClockResolutionChanged);
        ClockSecondsLease {
            shared: Arc::downgrade(&self.shared),
            sender: self.sender.clone(),
        }
    }
    pub fn subscribe(
        &self,
        callback: impl Fn(Arc<Snapshot>) + Send + Sync + 'static,
    ) -> Subscription {
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let callback: Callback = Arc::new(callback);
        self.shared
            .callbacks
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .insert(id, callback.clone());
        let snapshot = self.snapshot();
        callback(snapshot);
        Subscription {
            shared: Arc::downgrade(&self.shared),
            id,
        }
    }
    pub fn call(&self, action: Action) -> Result<(), String> {
        self.action(action)
    }
    pub fn action(&self, action: Action) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        match action {
            Action::Volume(volume) => {
                validate_volume(volume)?;
                self.audio(super::audio::Command::Volume(volume, sender))?;
            }
            Action::Mute(mute) => self.audio(super::audio::Command::Mute(mute, sender))?,
            action => self
                .sender
                .try_send(Command::Action(action, sender))
                .map_err(|e| e.to_string())?,
        }
        receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|e| e.to_string())?
    }
    /// Enqueue an absolute level without waiting for the audio worker. Admission
    /// succeeds here; subscriptions report the resulting authoritative state.
    pub fn queue_volume(&self, volume: f64) -> Result<(), String> {
        validate_volume(volume)?;
        let (reply, _receiver) = std::sync::mpsc::channel();
        self.audio(super::audio::Command::Volume(volume, reply))
    }
    fn audio(&self, command: super::audio::Command) -> Result<(), String> {
        self.shared
            .audio_sender
            .lock()
            .map_err(|_| "audio sender lock poisoned")?
            .as_ref()
            .ok_or("PipeWire unavailable")?
            .send(command)
            .map_err(|_| "PipeWire disconnected".to_string())
    }
    pub fn bus_task_count(&self) -> usize {
        self.bus_task_counts().0
    }
    pub fn bus_task_counts(&self) -> (usize, usize) {
        let (sender, receiver) = std::sync::mpsc::channel();
        if self.sender.try_send(Command::Tasks(sender)).is_err() {
            return (0, 0);
        }
        receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap_or_default()
    }
    pub fn subscription_count(&self) -> usize {
        self.shared
            .callbacks
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .len()
    }
    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = self.sender.try_send(Command::Stop);
        if let Some(sender) = self
            .shared
            .audio_sender
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .take()
        {
            let _ = sender.send(super::audio::Command::Stop);
        }
        self.shared.audio_wait.1.notify_all();
        for worker in self
            .workers
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .drain(..)
        {
            let _ = worker.join();
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn block_worker_signals() {
    // SAFETY: initialized local signal mask, no retained pointers.
    unsafe {
        let mut mask = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        libc::sigemptyset(mask.as_mut_ptr());
        let mut mask = mask.assume_init();
        libc::sigaddset(&mut mask, libc::SIGINT);
        libc::sigaddset(&mut mask, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
}

#[cfg(test)]
mod volume_queue_tests {
    use super::*;
    #[test]
    fn queued_volume_does_not_wait_for_a_worker_reply_and_rejects_invalid_levels() {
        let desktop = Desktop::start_config(Config {
            audio: false,
            bus: false,
        });
        let (sender, _receiver) = pipewire::channel::channel();
        *desktop.shared.audio_sender.lock().unwrap() = Some(sender);
        let start = std::time::Instant::now();
        desktop.queue_volume(0.6).unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "must not wait for the three-second synchronous timeout"
        );
        for bad in [f64::NAN, f64::INFINITY, -0.1, 1.6] {
            assert!(desktop.queue_volume(bad).is_err());
        }
    }
}

#[cfg(test)]
mod clock_lease_tests {
    use super::*;

    #[test]
    fn seconds_clock_leases_share_demand_and_release_to_minute_mode() {
        let desktop = Desktop::start_config(Config {
            audio: false,
            bus: false,
        });
        assert_eq!(
            desktop.shared.clock_seconds_leases.load(Ordering::Acquire),
            0
        );
        let first = desktop.clock_seconds_lease();
        assert_eq!(
            desktop.shared.clock_seconds_leases.load(Ordering::Acquire),
            1
        );
        let second = desktop.clock_seconds_lease();
        assert_eq!(
            desktop.shared.clock_seconds_leases.load(Ordering::Acquire),
            2
        );
        drop(first);
        assert_eq!(
            desktop.shared.clock_seconds_leases.load(Ordering::Acquire),
            1
        );
        drop(second);
        assert_eq!(
            desktop.shared.clock_seconds_leases.load(Ordering::Acquire),
            0
        );
    }
}
