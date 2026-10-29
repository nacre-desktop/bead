// SPDX-License-Identifier: MIT OR Apache-2.0
//! One notification thread per identical bundle/palette, with scoped subscribers.
use notify::Watcher;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
type Callback = Box<dyn Fn(Result<(), String>) + Send>;
type Subscribers = Arc<Mutex<BTreeMap<u64, Arc<Mutex<Callback>>>>>;
type Key = (PathBuf, PathBuf, Vec<PathBuf>);
type Registry = Mutex<BTreeMap<Key, Weak<Bus>>>;
struct Bus {
    _watcher: Mutex<notify::RecommendedWatcher>,
    subscribers: Subscribers,
    next: AtomicU64,
}
pub(super) struct Subscription {
    bus: Arc<Bus>,
    id: u64,
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| {
        eprintln!("WATCH_ERROR poisoned notification bookkeeping; recovering");
        error.into_inner()
    })
}
impl Drop for Subscription {
    fn drop(&mut self) {
        lock(&self.bus.subscribers).remove(&self.id);
    }
}
/// `extra` are further watched paths (spec 0203: plugin directories, recursively, and
/// the plugin state file, exactly).
pub(super) fn subscribe(
    root: PathBuf,
    palette: PathBuf,
    extra: Vec<PathBuf>,
    wakeup: Callback,
) -> Result<Subscription, Box<dyn std::error::Error>> {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    let mut registry = lock(REGISTRY.get_or_init(Default::default));
    registry.retain(|_, bus| bus.strong_count() != 0);
    let key = (root.clone(), palette.clone(), extra.clone());
    let bus = match registry.get(&key).and_then(Weak::upgrade) {
        Some(bus) => bus,
        None => {
            let subscribers: Subscribers = Default::default();
            let callbacks = subscribers.clone();
            let watched_root = root.clone();
            let watched_palette = palette.clone();
            let watched_extra = extra.clone();
            let mut watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let event = match event {
                        Ok(event)
                            if !event.kind.is_access()
                                && (event.need_rescan()
                                    || event.paths.is_empty()
                                    || event.paths.iter().any(|path| {
                                        path == &watched_palette
                                            || watched_extra.iter().any(|extra| {
                                                path == extra
                                                    || (extra.is_dir()
                                                        && path.starts_with(extra)
                                                        && path.extension().is_some_and(|ext| {
                                                            ext == "luau" || ext == "toml"
                                                        }))
                                            })
                                            || (path.starts_with(&watched_root)
                                                && (path.is_dir()
                                                    || path.extension().is_none()
                                                    || path.extension().is_some_and(|ext| {
                                                        [
                                                            "json", "luau", "svg", "png", "jpg",
                                                            "jpeg", "webp",
                                                        ]
                                                        .iter()
                                                        .any(|suffix| ext == *suffix)
                                                    })))
                                    })) =>
                        {
                            Ok(())
                        }
                        Err(error) => Err(error.to_string()),
                        _ => return,
                    };
                    let listeners: Vec<_> = lock(&callbacks)
                        .iter()
                        .map(|(id, callback)| (*id, callback.clone()))
                        .collect();
                    // Wakeups only enqueue work; isolate a bad subscriber from the others.
                    for (id, callback) in listeners {
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            lock(&callback)(event.clone())
                        }))
                        .is_err()
                        {
                            lock(&callbacks).remove(&id);
                            eprintln!("WATCH_ERROR subscriber panicked; subscription disabled");
                        }
                    }
                })?;
            watcher.watch(&root, notify::RecursiveMode::Recursive)?;
            for path in &extra {
                if path.starts_with(&root) {
                    continue;
                }
                if path.is_dir() {
                    watcher.watch(path, notify::RecursiveMode::Recursive)?;
                } else if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
                    watcher.watch(parent, notify::RecursiveMode::NonRecursive)?;
                }
            }
            if !palette.starts_with(&root) {
                watcher.watch(
                    palette.parent().ok_or("palette parent unavailable")?,
                    notify::RecursiveMode::NonRecursive,
                )?;
            }
            let bus = Arc::new(Bus {
                _watcher: Mutex::new(watcher),
                subscribers,
                next: AtomicU64::new(0),
            });
            registry.insert(key, Arc::downgrade(&bus));
            bus
        }
    };
    let id = bus.next.fetch_add(1, Ordering::Relaxed);
    lock(&bus.subscribers).insert(id, Arc::new(Mutex::new(wakeup)));
    Ok(Subscription { bus, id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    #[test]
    fn shared_watch_delivers_to_each_owner_and_drops_after_last_subscription() {
        let f = crate::tests::Fixture::new();
        let root = f.0.canonicalize().unwrap();
        let palette = root.join("palette.json");
        let (a, ar) = mpsc::channel();
        let first = subscribe(
            root.clone(),
            palette.clone(),
            vec![],
            Box::new(move |v| {
                let _ = a.send(v);
            }),
        )
        .unwrap();
        let (b, br) = mpsc::channel();
        let second = subscribe(
            root.clone(),
            palette.clone(),
            vec![],
            Box::new(move |v| {
                let _ = b.send(v);
            }),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&first.bus, &second.bus));
        let lifetime = Arc::downgrade(&second.bus);
        std::fs::write(root.join("change.luau"), "return 1").unwrap();
        assert!(ar.recv_timeout(Duration::from_secs(3)).unwrap().is_ok());
        assert!(br.recv_timeout(Duration::from_secs(3)).unwrap().is_ok());
        drop(first);
        while br.try_recv().is_ok() {}
        std::fs::write(root.join("change.luau"), "return 2").unwrap();
        assert!(br.recv_timeout(Duration::from_secs(3)).unwrap().is_ok());
        drop(second);
        assert!(
            lifetime.upgrade().is_none(),
            "no persistent watcher after final owner drops"
        );
    }
}
