// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded UI recovery data. Reports contain fixed codes, never source/user text.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static SERIAL: AtomicU64 = AtomicU64::new(0);
const MAX_FILE: u64 = 32 * 1024 * 1024;
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|p| matches!(p, Component::Normal(_)))
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("recovery file exceeds budget".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let parent = path.parent().ok_or("recovery parent missing")?;
    std::fs::create_dir_all(parent)?;
    let meta = std::fs::symlink_metadata(parent)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("recovery parent must be directory".into());
    }
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    let temporary = parent.join(format!(
        ".ui-{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        serde_json::to_writer(&mut file, value)?;
        file.flush()?;
        if file.metadata()?.len() > MAX_FILE {
            return Err("encoded recovery exceeds budget".into());
        }
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub version: u32,
    pub entry: PathBuf,
    pub palette: Vec<u8>,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}
impl Bundle {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1
            || !relative(&self.entry)
            || !self.files.contains_key(&self.entry)
            || self.files.len() > 512
            || self.palette.len() > 1024 * 1024
            || self.files.iter().any(|(p, b)| {
                !relative(p)
                    || p == Path::new(".bead-recovery-palette.json")
                    || b.len() > 2 * 1024 * 1024
            })
            || self.files.values().map(Vec::len).sum::<usize>() + self.palette.len()
                > 8 * 1024 * 1024
        {
            return Err("invalid or oversized recovery bundle".into());
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        write(path, self)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = read(path)?;
        value.validate()?;
        Ok(value)
    }
    pub fn restore(&self, parent: &Path) -> Result<Restored> {
        use std::os::unix::fs::DirBuilderExt;
        self.validate()?;
        // Reap only this cache's dead-process restore directories, never live owners.
        use std::os::unix::fs::MetadataExt;
        for item in std::fs::read_dir(parent)? {
            let item = item?;
            let name = item.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((pid, serial)) = name
                .strip_prefix(".restored-")
                .and_then(|n| n.split_once('-'))
            else {
                continue;
            };
            let (Ok(pid), Ok(_)) = (pid.parse::<i32>(), serial.parse::<u64>()) else {
                continue;
            };
            let meta = std::fs::symlink_metadata(item.path())?;
            // SAFETY: signal zero probes liveness without delivering a signal.
            if pid > 0
                && meta.is_dir()
                && meta.uid() == unsafe { libc::geteuid() }
                && unsafe { libc::kill(pid, 0) } < 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                let _ = std::fs::remove_dir_all(item.path());
            }
        }
        let path = parent.join(format!(
            ".restored-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        let mut restored = Restored { path };
        restored.path = restored.path.canonicalize()?;
        for (name, bytes) in self.files.iter().chain(std::iter::once((
            &PathBuf::from(".bead-recovery-palette.json"),
            &self.palette,
        ))) {
            let target = restored.path.join(name);
            std::fs::create_dir_all(target.parent().ok_or("bundle parent missing")?)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(target)?;
            file.write_all(bytes)?;
        }
        Ok(restored)
    }
}
pub struct Restored {
    pub path: PathBuf,
}
impl Drop for Restored {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    version: u32,
    failures: Vec<u64>,
    running: Option<u64>,
}
pub struct Run {
    path: PathBuf,
    history: History,
    pub safe_mode: bool,
    lock: std::fs::File,
    finished: bool,
}
impl Run {
    pub fn begin(path: PathBuf, now_ms: u64) -> Result<Self> {
        use std::os::fd::AsRawFd;
        let parent = path.parent().ok_or("crash history parent missing")?;
        std::fs::create_dir_all(parent)?;
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path.with_extension("lock"))?;
        // SAFETY: a live owned descriptor, nonblocking exclusive process lock.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("UI crash history already owned".into());
        }
        let mut history = match read::<History>(&path) {
            Ok(h) if h.version == 1 && h.failures.len() <= 3 => h,
            Ok(_) => return Err("invalid crash history".into()),
            Err(e) if !path.exists() => {
                let _ = e;
                History {
                    version: 1,
                    ..Default::default()
                }
            }
            Err(e) => return Err(e),
        };
        if let Some(started) = history.running.take() {
            history.failures.push(started);
        }
        history
            .failures
            .retain(|t| now_ms.saturating_sub(*t) <= 120_000);
        if history.failures.len() > 3 {
            history.failures.drain(..history.failures.len() - 3);
        }
        let safe_mode = history.failures.len() >= 3;
        history.running = Some(now_ms);
        write(&path, &history)?;
        Ok(Self {
            path,
            history,
            safe_mode,
            lock,
            finished: false,
        })
    }
    pub fn finish(&mut self) -> Result<()> {
        use std::os::fd::AsRawFd;
        if self.finished {
            return Ok(());
        }
        self.history.running = None;
        write(&self.path, &self.history)?;
        // SAFETY: release only this run's descriptor lock after its durable clean marker.
        if unsafe { libc::flock(self.lock.as_raw_fd(), libc::LOCK_UN) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.finished = true;
        Ok(())
    }
}
/// Pure deadline policy, also used by deterministic headless replay tests.
pub fn stalled(last_ns: u64, now_ns: u64, limit_ns: u64) -> bool {
    now_ns.saturating_sub(last_ns) > limit_ns
}
pub struct Watchdog {
    heartbeat: Arc<AtomicU64>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    origin: Instant,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Watchdog {
    pub fn start(timeout: Duration) -> std::io::Result<Self> {
        if timeout.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "zero watchdog timeout",
            ));
        }
        // Core images could contain private UI/source data. Preserve the hard limit.
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid initialized limit pointer; affects only this UI process.
        if unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        limit.rlim_cur = 0;
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let origin = Instant::now();
        let heartbeat = Arc::new(AtomicU64::new(0));
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let beat = heartbeat.clone();
        let stopped = stop.clone();
        let limit = timeout.as_nanos().min(u64::MAX as u128) as u64;
        let thread = std::thread::Builder::new()
            .name("bead-ui-watchdog".into())
            .spawn(move || {
                let mut lock = match stopped.0.lock() {
                    Ok(lock) => lock,
                    Err(_) => return,
                };
                loop {
                    if *lock {
                        break;
                    }
                    let now = origin.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                    let last = beat.load(Ordering::Relaxed);
                    if stalled(last, now, limit) {
                        eprintln!(
                            "WATCHDOG code=main_loop_timeout stalled_ms={} monotonic_ns={}",
                            now.saturating_sub(last) / 1_000_000,
                            crate::profile::monotonic_ns()
                        );
                        std::process::abort();
                    }
                    let wait =
                        Duration::from_nanos(limit.saturating_sub(now.saturating_sub(last)).max(1));
                    match stopped.1.wait_timeout(lock, wait) {
                        Ok((next, _)) => lock = next,
                        Err(_) => return,
                    }
                }
            })?;
        Ok(Self {
            heartbeat,
            stop,
            origin,
            thread: Some(thread),
        })
    }
    pub fn beat(&self) {
        self.heartbeat.store(
            self.origin.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        if let Ok(mut stopped) = self.stop.0.lock() {
            *stopped = true;
            self.stop.1.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
