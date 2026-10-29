// SPDX-License-Identifier: MIT OR Apache-2.0
//! Opt-in numeric timing; never records text, keys or user content.
use std::sync::OnceLock;
/// Per-frame/per-commit diagnostics (`PROFILE`, `UI_COMMIT`, `BUFFER`, ...) are
/// off in production so normal use writes nothing per frame. Harnesses opt in
/// with `BEAD_VERBOSE_LOG=1` (or any `BEAD_PROFILE_INPUT`).
pub fn verbose() -> bool {
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    *VERBOSE.get_or_init(|| {
        std::env::var_os("BEAD_VERBOSE_LOG")
            .is_some_and(|v| v.as_os_str() != std::ffi::OsStr::new("0"))
            || enabled()
    })
}
/// Per-rendered-frame timing (`UI_FRAME`); harness-only, via `BEAD_FRAME_TRACE=1`.
pub fn frame_trace() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE.get_or_init(|| {
        std::env::var_os("BEAD_FRAME_TRACE")
            .is_some_and(|v| v.as_os_str() != std::ffi::OsStr::new("0"))
    })
}
/// `println!` that only writes when [`verbose`] logging is enabled.
#[macro_export]
macro_rules! vprintln {
    ($($arg:tt)*) => {
        if $crate::profile::verbose() {
            println!($($arg)*);
        }
    };
}
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BEAD_PROFILE_INPUT").is_some())
}
// Keep exact input/presentation clocks usable without per-stage logging cost.
fn stages_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("BEAD_PROFILE_INPUT")
            .is_some_and(|value| value.as_os_str() != std::ffi::OsStr::new("clocks"))
    })
}
pub fn clock(id: libc::clockid_t) -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(id, &mut time) } == 0 {
        time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
    } else {
        0
    }
}
pub fn monotonic_ns() -> u64 {
    clock(libc::CLOCK_MONOTONIC)
}
pub fn process_cpu_ns() -> u64 {
    clock(libc::CLOCK_PROCESS_CPUTIME_ID)
}
/// CPU time of the calling thread: unaffected by suspend, scheduling or CPU starvation.
pub fn thread_cpu_ns() -> u64 {
    clock(libc::CLOCK_THREAD_CPUTIME_ID)
}
pub struct Span {
    stage: &'static str,
    start: Option<(u64, u64)>,
}
impl Span {
    pub fn new(stage: &'static str) -> Self {
        Self {
            stage,
            start: stages_enabled().then(|| (monotonic_ns(), clock(libc::CLOCK_THREAD_CPUTIME_ID))),
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((wall, cpu)) = self.start {
            println!(
                "INPUT_PROFILE stage={} wall_ms={:.4} cpu_ms={:.4}",
                self.stage,
                monotonic_ns().saturating_sub(wall) as f64 / 1e6,
                clock(libc::CLOCK_THREAD_CPUTIME_ID).saturating_sub(cpu) as f64 / 1e6
            );
        }
    }
}

/// Opt-in cross-thread timeline; no clock read or allocation when profiling is off.
pub fn mark(stage: &'static str) {
    if enabled() {
        println!(
            "UI_TIMELINE stage={stage} monotonic_ns={} thread={:?}",
            monotonic_ns(),
            std::thread::current().id()
        );
    }
}

/// Opt-in numeric pointer positions associate coalesced input with rendered updates.
pub fn position(stage: &'static str, x: f32, y: f32) {
    if enabled() {
        println!(
            "UI_POINTER stage={stage} x={x:.3} y={y:.3} monotonic_ns={} thread={:?}",
            monotonic_ns(),
            std::thread::current().id()
        );
    }
}

/// B0205: main-thread attribution under [`frame_trace`]. A guard charges its own
/// (exclusive) wall and thread-CPU time to `stage`; [`busy_iteration`] prints the
/// loop iterations that spent at least `BEAD_FRAME_TRACE_BUSY_MS` (default 4) of
/// either. Nothing is read or allocated when tracing is off.
pub struct Busy(bool);
struct BusyFrame {
    stage: std::borrow::Cow<'static, str>,
    wall: u64,
    cpu: u64,
    child_wall: u64,
    child_cpu: u64,
}
#[derive(Default)]
struct BusyState {
    stack: Vec<BusyFrame>,
    totals: Vec<(std::borrow::Cow<'static, str>, u64, u64, u32)>,
    last_cpu: u64,
    marks: Vec<(&'static str, u64)>,
}
thread_local! {
    static BUSY: std::cell::RefCell<BusyState> = Default::default();
}
impl Busy {
    pub fn new(stage: &'static str) -> Self {
        Self::named(|| stage.into())
    }
    /// A stage whose label is only built when tracing is on.
    pub fn named(stage: impl FnOnce() -> std::borrow::Cow<'static, str>) -> Self {
        if !frame_trace() {
            return Self(false);
        }
        let frame = BusyFrame {
            stage: stage(),
            wall: monotonic_ns(),
            cpu: thread_cpu_ns(),
            child_wall: 0,
            child_cpu: 0,
        };
        BUSY.with(|b| b.borrow_mut().stack.push(frame));
        Self(true)
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        let (wall, cpu) = (monotonic_ns(), thread_cpu_ns());
        BUSY.with(|b| {
            let mut b = b.borrow_mut();
            let Some(frame) = b.stack.pop() else {
                return;
            };
            let (w, c) = (wall - frame.wall, cpu.saturating_sub(frame.cpu));
            if let Some(parent) = b.stack.last_mut() {
                parent.child_wall += w;
                parent.child_cpu += c;
            }
            let (own_w, own_c) = (
                w.saturating_sub(frame.child_wall),
                c.saturating_sub(frame.child_cpu),
            );
            if let Some(total) = b.totals.iter_mut().find(|t| t.0 == frame.stage) {
                total.1 += own_w;
                total.2 += own_c;
                total.3 += 1;
            } else {
                b.totals.push((frame.stage, own_w, own_c, 1));
            }
        });
    }
}
/// Call once per event-loop iteration (after dispatch) on the thread that uses [`Busy`].
pub fn busy_iteration() {
    if !frame_trace() {
        return;
    }
    static THRESHOLD_NS: OnceLock<u64> = OnceLock::new();
    let threshold = *THRESHOLD_NS.get_or_init(|| {
        std::env::var("BEAD_FRAME_TRACE_BUSY_MS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map_or(4_000_000, |ms| (ms * 1e6) as u64)
    });
    let cpu = thread_cpu_ns();
    BUSY.with(|b| {
        let mut b = b.borrow_mut();
        let previous = std::mem::replace(&mut b.last_cpu, cpu);
        let spent = cpu.saturating_sub(previous);
        let wall: u64 = b.totals.iter().map(|t| t.1).sum();
        if previous != 0 && (spent >= threshold || wall >= threshold) {
            let attributed: u64 = b.totals.iter().map(|t| t.2).sum();
            let mut totals = std::mem::take(&mut b.totals);
            totals.sort_by_key(|t| std::cmp::Reverse(t.1.max(t.2)));
            let stages = totals
                .iter()
                .map(|(stage, w, c, n)| {
                    format!("{stage}:{:.2}/{:.2}x{n}", *w as f64 / 1e6, *c as f64 / 1e6)
                })
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "MAIN_BUSY monotonic_ns={} cpu_ms={:.2} attributed_wall_ms={:.2} other_cpu_ms={:.2} stages={stages}",
                monotonic_ns(),
                spent as f64 / 1e6,
                wall as f64 / 1e6,
                spent.saturating_sub(attributed) as f64 / 1e6
            );
        } else {
            b.totals.clear();
        }
    });
}
/// Per-frame split points for `UI_FRAME` (wall clock, [`frame_trace`] only).
pub fn split(stage: &'static str) {
    if frame_trace() {
        let now = monotonic_ns();
        BUSY.with(|b| b.borrow_mut().marks.push((stage, now)));
    }
}
/// The splits since `begin` as `stage:ms` deltas, clearing them; empty when tracing is off.
pub fn take_splits(begin: u64) -> String {
    if !frame_trace() {
        return String::new();
    }
    BUSY.with(|b| {
        let mut previous = begin;
        let marks = std::mem::take(&mut b.borrow_mut().marks);
        marks
            .iter()
            .map(|(stage, at)| {
                let delta = at.saturating_sub(previous);
                previous = *at;
                format!("{stage}:{:.3}", delta as f64 / 1e6)
            })
            .collect::<Vec<_>>()
            .join(",")
    })
}
/// One timeline line (`PANEL_TRACE`) under [`frame_trace`], with the caller's numeric fields.
pub fn trace(stage: &'static str, fields: impl FnOnce() -> String) {
    if frame_trace() {
        println!(
            "PANEL_TRACE stage={stage} monotonic_ns={} thread={:?} {}",
            monotonic_ns(),
            std::thread::current().id(),
            fields()
        );
    }
}
/// The leading identifier of a `Debug` rendering (an enum variant's name) without
/// formatting the rest of the value.
pub fn variant_name(value: &dyn std::fmt::Debug) -> String {
    struct Head(String);
    impl std::fmt::Write for Head {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let end = s
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(s.len());
            self.0.push_str(&s[..end]);
            if end < s.len() {
                Err(std::fmt::Error)
            } else {
                Ok(())
            }
        }
    }
    let mut head = Head(String::new());
    let _ = std::fmt::write(&mut head, format_args!("{value:?}"));
    head.0
}
#[cfg(test)]
mod tests {
    #[test]
    fn variant_name_stops_at_the_first_field() {
        #[derive(Debug)]
        #[allow(dead_code)]
        enum Event {
            Plain,
            Tuple(String),
            Fields { x: u32 },
        }
        assert_eq!(super::variant_name(&Event::Plain), "Plain");
        assert_eq!(super::variant_name(&Event::Tuple("x".repeat(9))), "Tuple");
        assert_eq!(super::variant_name(&Event::Fields { x: 1 }), "Fields");
    }
}
