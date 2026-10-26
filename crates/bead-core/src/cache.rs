// SPDX-License-Identifier: MIT OR Apache-2.0
//! Process-wide admission for retained cache bytes, independent of any renderer.
//! Charges follow shared resources, so evicting a lookup does not hide live frames.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};
pub const MIB: usize = 1024 * 1024;
pub const PROCESS_LIMIT: usize = 256 * MIB;
#[derive(Default)]
struct State {
    used: usize,
    next: u64,
    owners: BTreeMap<u64, Owner>,
}
struct Owner {
    name: &'static str,
    limit: usize,
    used: usize,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}
#[derive(Clone)]
pub struct Budget(Arc<Registration>);
struct Registration(u64);
impl Drop for Registration {
    fn drop(&mut self) {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(o) = s.owners.remove(&self.0) {
            debug_assert_eq!(o.used, 0);
        }
    }
}
impl Budget {
    pub fn new(name: &'static str, limit: usize) -> Self {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        s.next += 1;
        let id = s.next;
        s.owners.insert(
            id,
            Owner {
                name,
                limit,
                used: 0,
            },
        );
        Self(Arc::new(Registration(id)))
    }
    pub fn reserve(&self, bytes: usize) -> Result<Charge, &'static str> {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        let o = s
            .owners
            .get(&self.0.0)
            .ok_or("cache registration missing")?;
        if bytes > o.limit.saturating_sub(o.used) || bytes > PROCESS_LIMIT.saturating_sub(s.used) {
            return Err("retained cache byte budget exceeded");
        }
        s.owners
            .get_mut(&self.0.0)
            .ok_or("cache registration missing")?
            .used += bytes;
        s.used += bytes;
        Ok(Charge(Arc::new(Allocation {
            budget: self.clone(),
            bytes,
        })))
    }
    pub fn used(&self) -> usize {
        state()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .owners
            .get(&self.0.0)
            .map_or(0, |o| o.used)
    }
}
/// Cloning retains the same charge, never double-counting an Arc-backed payload.
#[derive(Clone)]
pub struct Charge(Arc<Allocation>);
impl Charge {
    pub fn bytes(&self) -> usize {
        self.0.bytes
    }
}
struct Allocation {
    budget: Budget,
    bytes: usize,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        s.used -= self.bytes;
        if let Some(o) = s.owners.get_mut(&self.budget.0.0) {
            o.used -= self.bytes;
        }
    }
}
#[derive(Serialize)]
pub struct Report {
    pub limit_bytes: usize,
    pub used_bytes: usize,
    pub caches: Vec<CacheReport>,
}
#[derive(Serialize)]
pub struct CacheReport {
    pub name: &'static str,
    pub limit_bytes: usize,
    pub used_bytes: usize,
}
pub fn report() -> Report {
    let s = state().lock().unwrap_or_else(|p| p.into_inner());
    Report {
        limit_bytes: PROCESS_LIMIT,
        used_bytes: s.used,
        caches: s
            .owners
            .values()
            .map(|o| CacheReport {
                name: o.name,
                limit_bytes: o.limit,
                used_bytes: o.used,
            })
            .collect(),
    }
}
/// Conservative owned JSON payload/collection storage (excludes allocator headers).
pub fn json_bytes(v: &serde_json::Value) -> usize {
    use serde_json::Value::*;
    std::mem::size_of_val(v)
        + match v {
            String(s) => s.capacity(),
            Array(a) => {
                std::mem::size_of::<Vec<serde_json::Value>>()
                    + 2 * std::mem::size_of::<usize>()
                    + a.capacity() * std::mem::size_of::<serde_json::Value>()
                    + a.iter().map(json_bytes).sum::<usize>()
            }
            Object(o) => o
                .iter()
                .map(|(k, v)| k.capacity() + 48 + json_bytes(v))
                .sum(),
            _ => 0,
        }
}
/// Shared model payloads are conservatively admitted once per cache reference.
pub fn value_bytes(v: &crate::Value) -> usize {
    std::mem::size_of_val(v)
        + match v {
            crate::Value::Text(s) => s.capacity(),
            crate::Value::List(a) => {
                a.capacity() * std::mem::size_of::<serde_json::Value>()
                    + a.iter().map(json_bytes).sum::<usize>()
            }
            _ => 0,
        }
}
/// Byte-admitted LRU for exclusively owned values. Shared payloads carry a Charge
/// in the payload instead, keeping their lifetime visible after lookup eviction.
pub struct Lru<K, V> {
    entries: BTreeMap<K, (V, Charge, std::cell::Cell<u64>)>,
    clock: std::cell::Cell<u64>,
    budget: Budget,
}
impl<K: Ord, V> Lru<K, V> {
    pub fn new(name: &'static str, limit: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            clock: std::cell::Cell::new(0),
            budget: Budget::new(name, limit),
        }
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
    {
        let (v, _, t) = self.entries.get(key)?;
        self.clock.set(self.clock.get().wrapping_add(1));
        t.set(self.clock.get());
        Some(v)
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
    {
        self.get(key).is_some()
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
    {
        self.entries.remove(key).map(|(v, _, _)| v)
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.keys()
    }
    pub fn pop_lru(&mut self) -> Option<V>
    where
        K: Clone,
    {
        let key = self
            .entries
            .iter()
            .min_by_key(|(_, (_, _, age))| age.get())
            .map(|(key, _)| key.clone())?;
        self.remove(&key)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn bytes(&self) -> usize {
        self.budget.used()
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) -> Result<(), &'static str>
    where
        K: Clone,
    {
        self.entries.remove(&key);
        let charge = loop {
            match self.budget.reserve(bytes) {
                Ok(c) => break c,
                Err(e) => {
                    let Some(k) = self
                        .entries
                        .iter()
                        .min_by_key(|(_, (_, _, t))| t.get())
                        .map(|(k, _)| k.clone())
                    else {
                        return Err(e);
                    };
                    self.entries.remove(&k);
                }
            }
        };
        self.clock.set(self.clock.get().wrapping_add(1));
        self.entries
            .insert(key, (value, charge, std::cell::Cell::new(self.clock.get())));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn charges_survive_eviction_and_release_with_last_reference() {
        let b = Budget::new("test-shared", 128);
        let a = b.reserve(96).unwrap();
        let held = a.clone();
        drop(a);
        assert_eq!(b.used(), 96);
        assert!(b.reserve(33).is_err());
        drop(held);
        assert_eq!(b.used(), 0);
        assert!(b.reserve(128).is_ok());
    }
    #[test]
    fn global_admission_cannot_be_bypassed_by_another_owner() {
        let b = Budget::new("test-global", usize::MAX);
        assert!(b.reserve(PROCESS_LIMIT + 1).is_err());
        let held = b.reserve(PROCESS_LIMIT / 2).unwrap();
        let other = Budget::new("test-independent-owner", PROCESS_LIMIT);
        assert!(other.reserve(PROCESS_LIMIT / 2 + 1).is_err());
        drop(held);
        let r = report();
        assert!(r.used_bytes <= r.limit_bytes);
        assert!(r.caches.iter().all(|c| c.used_bytes <= c.limit_bytes));
    }
}
