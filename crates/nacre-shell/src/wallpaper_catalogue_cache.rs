// SPDX-License-Identifier: GPL-3.0-only
//! Small persistent metadata snapshot for the wallpaper picker cold-open path.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io::Write, path::PathBuf, time::SystemTime};

const LIMIT: u64 = 4 * 1024 * 1024;
const VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct Snapshot {
    version: u32,
    entries: Vec<Value>,
}

fn path() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("bead/wallpaper-catalogue-v1.json")
}

pub(crate) fn load() -> Option<Vec<Value>> {
    let path = path();
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return None;
    }
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if snapshot.version != VERSION
        || snapshot.entries.len() > 10_000
        || snapshot.entries.iter().any(|entry| {
            entry["id"].as_str().is_none_or(str::is_empty)
                || entry["kind"].as_str().is_none()
        })
    {
        return None;
    }
    Some(snapshot.entries)
}

pub(crate) fn store(entries: &[Value]) {
    if entries.len() > 10_000
        || entries.iter().any(|entry| {
            entry["id"].as_str().is_none_or(str::is_empty)
                || entry["kind"].as_str().is_none()
        })
    {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(&Snapshot {
        version: VERSION,
        entries: entries.to_vec(),
    }) else {
        return;
    };
    if bytes.len() as u64 > LIMIT {
        return;
    }
    let destination = path();
    let Some(parent) = destination.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let temporary = parent.join(format!(
        ".wallpaper-catalogue-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let Ok(mut file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    let written = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    if written.is_ok() && fs::rename(&temporary, &destination).is_ok() {
        return;
    }
    let _ = fs::remove_file(temporary);
}
