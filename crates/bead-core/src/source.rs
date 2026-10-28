// SPDX-License-Identifier: MIT OR Apache-2.0
//! Protocol-neutral external data, bounded publication and transactional store projection.
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub retry: bool,
}
impl Error {
    pub fn new(code: &str, message: &str, retry: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retry,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
pub type Reply<'a> = Pin<Box<dyn Future<Output = Result<Json>> + Send + 'a>>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch: String,
    pub revision: u64,
    pub data: Arc<Json>,
    pub connected: bool,
}
pub type Callback = Arc<dyn Fn(Arc<Snapshot>) + Send + Sync>;
pub trait Source: Send + Sync {
    fn snapshot(&self) -> Arc<Snapshot>;
    fn subscribe(&self, callback: Callback) -> Result<Subscription>;
    fn invoke(&self, method: String, params: Json) -> Reply<'_>;
    fn reconnect(&self) -> Reply<'_>;
    fn stop(&self) -> Reply<'_>;
    /// Uses the caller's Tokio time driver, never creates another runtime/thread.
    /// Timeout does not roll back a side effect or automatically retry a mutation.
    fn call(&self, method: String, params: Json, timeout: Duration) -> Reply<'_> {
        Box::pin(async move {
            if timeout.is_zero()
                || timeout > Duration::from_secs(120)
                || method.is_empty()
                || method.len() > 128
            {
                return Err(Error::new(
                    "validation",
                    "invalid source call/deadline",
                    false,
                ));
            }
            bounded(&params)?;
            tokio::time::timeout(timeout, self.invoke(method, params))
                .await
                .map_err(|_| {
                    Error::new(
                        "timeout",
                        "source call timed out; re-read before retry",
                        true,
                    )
                })?
        })
    }
}
struct State {
    snapshot: Arc<Snapshot>,
    callbacks: BTreeMap<u64, Callback>,
    next: u64,
}
#[derive(Clone)]
pub struct Publisher {
    state: Arc<Mutex<State>>,
}
pub struct Subscription {
    state: Weak<Mutex<State>>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade()
            && let Ok(mut state) = state.lock()
        {
            state.callbacks.remove(&self.id);
        }
    }
}
impl Publisher {
    pub fn new(epoch: String, data: Json) -> Result<Self> {
        Self::at(epoch, 0, data)
    }
    pub fn at(epoch: String, revision: u64, data: Json) -> Result<Self> {
        validate_epoch(&epoch)?;
        object(&data)?;
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                snapshot: Arc::new(Snapshot {
                    epoch,
                    revision,
                    data: Arc::new(data),
                    connected: true,
                }),
                callbacks: BTreeMap::new(),
                next: 0,
            })),
        })
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .snapshot
            .clone()
    }
    pub fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        let (id, snapshot) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::new("internal", "source state poisoned", false))?;
            if state.callbacks.len() >= 64 {
                return Err(Error::new("busy", "source subscriber limit", true));
            }
            state.next = state
                .next
                .checked_add(1)
                .ok_or_else(|| Error::new("internal", "subscription id exhausted", false))?;
            let id = state.next;
            state.callbacks.insert(id, callback.clone());
            (id, state.snapshot.clone())
        };
        callback(snapshot);
        Ok(Subscription {
            state: Arc::downgrade(&self.state),
            id,
        })
    }
    /// Preserve last-good data while changing the producer's lifetime identity.
    /// A fresh snapshot may be byte-identical after restart; the epoch must still
    /// invalidate consumers' cached results and activation handles.
    pub fn restart(&self, epoch: String) -> Result<()> {
        validate_epoch(&epoch)?;
        let (snapshot, callbacks) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::new("internal", "source state poisoned", false))?;
            if state.snapshot.epoch == epoch {
                return Ok(());
            }
            let snapshot = Arc::new(Snapshot {
                epoch,
                revision: 0,
                data: state.snapshot.data.clone(),
                connected: false,
            });
            state.snapshot = snapshot.clone();
            (
                snapshot,
                state.callbacks.values().cloned().collect::<Vec<_>>(),
            )
        };
        for callback in callbacks {
            callback(snapshot.clone());
        }
        Ok(())
    }
    pub fn publish(&self, data: Json) -> Result<()> {
        object(&data)?;
        self.change(Some(Arc::new(data)), true, None)
    }
    /// Share an immutable transport snapshot without duplicating its payload.
    pub fn publish_shared(&self, data: Arc<Json>) -> Result<()> {
        object(&data)?;
        self.change(Some(data), true, None)
    }
    pub fn disconnect(&self) -> Result<()> {
        self.change(None, false, None)
    }
    pub fn publish_revision(&self, revision: u64, data: Json) -> Result<()> {
        object(&data)?;
        self.change(Some(Arc::new(data)), true, Some(revision))
    }
    fn change(&self, data: Option<Arc<Json>>, connected: bool, origin: Option<u64>) -> Result<()> {
        let (snapshot, callbacks) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::new("internal", "source state poisoned", false))?;
            if let Some(revision) = origin {
                if revision < state.snapshot.revision {
                    return Ok(());
                }
                if revision == state.snapshot.revision
                    && data.as_ref().is_some_and(|d| **d != *state.snapshot.data)
                {
                    return Err(Error::new("validation", "origin revision reused", false));
                }
            }
            if data.as_ref().is_none_or(|d| **d == *state.snapshot.data)
                && connected == state.snapshot.connected
                && origin.is_none_or(|r| r == state.snapshot.revision)
            {
                return Ok(());
            }
            let changed = data.as_ref().is_some_and(|d| **d != *state.snapshot.data);
            let revision = if let Some(revision) = origin {
                revision
            } else if changed {
                state
                    .snapshot
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| Error::new("internal", "source revision exhausted", false))?
            } else {
                state.snapshot.revision
            };
            let snapshot = Arc::new(Snapshot {
                epoch: state.snapshot.epoch.clone(),
                revision,
                data: data.unwrap_or_else(|| state.snapshot.data.clone()),
                connected,
            });
            state.snapshot = snapshot.clone();
            (
                snapshot,
                state.callbacks.values().cloned().collect::<Vec<_>>(),
            )
        };
        // Reentrant listeners run unlocked; mirrors reject obsolete callback revisions.
        for callback in callbacks {
            callback(snapshot.clone());
        }
        Ok(())
    }
}
fn validate_epoch(epoch: &str) -> Result<()> {
    if epoch.is_empty() || epoch.len() > 128 {
        Err(Error::new("validation", "invalid source epoch", false))
    } else {
        Ok(())
    }
}
fn object(value: &Json) -> Result<()> {
    if !value.is_object() {
        return Err(Error::new(
            "validation",
            "source snapshot must be an object",
            false,
        ));
    }
    bounded(value)
}
/// JSON has an8MiB encoded cap,64 levels and65536 values. Count escapes without allocating JSON.
pub fn bounded(value: &Json) -> Result<()> {
    let mut pending = vec![(value, 0usize)];
    let mut count = 0usize;
    while let Some((value, depth)) = pending.pop() {
        count += 1;
        if depth > 64 || count > 65536 {
            return Err(Error::new("too_large", "source depth/value budget", false));
        }
        match value {
            Json::Array(a) => {
                if a.len() + pending.len() + count > 65536 {
                    return Err(Error::new("too_large", "source value budget", false));
                }
                pending.extend(a.iter().map(|v| (v, depth + 1)));
            }
            Json::Object(o) => {
                if o.len() + pending.len() + count > 65536 {
                    return Err(Error::new("too_large", "source value budget", false));
                }
                pending.extend(o.values().map(|v| (v, depth + 1)));
            }
            _ => {}
        }
    }
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("source size overflow"))?;
            if self.0 > 8 * 1024 * 1024 {
                return Err(std::io::Error::other("source byte budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Count(0), value)
        .map_err(|_| Error::new("too_large", "source encoded byte budget", false))
}
/// Independent implementation of RFC7396: null removes members; arrays/scalars replace.
/// Callers validate depth/bytes first, then validate the resulting object before publication.
pub fn merge_patch(target: &mut Json, patch: &Json) -> Result<()> {
    bounded(target)?;
    bounded(patch)?;
    let mut candidate = target.clone();
    apply_patch(&mut candidate, patch);
    bounded(&candidate)?;
    *target = candidate;
    Ok(())
}
fn apply_patch(target: &mut Json, patch: &Json) {
    if let Some(changes) = patch.as_object() {
        if !target.is_object() {
            *target = Json::Object(Default::default());
        }
        if let Some(object) = target.as_object_mut() {
            for (key, value) in changes {
                if value.is_null() {
                    object.remove(key);
                } else {
                    apply_patch(object.entry(key.clone()).or_insert(Json::Null), value);
                }
            }
        }
    } else {
        *target = patch.clone();
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patch {
    pub epoch: String,
    pub base: u64,
    pub revision: u64,
    pub data: Json,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    Updated,
    Ignored,
    Resync,
    Disconnected,
}
#[derive(Clone)]
pub struct Mirror {
    snapshot: Option<Arc<Snapshot>>,
    expected_epoch: Option<String>,
    stale_since: Option<Instant>,
    resync: bool,
}
impl Mirror {
    pub fn new(now: Instant) -> Self {
        Self {
            snapshot: None,
            expected_epoch: None,
            stale_since: Some(now),
            resync: true,
        }
    }
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_deref()
    }
    pub fn needs_snapshot(&self) -> bool {
        self.resync
    }
    pub fn stale(&self) -> bool {
        self.stale_since.is_some()
    }
    pub fn reconnecting(&self, now: Instant) -> bool {
        self.stale_since
            .is_some_and(|since| now.saturating_duration_since(since) >= Duration::from_secs(2))
    }
    pub fn disconnected(&mut self, now: Instant) {
        self.stale_since.get_or_insert(now);
        self.resync = true;
    }
    /// Hook for a transport handshake; retain old data until a fresh snapshot arrives.
    pub fn reconnected(&mut self, epoch: &str, now: Instant) -> Result<bool> {
        validate_epoch(epoch)?;
        let changed = self.snapshot.as_ref().is_none_or(|s| s.epoch != epoch);
        self.expected_epoch = Some(epoch.into());
        if changed {
            self.disconnected(now);
        }
        Ok(changed)
    }
    pub fn accept(&mut self, snapshot: Arc<Snapshot>, now: Instant) -> Result<Applied> {
        self.accept_with_change(snapshot, now).map(|(applied, _)| applied)
    }
    /// Accept a full snapshot and report whether its payload changed. An unchanged
    /// payload still advances the mirror revision but need not be projected again.
    pub fn accept_with_change(
        &mut self,
        snapshot: Arc<Snapshot>,
        now: Instant,
    ) -> Result<(Applied, bool)> {
        validate_epoch(&snapshot.epoch)?;
        if self
            .expected_epoch
            .as_ref()
            .is_some_and(|epoch| *epoch != snapshot.epoch)
        {
            return Ok((Applied::Ignored, false));
        }
        if !snapshot.connected {
            self.disconnected(now);
            return Ok((Applied::Disconnected, false));
        }
        object(&snapshot.data)?;
        if let Some(old) = &self.snapshot
            && old.epoch == snapshot.epoch
        {
            if snapshot.revision < old.revision {
                return Ok((Applied::Ignored, false));
            }
            if snapshot.revision == old.revision && snapshot.data != old.data {
                return Err(Error::new(
                    "validation",
                    "source revision reused with different data",
                    false,
                ));
            }
            if snapshot.revision == old.revision || snapshot.data == old.data {
                self.expected_epoch = Some(snapshot.epoch.clone());
                if snapshot.revision != old.revision {
                    self.snapshot = Some(snapshot);
                }
                self.stale_since = None;
                self.resync = false;
                return Ok((Applied::Updated, false));
            }
        }
        self.expected_epoch = Some(snapshot.epoch.clone());
        self.snapshot = Some(snapshot);
        self.stale_since = None;
        self.resync = false;
        Ok((Applied::Updated, true))
    }
    pub fn patch(
        &mut self,
        epoch: &str,
        base: u64,
        revision: u64,
        patch: &Json,
        now: Instant,
    ) -> Result<Applied> {
        validate_epoch(epoch)?;
        if self
            .expected_epoch
            .as_ref()
            .is_some_and(|expected| *expected != epoch)
        {
            return Ok(Applied::Ignored);
        }
        bounded(patch)?;
        if let Some(old) = &self.snapshot
            && old.epoch == epoch
            && revision <= old.revision
        {
            return Ok(Applied::Ignored);
        }
        let Some(old) = &self.snapshot else {
            self.disconnected(now);
            return Ok(Applied::Resync);
        };
        if self.resync || old.epoch != epoch || base != old.revision {
            self.disconnected(now);
            return Ok(Applied::Resync);
        }
        let mut data = (*old.data).clone();
        apply_patch(&mut data, patch);
        object(&data)?;
        self.accept(
            Arc::new(Snapshot {
                epoch: epoch.into(),
                revision,
                data: Arc::new(data),
                connected: true,
            }),
            now,
        )
    }
    pub fn values(
        &self,
        prefix: &str,
        projection: impl FnOnce(&Json) -> Result<BTreeMap<String, crate::Value>>,
        now: Instant,
    ) -> Result<BTreeMap<String, crate::Value>> {
        if prefix.is_empty() || prefix.len() > 128 {
            return Err(Error::new(
                "validation",
                "invalid source binding prefix",
                false,
            ));
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or_else(|| Error::new("unavailable", "source has no snapshot", true))?;
        let mut values = projection(&snapshot.data)?;
        values.insert(format!("{prefix}.stale"), crate::Value::Bool(self.stale()));
        values.insert(
            format!("{prefix}.reconnecting"),
            crate::Value::Bool(self.reconnecting(now)),
        );
        Ok(values)
    }
    pub fn project_patch(
        &mut self,
        stores: &mut crate::Stores,
        patch: &Patch,
        projection: impl FnOnce(&Json) -> Result<BTreeMap<String, crate::Value>>,
        now: Instant,
    ) -> Result<Applied> {
        let mut staged = self.clone();
        let applied = staged.patch(&patch.epoch, patch.base, patch.revision, &patch.data, now)?;
        if applied == Applied::Updated {
            let values = projection(
                &staged
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| Error::new("internal", "missing staged snapshot", false))?
                    .data,
            )?;
            let mut candidate = stores
                .reconcile(&values)
                .map_err(|_| Error::new("validation", "source store schema rejected", false))?;
            candidate
                .commit(values)
                .map_err(|_| Error::new("validation", "source store values rejected", false))?;
            *stores = candidate;
        }
        *self = staged;
        Ok(applied)
    }
    /// Projection and store commit are atomic with snapshot replacement.
    pub fn project(
        &mut self,
        stores: &mut crate::Stores,
        snapshot: Arc<Snapshot>,
        projection: impl FnOnce(&Json) -> Result<BTreeMap<String, crate::Value>>,
        now: Instant,
    ) -> Result<Applied> {
        let mut staged = self.clone();
        let (applied, changed) = staged.accept_with_change(snapshot, now)?;
        if changed {
            let values = projection(
                &staged
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| Error::new("internal", "missing staged snapshot", false))?
                    .data,
            )?;
            let mut candidate = stores
                .reconcile(&values)
                .map_err(|_| Error::new("validation", "source store schema rejected", false))?;
            candidate
                .commit(values)
                .map_err(|_| Error::new("validation", "source store values rejected", false))?;
            *stores = candidate;
        }
        *self = staged;
        Ok(applied)
    }
}
#[derive(Default)]
pub struct Reconnect {
    attempt: u8,
}
impl Reconnect {
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
    pub fn delay(&mut self) -> Duration {
        let ms = (50u64 << self.attempt.min(6)).min(2000);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_millis(ms)
    }
}

#[cfg(test)]
mod restart_tests {
    use super::*;
    #[test]
    fn restart_changes_identity_even_when_fresh_data_is_identical() {
        let p = Publisher::new("boot-a".into(), serde_json::json!({"rows":{}})).unwrap();
        p.publish_revision(42, serde_json::json!({"rows":{}}))
            .unwrap();
        let before = p.snapshot();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let callbacks = seen.clone();
        let _subscription = p
            .subscribe(Arc::new(move |s| callbacks.lock().unwrap().push(s)))
            .unwrap();
        p.restart("boot-b".into()).unwrap();
        assert!(!p.snapshot().connected);
        assert_eq!(p.snapshot().data, before.data);
        p.publish_shared(before.data.clone()).unwrap();
        let next = p.snapshot();
        assert!(next.connected);
        assert_ne!(next.epoch, before.epoch);
        assert_eq!(next.revision, 0);
        assert_eq!(seen.lock().unwrap().len(), 3);
        p.restart("boot-b".into()).unwrap();
        assert_eq!(seen.lock().unwrap().len(), 3);
        assert!(p.restart(String::new()).is_err());
        assert_eq!(p.snapshot().epoch, "boot-b");
    }

    #[test]
    fn identical_snapshots_do_not_restage_and_revisions_still_advance() {
        let now = Instant::now();
        let mut mirror = Mirror::new(now);
        let make = |revision| {
            Arc::new(Snapshot {
                epoch: "boot-a".into(),
                revision,
                data: Arc::new(serde_json::json!({"clock":"10:11","wallpaper":"still"})),
                connected: true,
            })
        };
        assert_eq!(mirror.accept(make(0), now).unwrap(), Applied::Updated);
        let mut restages = 0;
        for revision in 1..=60 {
            let (applied, changed) = mirror.accept_with_change(make(revision), now).unwrap();
            if changed {
                restages += 1;
            }
            assert_eq!(applied, Applied::Updated);
        }
        assert_eq!(restages, 0);
        assert_eq!(mirror.snapshot().unwrap().revision, 60);
        let different = Arc::new(Snapshot {
            epoch: "boot-a".into(),
            revision: 61,
            data: Arc::new(serde_json::json!({"clock":"10:12","wallpaper":"still"})),
            connected: true,
        });
        assert_eq!(mirror.accept(different, now).unwrap(), Applied::Updated);
    }
}
