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
    TrayActivate { item: String, x: i32, y: i32 },
    TrayMenu { item: String, entry: i32 },
    TrayPrepare { item: String },
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
pub struct Desktop {
    shared: Arc<Shared>,
    sender: mpsc::Sender<Command>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}
impl Desktop {
    pub fn start() -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(Snapshot::default())),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            audio_sender: Mutex::new(None),
            audio_wait: (Mutex::new(()), std::sync::Condvar::new()),
        });
        let (sender, receiver) = mpsc::channel(64);
        let bus_shared = shared.clone();
        let bus = std::thread::spawn(move || super::bus::run(bus_shared, receiver));
        let audio = super::audio::spawn(shared.clone());
        Arc::new(Self {
            shared,
            sender,
            workers: Mutex::new(vec![bus, audio]),
        })
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.shared
            .snapshot
            .lock()
            .unwrap_or_else(crate::runtime::poisoned)
            .clone()
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
    pub fn action(&self, action: Action) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        match action {
            Action::Volume(volume) => {
                if !volume.is_finite() || !(0.0..=1.5).contains(&volume) {
                    return Err("volume must be finite and in 0..1.5".into());
                }
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
}
impl Drop for Desktop {
    fn drop(&mut self) {
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
