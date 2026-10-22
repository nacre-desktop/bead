// SPDX-License-Identifier: GPL-3.0-only
//! Nacre N0035 catalog provider: follows the `i18n` topic and keeps the `bead`
//! namespace of `i18n.catalog` in the generic `bead_ui::i18n` catalog.
//!
//! One revision is held at a time (a few KiB): a new revision is fetched in full
//! before it replaces the old one, so a failed fetch keeps the last good text
//! and a daemon-less shell keeps the built-in English.
use bead::{
    core::source::{Error, Source, Subscription},
    ui::i18n::Entries,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

pub const NAMESPACE: &str = "bead";
const METHOD: &str = "i18n.catalog";
const MAX_PAGES: usize = 32;
const MAX_ENTRIES: usize = 2048;
const MAX_BYTES: usize = 256 * 1024;
const STALE_RETRIES: u32 = 3;
const RETRY_AFTER: Duration = Duration::from_secs(5);

pub type Sink = Arc<dyn Fn(u64, Entries) + Send + Sync>;

#[derive(Deserialize)]
struct Page {
    revision: u64,
    entries: Entries,
    #[serde(default)]
    next: Option<String>,
}
fn bad(message: &str) -> Error {
    Error::new("protocol", message, false)
}

/// Every page of the namespace at `revision`, or `stale_revision` when the
/// daemon moved on (the caller re-reads the topic and tries again).
pub async fn fetch(topic: &dyn Source, revision: u64) -> Result<Entries, Error> {
    let (mut out, mut bytes, mut after) = (Entries::new(), 0, None::<String>);
    for _ in 0..MAX_PAGES {
        let mut params = json!({"namespace": NAMESPACE, "revision": revision});
        if let Some(after) = &after {
            params["after"] = json!(after);
        }
        let reply = topic
            .call(METHOD.into(), params, Duration::from_secs(3))
            .await?;
        let page: Page =
            serde_json::from_value(reply).map_err(|_| bad("malformed catalog page"))?;
        if page.revision != revision {
            return Err(Error::new("stale_revision", "catalog revision moved", true));
        }
        for (id, message) in page.entries {
            if !id.starts_with("bead.") || after.as_ref().is_some_and(|a| id <= *a) {
                return Err(bad("catalog id outside the page contract"));
            }
            bytes += id.len() + 64;
            bytes += match &message {
                bead::ui::i18n::Message::Text(t) => t.len(),
                bead::ui::i18n::Message::Plural { one, other } => one.len() + other.len(),
            };
            out.insert(id, message);
        }
        if out.len() > MAX_ENTRIES || bytes > MAX_BYTES {
            return Err(bad("catalog exceeds the shell bound"));
        }
        match page.next {
            None => return Ok(out),
            Some(next) if after.as_ref().is_none_or(|a| next > *a) => after = Some(next),
            Some(_) => return Err(bad("catalog paging did not advance")),
        }
    }
    Err(bad("catalog has too many pages"))
}

#[derive(Default)]
struct Wake {
    state: Mutex<(bool, bool)>, // (pending, stopped)
    cv: Condvar,
}
impl Wake {
    fn notify(&self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).0 = true;
        self.cv.notify_all();
    }
    fn stop(&self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).1 = true;
        self.cv.notify_all();
    }
    /// Wait for a wake-up (or the retry timeout); false once stopped.
    fn wait(&self, timeout: Option<Duration>) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        while !state.0 && !state.1 {
            match timeout {
                Some(t) => {
                    let (next, result) = self
                        .cv
                        .wait_timeout(state, t)
                        .unwrap_or_else(|p| p.into_inner());
                    state = next;
                    if result.timed_out() {
                        break;
                    }
                }
                None => state = self.cv.wait(state).unwrap_or_else(|p| p.into_inner()),
            }
        }
        state.0 = false;
        !state.1
    }
}

pub struct Service {
    wake: Arc<Wake>,
    thread: Option<std::thread::JoinHandle<()>>,
    _subscription: Subscription,
}
impl Service {
    pub fn start(topic: Arc<dyn Source>, sink: Sink) -> Result<Self, Error> {
        let wake = Arc::new(Wake::default());
        let notify = wake.clone();
        let subscription = topic.subscribe(Arc::new(move |_| notify.notify()))?;
        let worker = wake.clone();
        let thread = std::thread::Builder::new()
            .name("bead-i18n".into())
            .stack_size(256 * 1024)
            .spawn(move || run(&*topic, &sink, &worker))
            .map_err(|_| Error::new("io", "i18n thread unavailable", true))?;
        Ok(Self {
            wake,
            thread: Some(thread),
            _subscription: subscription,
        })
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.wake.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn run(topic: &dyn Source, sink: &Sink, wake: &Wake) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    else {
        return;
    };
    let (mut loaded, mut stale, mut retry) = (0_u64, 0, None);
    let (mut failed_at, mut failures) = (0_u64, 0);
    while wake.wait(retry.take()) {
        let snapshot = topic.snapshot();
        let revision = snapshot.data.get("revision").and_then(|r| r.as_u64());
        let Some(revision) = revision.filter(|r| snapshot.connected && *r != loaded) else {
            continue;
        };
        if revision != failed_at {
            (failed_at, failures) = (revision, 0);
        }
        match runtime.block_on(fetch(topic, revision)) {
            Ok(entries) => {
                (loaded, stale) = (revision, 0);
                sink(revision, entries);
            }
            Err(e) if e.code == "stale_revision" && stale < STALE_RETRIES => {
                stale += 1;
                wake.notify();
            }
            Err(e) => {
                stale = 0;
                eprintln!("I18N_FETCH_FAILED code={}", e.code);
                failures += 1;
                retry = (failures <= 3).then_some(RETRY_AFTER);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::core::source::{Publisher, Reply, Result as SourceResult, Snapshot};
    use serde_json::Value as Json;
    use std::sync::mpsc;

    struct Daemon {
        topic: Publisher,
        revision: Mutex<u64>,
        entries: Mutex<Vec<(String, Json)>>,
        page: usize,
        calls: Mutex<Vec<Json>>,
    }
    impl Daemon {
        fn new(page: usize) -> Arc<Self> {
            Arc::new(Self {
                topic: Publisher::new("i18n-test".into(), json!({})).unwrap(),
                revision: Mutex::new(1),
                entries: Mutex::default(),
                page,
                calls: Mutex::default(),
            })
        }
        /// The daemon switches language: it serves the new text at the next revision.
        fn switch(&self, revision: u64, pseudo: bool) {
            *self.revision.lock().unwrap() = revision;
            let mut entries: Vec<(String, Json)> = (0..5)
                .map(|n| (format!("bead.fake.item-{n}"), json!(format!("Item {n}"))))
                .collect();
            entries.push((
                "bead.fake.files".into(),
                json!({"one": "{count} file", "other": "{count} files"}),
            ));
            if pseudo {
                for (_, v) in &mut entries {
                    *v = match v.take() {
                        Json::String(s) => json!(format!("[!! {s} !!]")),
                        plural => plural,
                    };
                }
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            *self.entries.lock().unwrap() = entries;
        }
        fn announce(&self, language: &str, revision: u64) {
            self.topic
                .publish(json!({"language": language, "fallback": "en", "revision": revision}))
                .unwrap();
        }
        fn reply(&self, params: Json) -> SourceResult<Json> {
            self.calls.lock().unwrap().push(params.clone());
            assert_eq!(params["namespace"], "bead");
            let revision = *self.revision.lock().unwrap();
            if params
                .get("revision")
                .is_some_and(|r| *r != json!(revision))
            {
                return Err(Error::new("stale_revision", "revision changed", true));
            }
            let after = params.get("after").and_then(|a| a.as_str()).unwrap_or("");
            let entries = self.entries.lock().unwrap();
            let rest: Vec<_> = entries
                .iter()
                .filter(|(id, _)| id.as_str() > after)
                .collect();
            let page: serde_json::Map<String, Json> = rest
                .iter()
                .take(self.page)
                .map(|(id, v)| (id.clone(), v.clone()))
                .collect();
            let mut out = json!({"revision": revision, "entries": page});
            if rest.len() > self.page {
                out["next"] = json!(rest[self.page - 1].0);
            }
            Ok(out)
        }
    }
    struct Fake(Arc<Daemon>);
    impl Source for Fake {
        fn snapshot(&self) -> Arc<Snapshot> {
            self.0.topic.snapshot()
        }
        fn subscribe(&self, cb: bead::core::source::Callback) -> SourceResult<Subscription> {
            self.0.topic.subscribe(cb)
        }
        fn invoke(&self, method: String, params: Json) -> Reply<'_> {
            assert_eq!(method, METHOD);
            Box::pin(std::future::ready(self.0.reply(params)))
        }
        fn reconnect(&self) -> Reply<'_> {
            Box::pin(std::future::ready(Ok(Json::Null)))
        }
        fn stop(&self) -> Reply<'_> {
            Box::pin(std::future::ready(Ok(Json::Null)))
        }
    }
    fn block<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(f)
    }
    fn text(entries: &Entries, id: &str) -> String {
        match &entries[id] {
            bead::ui::i18n::Message::Text(t) => t.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pages_are_followed_with_the_revision_and_the_cursor() {
        let daemon = Daemon::new(2);
        daemon.switch(1, false);
        let entries = block(fetch(&Fake(daemon.clone()), 1)).unwrap();
        assert_eq!(entries.len(), 6);
        assert_eq!(text(&entries, "bead.fake.item-4"), "Item 4");
        let calls = daemon.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].get("after").is_none());
        assert_eq!(calls[1]["after"], "bead.fake.item-0");
        assert_eq!(calls[2]["after"], "bead.fake.item-2");
        assert!(calls.iter().all(|c| c["revision"] == 1));
    }
    #[test]
    fn a_moved_revision_is_reported_as_stale_and_bad_pages_are_refused() {
        let daemon = Daemon::new(2);
        daemon.switch(2, false);
        let error = block(fetch(&Fake(daemon.clone()), 1)).unwrap_err();
        assert_eq!((error.code.as_str(), error.retry), ("stale_revision", true));
        // An id outside the bead namespace is a contract violation.
        daemon
            .entries
            .lock()
            .unwrap()
            .push(("zzz.other".into(), json!("x")));
        assert_eq!(block(fetch(&Fake(daemon), 2)).unwrap_err().code, "protocol");
    }
    #[test]
    fn language_switch_replaces_the_catalog_and_a_racing_revision_recovers() {
        let daemon = Daemon::new(2);
        let (tx, rx) = mpsc::channel();
        let sink: Sink = Arc::new(move |revision, entries| {
            let _ = tx.send((revision, entries));
        });
        daemon.switch(1, false);
        daemon.announce("en", 1);
        let service = Service::start(Arc::new(Fake(daemon.clone())), sink).unwrap();
        let wait = || rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (revision, entries) = wait();
        assert_eq!(
            (revision, text(&entries, "bead.fake.item-0").as_str()),
            (1, "Item 0")
        );
        // en -> pseudo.
        daemon.switch(2, true);
        daemon.announce("pseudo", 2);
        let (revision, entries) = wait();
        assert_eq!(revision, 2);
        assert_eq!(text(&entries, "bead.fake.item-0"), "[!! Item 0 !!]");
        // The daemon moves on to revision 4 before the topic says so: the announced
        // revision 3 is refused as stale (bounded retries), then 4 is fetched.
        daemon.switch(4, false);
        let before = daemon.calls.lock().unwrap().len();
        daemon.announce("en", 3);
        for _ in 0..500 {
            if daemon.calls.lock().unwrap().len() >= before + 1 + STALE_RETRIES as usize {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            daemon.calls.lock().unwrap().len(),
            before + 1 + STALE_RETRIES as usize
        );
        assert!(rx.try_recv().is_err());
        daemon.announce("en", 4);
        let (revision, entries) = wait();
        assert_eq!(
            (revision, text(&entries, "bead.fake.item-0").as_str()),
            (4, "Item 0")
        );
        drop(service);
    }
}
