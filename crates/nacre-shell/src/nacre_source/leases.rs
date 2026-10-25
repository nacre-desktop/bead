// SPDX-License-Identifier: GPL-3.0-only
//! Page-scoped subscriptions on the shell's existing daemon connection.
use super::*;

#[derive(Default)]
pub(super) struct Registry {
    records: Mutex<BTreeMap<String, Record>>,
    pub changed: Notify,
    next: AtomicU64,
}
struct Record {
    generation: u64,
    users: usize,
    publisher: Publisher,
    resync: bool,
}
pub(super) struct Wanted {
    pub generation: u64,
    pub publisher: Publisher,
    pub resync: bool,
}
impl Registry {
    pub(super) fn missing(&self, advertised: &BTreeMap<String, u16>) -> bool {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .any(|name| !advertised.contains_key(name))
    }
    #[cfg(test)]
    pub(super) fn has_topic(&self, name: &str) -> bool {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(name)
    }

    pub fn wanted(&self) -> BTreeMap<String, Wanted> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter_mut()
            .map(|(name, r)| {
                let resync = std::mem::take(&mut r.resync);
                (
                    name.clone(),
                    Wanted {
                        generation: r.generation,
                        publisher: r.publisher.clone(),
                        resync,
                    },
                )
            })
            .collect()
    }
    pub fn publisher(&self, name: &str) -> Option<Publisher> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(name)
            .map(|r| r.publisher.clone())
    }
    pub fn publishers(&self) -> Vec<Publisher> {
        self.records
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|r| r.publisher.clone())
            .collect()
    }
}

/// Clones share a lease. Independent acquisitions share a topic until the last
/// lease stops or drops; neither action stops the UI provider or other topics.
#[derive(Clone)]
pub struct LeasedTopic(Arc<Lease>);
struct Lease {
    client: Client,
    name: String,
    generation: u64,
    publisher: Mutex<Option<Publisher>>,
    empty: Arc<Snapshot>,
}
impl Client {
    pub fn lease_topic(&self, name: &str) -> source::Result<LeasedTopic> {
        if self.0.shared.topics.contains_key(name) {
            return Err(Error::new(
                "validation",
                "permanent topic cannot be leased",
                false,
            ));
        }
        framing::encode(&Message::Sub {
            topics: vec![name.into()],
        })
        .map_err(failure)?;
        if self.0.shared.stop.load(Ordering::Acquire) {
            return Err(unavailable());
        }
        let mut records = self
            .0
            .shared
            .leases
            .records
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if !records.contains_key(name)
            && records.len() + self.0.shared.topics.len() >= nacre_protocol::MAX_SUBSCRIPTIONS
        {
            return Err(Error::new("busy", "daemon subscription limit", false));
        }
        let record = match records.entry(name.into()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let publisher = Publisher::new("pending".into(), serde_json::json!({}))?;
                publisher.disconnect()?;
                entry.insert(Record {
                    generation: self.0.shared.leases.next.fetch_add(1, Ordering::Relaxed),
                    users: 0,
                    publisher,
                    resync: false,
                })
            }
        };
        record.users += 1;
        let publisher = record.publisher.clone();
        let generation = record.generation;
        drop(records);
        let empty = Arc::new(Snapshot {
            epoch: "pending".into(),
            revision: 0,
            data: Arc::new(serde_json::json!({})),
            connected: false,
        });
        self.0.shared.leases.changed.notify_one();
        Ok(LeasedTopic(Arc::new(Lease {
            client: self.clone(),
            name: name.into(),
            generation,
            publisher: Mutex::new(Some(publisher)),
            empty,
        })))
    }
}
impl Lease {
    fn publisher(&self) -> source::Result<Publisher> {
        self.publisher
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(unavailable)
    }
    fn release(&self) {
        let publisher = self
            .publisher
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if publisher.is_none() {
            return;
        }
        let mut records = self
            .client
            .0
            .shared
            .leases
            .records
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(r) = records.get_mut(&self.name)
            && r.generation == self.generation
        {
            r.users -= 1;
            if r.users == 0 {
                records.remove(&self.name);
            }
        }
        drop(records);
        self.client.0.shared.leases.changed.notify_one();
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
impl Source for LeasedTopic {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.0
            .publisher()
            .map(|p| p.snapshot())
            .unwrap_or_else(|_| self.0.empty.clone())
    }
    fn subscribe(&self, cb: Callback) -> source::Result<Subscription> {
        self.0.publisher()?.subscribe(cb)
    }
    fn invoke(&self, m: String, p: Value) -> source::Reply<'_> {
        Source::call(self, m, p, Duration::from_secs(3))
    }
    fn call(&self, m: String, p: Value, d: Duration) -> source::Reply<'_> {
        Box::pin(async move {
            self.0.publisher()?;
            self.0.client.call(m, p, d).await
        })
    }
    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(async move {
            self.0.publisher()?;
            let mut records = self
                .0
                .client
                .0
                .shared
                .leases
                .records
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            if let Some(r) = records.get_mut(&self.0.name)
                && r.generation == self.0.generation
            {
                r.resync = true;
            }
            drop(records);
            self.0.client.0.shared.leases.changed.notify_one();
            Ok(Value::Null)
        })
    }
    fn stop(&self) -> source::Reply<'_> {
        Box::pin(async move {
            self.0.release();
            Ok(Value::Null)
        })
    }
}
