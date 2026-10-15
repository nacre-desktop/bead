// SPDX-License-Identifier: MIT OR Apache-2.0
//! Opt-in numeric timing; never records text, keys or user content.
use std::sync::OnceLock;
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BEAD_PROFILE_INPUT").is_some())
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
pub struct Span {
    stage: &'static str,
    start: Option<(u64, u64)>,
}
impl Span {
    pub fn new(stage: &'static str) -> Self {
        Self {
            stage,
            start: enabled().then(|| (monotonic_ns(), clock(libc::CLOCK_THREAD_CPUTIME_ID))),
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
