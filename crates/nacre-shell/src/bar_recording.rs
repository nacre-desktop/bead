// SPDX-License-Identifier: GPL-3.0-only
//! B0199: the bar's recording indicator. QML showed a red stop button with the elapsed time in
//! the status column while a recording ran. The daemon publishes `controls.recorder.recording`;
//! this turns it into `bar.recording` and a once-a-second `bar.recording.elapsed`, and only while
//! it is true does anything tick. Stop is `recorder.stop` on the same daemon.
use bead::core::{Value, source::Snapshot};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// The daemon is connected and says a recording is running.
pub fn recording(snapshot: &Snapshot) -> bool {
    snapshot.connected && snapshot.data["recorder"]["recording"] == true
}
/// QML `elapsed`: `m:ss`, with hours once there are any.
pub fn elapsed(seconds: u64) -> String {
    match (seconds / 3600, seconds / 60 % 60, seconds % 60) {
        (0, minutes, seconds) => format!("{minutes}:{seconds:02}"),
        (hours, minutes, seconds) => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}
pub fn values(on: bool, seconds: u64) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("bar.recording".into(), Value::Bool(on)),
        (
            "bar.recording.elapsed".into(),
            Value::Text(if on { elapsed(seconds) } else { String::new() }),
        ),
    ])
}

/// Follows the topic: a ticker thread lives exactly as long as the recording does.
pub struct Indicator {
    running: Option<Arc<AtomicBool>>,
}
impl Indicator {
    pub fn new() -> Self {
        Self { running: None }
    }
    /// Applies one snapshot; `publish` receives the stores to set (on a thread, once a second).
    pub fn follow(
        &mut self,
        snapshot: &Snapshot,
        publish: impl Fn(BTreeMap<String, Value>) + Send + 'static,
    ) {
        let on = recording(snapshot);
        if on == self.running.is_some() {
            return;
        }
        if let Some(flag) = self.running.take() {
            flag.store(false, Ordering::Release);
            publish(values(false, 0));
            return;
        }
        let flag = Arc::new(AtomicBool::new(true));
        self.running = Some(flag.clone());
        let started = Instant::now();
        std::thread::spawn(move || {
            while flag.load(Ordering::Acquire) {
                publish(values(true, started.elapsed().as_secs()));
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }
}
impl Drop for Indicator {
    fn drop(&mut self) {
        if let Some(flag) = self.running.take() {
            flag.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn snapshot(connected: bool, recording: bool) -> Snapshot {
        Snapshot {
            epoch: "test".into(),
            revision: 1,
            data: Arc::new(serde_json::json!({"recorder": {"recording": recording}})),
            connected,
        }
    }
    #[test]
    fn only_a_connected_running_recorder_shows_the_indicator() {
        assert!(recording(&snapshot(true, true)));
        assert!(!recording(&snapshot(true, false)));
        assert!(!recording(&snapshot(false, true)));
        let mut absent = snapshot(true, true);
        absent.data = Arc::new(serde_json::json!({"health": {"available": true}}));
        assert!(!recording(&absent));
    }
    #[test]
    fn elapsed_reads_like_the_qml_clock() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(65), "1:05");
        assert_eq!(elapsed(3599), "59:59");
        assert_eq!(elapsed(3661), "1:01:01");
        assert_eq!(
            values(false, 99)["bar.recording.elapsed"],
            Value::Text(String::new())
        );
        assert_eq!(values(true, 5)["bar.recording"], Value::Bool(true));
    }
    #[test]
    fn the_ticker_starts_with_the_recording_and_stops_with_it() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let publish = move |values: BTreeMap<String, Value>| sink.lock().unwrap().push(values);
        let mut indicator = Indicator::new();
        indicator.follow(&snapshot(true, false), publish.clone());
        assert!(seen.lock().unwrap().is_empty() && indicator.running.is_none());
        indicator.follow(&snapshot(true, true), publish.clone());
        indicator.follow(&snapshot(true, true), publish.clone());
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(seen.lock().unwrap()[0]["bar.recording"], Value::Bool(true));
        assert_eq!(
            seen.lock().unwrap()[0]["bar.recording.elapsed"],
            Value::Text("0:00".into())
        );
        indicator.follow(&snapshot(true, false), publish);
        assert!(indicator.running.is_none());
        let last = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last["bar.recording"], Value::Bool(false));
        assert_eq!(last["bar.recording.elapsed"], Value::Text(String::new()));
    }
}
