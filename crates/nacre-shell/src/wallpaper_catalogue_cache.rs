// SPDX-License-Identifier: GPL-3.0-only
//! Small persistent metadata snapshot for the wallpaper picker cold-open path.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

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
    load_at(&path())
}

fn valid_entry(entry: &Value) -> bool {
    entry["id"]
        .as_str()
        .is_some_and(|id| !id.is_empty() && id.len() <= 4096)
        && matches!(entry["kind"].as_str(), Some("image" | "video"))
        && entry["title"].as_str().is_some()
        && entry["thumbnail"].as_str().is_some()
        && entry["preview"].as_str().is_some()
        && entry["animated"].is_boolean()
        && entry["active"].is_boolean()
}

fn load_at(path: &Path) -> Option<Vec<Value>> {
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return None;
    }
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if snapshot.version != VERSION
        || snapshot.entries.len() > 10_000
        || snapshot.entries.iter().any(|entry| !valid_entry(entry))
    {
        return None;
    }
    Some(snapshot.entries)
}

pub(crate) fn store(entries: &[Value]) {
    store_at(&path(), entries)
}

fn store_at(destination: &Path, entries: &[Value]) {
    if entries.len() > 10_000 || entries.iter().any(|entry| !valid_entry(entry)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bead-wallpaper-catalogue-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn entry() -> Value {
        json!({"id":"/pictures/lake.png","kind":"image","animated":false,"title":"Lake",
            "thumbnail":"/cache/lake.jpg","preview":"","active":true})
    }

    #[test]
    fn snapshot_round_trips_valid_entries() {
        let directory = temp_dir();
        let path = directory.join("catalogue.json");
        let entries = vec![entry()];
        store_at(&path, &entries);
        assert_eq!(load_at(&path), Some(entries));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn snapshot_rejects_wrong_version_and_invalid_schema() {
        let directory = temp_dir();
        let path = directory.join("catalogue.json");
        fs::write(
            &path,
            serde_json::to_vec(&Snapshot {
                version: VERSION + 1,
                entries: vec![entry()],
            })
            .unwrap(),
        ).unwrap();
        assert!(load_at(&path).is_none());
        fs::write(
            &path,
            serde_json::to_vec(&Snapshot {
                version: VERSION,
                entries: vec![json!({"id":"x","kind":"other"})],
            })
            .unwrap(),
        ).unwrap();
        assert!(load_at(&path).is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn snapshot_rejects_oversize_files_and_symlinks() {
        let directory = temp_dir();
        let oversized = directory.join("oversized.json");
        fs::File::create(&oversized).unwrap().set_len(LIMIT + 1).unwrap();
        assert!(load_at(&oversized).is_none());
        #[cfg(unix)]
        {
            let target = directory.join("target.json");
            store_at(&target, &[entry()]);
            let link = directory.join("link.json");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(load_at(&link).is_none());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn store_ignores_invalid_entries() {
        let directory = temp_dir();
        let path = directory.join("catalogue.json");
        store_at(&path, &[json!({"id":"x","kind":"wrong"})]);
        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
