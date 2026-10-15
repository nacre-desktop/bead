// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded regular-file snapshots and coalesced parent-directory watches.
use std::{
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::mpsc,
};
pub fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    if limit > 8 * 1024 * 1024 {
        return Err("file limit exceeds8MiB".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("not a regular file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("file byte limit exceeded".into());
    }
    Ok(bytes)
}
pub fn write_json(path: &Path, value: &serde_json::Value, limit: usize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("file byte limit exceeded".into());
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| e.to_string())
}
pub struct Watch {
    path: PathBuf,
    limit: usize,
    last: Option<serde_json::Value>,
    events: mpsc::Receiver<()>,
    _watcher: notify::RecommendedWatcher,
}
impl Watch {
    pub fn new(path: PathBuf, limit: usize) -> Result<Self, String> {
        use notify::Watcher;
        let (tx, events) = mpsc::sync_channel(1);
        let watched = path.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event
                    && !event.kind.is_access()
                    && (event.need_rescan()
                        || event.paths.is_empty()
                        || event.paths.contains(&watched))
                {
                    let _ = tx.try_send(());
                }
            })
            .map_err(|e| e.to_string())?;
        watcher
            .watch(
                path.parent().ok_or("missing file parent")?,
                notify::RecursiveMode::NonRecursive,
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            limit,
            last: None,
            events,
            _watcher: watcher,
        })
    }
    pub fn refresh(
        &mut self,
        validate: impl Fn(&serde_json::Value) -> Result<(), String>,
    ) -> Result<&serde_json::Value, String> {
        let value =
            serde_json::from_slice(&read(&self.path, self.limit)?).map_err(|e| e.to_string())?;
        validate(&value)?;
        self.last = Some(value);
        self.last.as_ref().ok_or("no valid snapshot".into())
    }
    pub fn changed(&self) -> bool {
        self.events.try_recv().is_ok()
    }
    pub fn last_good(&self) -> Option<&serde_json::Value> {
        self.last.as_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_watch_retains_last_good_after_invalid_write() {
        let dir = std::env::temp_dir().join(format!("bead-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        write_json(&path, &serde_json::json!({"version":1}), 4096).unwrap();
        let mut watch = Watch::new(path.clone(), 4096).unwrap();
        watch
            .refresh(|v| {
                if v["version"] == 1 {
                    Ok(())
                } else {
                    Err("version".into())
                }
            })
            .unwrap();
        write_json(&path, &serde_json::json!({"version":2}), 4096).unwrap();
        let start = std::time::Instant::now();
        while !watch.changed() {
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(watch.refresh(|_| Err("wrong version".into())).is_err());
        assert_eq!(watch.last_good().unwrap()["version"], 1);
        assert!(read(&path, 2).is_err());
        drop(watch);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
