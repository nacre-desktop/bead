// SPDX-License-Identifier: MIT
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
    /// Wakes the clock worker on lease changes and stop.
    pub clock_wait: (Mutex<()>, std::sync::Condvar),
    pub audio_sender: Mutex<Option<pipewire::channel::Sender<super::audio::Command>>>,
    pub audio_wait: (Mutex<()>, std::sync::Condvar),
}
/// Reconnect pacing for the audio and bus workers: the delay doubles from 400 ms to a
/// 60 s cap, and each distinct failure is returned for logging once instead of per retry.
pub(crate) struct Retry {
    delay: std::time::Duration,
    logged: Option<String>,
}
impl Retry {
    pub const CAP: std::time::Duration = std::time::Duration::from_secs(60);
    pub fn new() -> Self {
        Self {
            delay: std::time::Duration::from_millis(200),
            logged: None,
        }
    }
    /// Delay before the next attempt and the error to log, if it is new. `None` means the
    /// attempt connected, which resets the pacing.
    pub fn next(&mut self, error: Option<String>) -> (std::time::Duration, Option<String>) {
        let log = match error {
            None => {
                *self = Self::new();
                None
            }
            Some(error) if self.logged.as_ref() == Some(&error) => None,
            Some(error) => {
                self.logged = Some(error.clone());
                Some(error)
            }
        };
        self.delay = (self.delay * 2).min(Self::CAP);
        (self.delay, log)
    }
}
impl Shared {
    /// Sleep until the next reconnect attempt or `Desktop::stop`. Checking `stop` under the
    /// lock that `stop` takes before notifying means a long wait cannot miss the wakeup.
    pub fn wait_retry(&self, delay: std::time::Duration) {
        let guard = self.audio_wait.0.lock().unwrap_or_else(poisoned);
        if !self.stop.load(Ordering::SeqCst) {
            let _ = self.audio_wait.1.wait_timeout(guard, delay);
        }
    }
    /// Wake the clock worker to re-read the lease count; taking the lock first means the worker
    /// either sees the new count under its own lock or is already waiting on the condvar.
    pub fn wake_clock(&self) {
        drop(self.clock_wait.0.lock().unwrap_or_else(poisoned));
        self.clock_wait.1.notify_all();
    }
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
    pub tray_watcher: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            audio: true,
            bus: true,
            tray_watcher: false,
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
}
impl Drop for ClockSecondsLease {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.clock_seconds_leases.fetch_sub(1, Ordering::AcqRel);
            shared.wake_clock();
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
            clock_wait: (Mutex::new(()), std::sync::Condvar::new()),
            audio_sender: Mutex::new(None),
            audio_wait: (Mutex::new(()), std::sync::Condvar::new()),
        });
        let (sender, receiver) = mpsc::channel(64);
        let bus_shared = shared.clone();
        let clock_shared = shared.clone();
        let mut workers = vec![std::thread::spawn(move || {
            super::bus::run_clock(clock_shared)
        })];
        if config.bus {
            let tray_watcher = config.tray_watcher;
            workers.push(std::thread::spawn(move || {
                super::bus::run(bus_shared, receiver, tray_watcher)
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
        self.shared.wake_clock();
        ClockSecondsLease {
            shared: Arc::downgrade(&self.shared),
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
        {
            // Under the clock lock so the clock worker cannot miss the wakeup.
            let _guard = self
                .shared
                .clock_wait
                .0
                .lock()
                .unwrap_or_else(crate::runtime::poisoned);
            self.shared.stop.store(true, Ordering::SeqCst);
        }
        self.shared.clock_wait.1.notify_all();
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
        drop(self.shared.audio_wait.0.lock().unwrap_or_else(poisoned));
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
            tray_watcher: false,
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
    use std::time::{Duration, Instant};

    fn wait(desktop: &Desktop, limit: Duration, ready: impl Fn(&Snapshot) -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while !ready(&desktop.snapshot()) {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    #[test]
    fn clock_ticks_without_any_bus_and_stops_promptly() {
        let desktop = Desktop::start_config(Config {
            audio: false,
            bus: false,
            tray_watcher: false,
        });
        assert!(wait(&desktop, Duration::from_secs(5), |s| s.clock.len() == 5));
        let first = desktop.snapshot().clock_seconds.clone();
        assert_eq!(first.len(), 8);
        let lease = desktop.clock_seconds_lease();
        let ticked = wait(&desktop, Duration::from_secs(3), |s| {
            s.clock_seconds != first
        });
        assert!(ticked, "a seconds lease must tick every second");
        drop(lease);
        let started = Instant::now();
        desktop.stop();
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn seconds_clock_leases_share_demand_and_release_to_minute_mode() {
        let desktop = Desktop::start_config(Config {
            audio: false,
            bus: false,
            tray_watcher: false,
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

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn reconnects_back_off_to_a_cap_and_log_each_failure_once() {
        let mut retry = Retry::new();
        let failed = |retry: &mut Retry, error: &str| retry.next(Some(error.into()));
        assert_eq!(
            failed(&mut retry, "Creation failed"),
            (Duration::from_millis(400), Some("Creation failed".into()))
        );
        let delays: Vec<_> = (0..12)
            .map(|_| failed(&mut retry, "Creation failed"))
            .collect();
        assert!(delays.iter().all(|(_, log)| log.is_none()), "{delays:?}");
        assert_eq!(delays[0].0, Duration::from_millis(800));
        assert_eq!(delays.last().unwrap().0, Retry::CAP);
        assert_eq!(failed(&mut retry, "other").1.as_deref(), Some("other"));
        assert_eq!(retry.next(None), (Duration::from_millis(400), None));
        assert_eq!(
            failed(&mut retry, "Creation failed").1.as_deref(),
            Some("Creation failed")
        );
    }

    #[test]
    fn stop_interrupts_a_capped_retry_wait() {
        let desktop = Desktop::start_config(Config {
            audio: false,
            bus: false,
            tray_watcher: false,
        });
        let shared = desktop.shared.clone();
        let waiter = std::thread::spawn(move || {
            let started = Instant::now();
            shared.wait_retry(Retry::CAP);
            started.elapsed()
        });
        std::thread::sleep(Duration::from_millis(100));
        desktop.stop();
        assert!(waiter.join().unwrap() < Duration::from_secs(5));
    }
}
