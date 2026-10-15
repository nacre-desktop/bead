// SPDX-License-Identifier: MIT OR Apache-2.0
//! Numeric-only diagnostics: no titles, output names, argv or environment.
use super::*;
pub(super) fn save(app: &App, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let usage = unsafe { usage.assume_init() };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    let surfaces: Vec<_> = app
        .report_surfaces
        .iter()
        .cloned()
        .chain(app.surfaces.iter().map(surface))
        .collect();
    let value = serde_json::json!({"version":1,"duration_seconds":app.started.elapsed().as_secs_f64(),"cpu_seconds":seconds(usage.ru_utime)+seconds(usage.ru_stime),"surfaces":surfaces,"cache_bytes":bead_core::cache::report()});
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

pub(super) fn surface(s: &Surface) -> serde_json::Value {
    let samples = |v: &[f64]| {
        v.iter()
            .copied()
            .filter(|v| v.is_finite())
            .collect::<Vec<_>>()
    };
    serde_json::json!({"panel":s.companion,"frames":s.frames,"processing_ms":samples(&s.frame_ms),"presentation_ms":samples(&s.presented_ms),"callback_ms":samples(&s.callbacks_ms)})
}
