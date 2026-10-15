// SPDX-License-Identifier: MIT
//! In-repo fake Source; no socket, daemon, compositor or external service.
use bead_core::source::{
    Callback, Error, Publisher, Reply, Result, Snapshot, Source, Subscription,
};
use serde_json::Value as Json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
static EPOCH: AtomicU64 = AtomicU64::new(1);
pub struct FakeSource {
    publisher: Publisher,
    delay: Mutex<Duration>,
    calls: AtomicU64,
}
impl FakeSource {
    pub fn new(data: Json) -> Result<Self> {
        Ok(Self {
            publisher: Publisher::new(
                format!("fake-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
                data,
            )?,
            delay: Mutex::new(Duration::ZERO),
            calls: AtomicU64::new(0),
        })
    }
    pub fn publish(&self, data: Json) -> Result<()> {
        self.publisher.publish(data)
    }
    pub fn disconnect(&self) -> Result<()> {
        self.publisher.disconnect()
    }
    pub fn delay(&self, duration: Duration) {
        if let Ok(mut delay) = self.delay.lock() {
            *delay = duration;
        }
    }
    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
}
impl Source for FakeSource {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, method: String, params: Json) -> Reply<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
            match method.as_str() {
                "fake.set" => self.publisher.publish(params)?,
                "fake.get" => {}
                _ => return Err(Error::new("unknown_method", "unknown fake method", false)),
            }
            let delay = *self
                .delay
                .lock()
                .map_err(|_| Error::new("internal", "fake delay poisoned", false))?;
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok((*self.snapshot().data).clone())
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async {
            self.publisher.publish((*self.snapshot().data).clone())?;
            Ok(Json::Null)
        })
    }
    fn stop(&self) -> Reply<'_> {
        Box::pin(async {
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}
