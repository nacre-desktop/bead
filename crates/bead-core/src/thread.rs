// SPDX-License-Identifier: MIT OR Apache-2.0
//! Background work that must not compete with the UI thread for CPU or power.
/// Spawn `work` on a named thread at a lower scheduling priority (nice +10 on Linux), for
/// work done ahead of when it is needed; callers poll `is_finished` and never wait on it.
pub fn spawn_background<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<T>> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            #[cfg(target_os = "linux")]
            // SAFETY: setpriority on the calling thread's own id has no memory preconditions;
            // failure only keeps the default priority.
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 10);
            }
            work()
        })
}
