// SPDX-License-Identifier: MIT
//! Atomic JSON document storage with process locking and revision compare-and-swap.
use std::{
    io,
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 65_536;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub struct LayoutStore {
    path: PathBuf,
}
impl LayoutStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn read(&self) -> io::Result<(serde_json::Value, u64)> {
        self.read_with_report(|_| {})
    }
    /// Reads a document while reporting lock or revision fallback errors to the host.
    pub fn read_with_report(
        &self,
        mut report: impl FnMut(&io::Error),
    ) -> io::Result<(serde_json::Value, u64)> {
        match lock(&self.path) {
            Ok(_guard) => {
                let (document, bytes) = read_document(&self.path)?;
                let revision = current_revision(&self.path, &bytes).unwrap_or_else(|error| {
                    report(&error);
                    stamp_revision(&self.path).unwrap_or(0)
                });
                Ok((document, revision))
            }
            Err(error) => {
                report(&error);
                read_document(&self.path)
                    .map(|(document, _)| (document, stamp_revision(&self.path).unwrap_or(0)))
            }
        }
    }
    pub fn write_revisioned(&self, document: &serde_json::Value, expected: u64) -> io::Result<u64> {
        let _lock = lock(&self.path)?;
        write_locked(&self.path, document, Some(expected))
    }
    pub fn write(&self, document: &serde_json::Value) -> io::Result<u64> {
        let _lock = lock(&self.path)?;
        write_locked(&self.path, document, None)
    }
}
struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("document path needs a file name"))?;
    Ok(path.with_file_name(format!("{}{}", name.to_string_lossy(), suffix)))
}
fn lock(path: &Path) -> io::Result<Lock> {
    use std::os::fd::AsRawFd;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("document path needs a parent"))?;
    std::fs::create_dir_all(parent)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(sibling(path, ".lock")?)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Lock(file))
}
fn hash(bytes: &[u8]) -> String {
    let mut value = 0xcbf29ce484222325u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}
fn stamp_revision(path: &Path) -> io::Result<u64> {
    let stamp: serde_json::Value = serde_json::from_slice(&std::fs::read(sibling(path, ".rev")?)?)?;
    stamp["rev"]
        .as_u64()
        .ok_or_else(|| io::Error::other("invalid revision stamp"))
}
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("document path needs a parent"))?;
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".document-{}-{next}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
fn read_document(path: &Path) -> io::Result<(serde_json::Value, Vec<u8>)> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::other("document exceeds 64KiB"));
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    if !doc.is_object() {
        return Err(io::Error::other("document must be an object"));
    }
    Ok((doc, bytes))
}
fn current_revision(path: &Path, bytes: &[u8]) -> io::Result<u64> {
    let stamp_path = sibling(path, ".rev")?;
    let digest = hash(bytes);
    let (revision, next) = match std::fs::read(&stamp_path) {
        Ok(raw) => {
            let stamp: serde_json::Value = serde_json::from_slice(&raw)?;
            let rev = stamp["rev"]
                .as_u64()
                .ok_or_else(|| io::Error::other("invalid revision stamp"))?;
            if stamp["hash"].as_str() == Some(digest.as_str()) {
                (rev, None)
            } else {
                (rev.saturating_add(1), Some(rev.saturating_add(1)))
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (0, Some(0)),
        Err(e) => return Err(e),
    };
    if let Some(rev) = next {
        atomic_replace(
            &stamp_path,
            &serde_json::to_vec(&serde_json::json!({"hash":digest,"rev":rev}))?,
        )?;
    }
    Ok(revision)
}
fn write_locked(
    path: &Path,
    document: &serde_json::Value,
    expected: Option<u64>,
) -> io::Result<u64> {
    let bytes = serde_json::to_vec(document)?;
    if !document.is_object() || bytes.len() > MAX_BYTES {
        return Err(io::Error::other("document must be an object under 64KiB"));
    }
    let old = match read_document(path) {
        Ok((_, bytes)) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let current = if let Some(bytes) = old.as_ref() {
        current_revision(path, bytes)?
    } else {
        match stamp_revision(path) {
            Ok(rev) => {
                let rev = rev.saturating_add(1);
                atomic_replace(
                    &sibling(path, ".rev")?,
                    &serde_json::to_vec(&serde_json::json!({"hash":"","rev":rev}))?,
                )?;
                rev
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
            Err(e) => return Err(e),
        }
    };
    if let Some(expected) = expected.filter(|expected| *expected != current) {
        return Err(io::Error::other(format!(
            "layout revision conflict: expected {expected}, found {current}"
        )));
    }
    let revision = current.saturating_add(1);
    atomic_replace(path, &bytes)?;
    let stamp_path = sibling(path, ".rev")?;
    let stamp = serde_json::to_vec(&serde_json::json!({"hash":hash(&bytes),"rev":revision}))?;
    if let Err(error) = atomic_replace(&stamp_path, &stamp) {
        if let Some(old) = old {
            let _ = atomic_replace(path, &old);
        } else {
            let _ = std::fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fresh_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bead-widgets-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn opaque_document_round_trips_and_malformed_edits_are_rejected() {
        let parent = fresh_dir("roundtrip");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("layout.json");
        let doc = json!({"version":1,"future":{"widget":"unknown","data":[1,2]}});
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(LayoutStore::new(&path).read().unwrap().0, doc);
        std::fs::write(&path, b"{broken").unwrap();
        assert!(LayoutStore::new(&path).read().is_err());
        std::fs::write(&path, b"[]").unwrap();
        assert!(LayoutStore::new(&path).read().is_err());
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn invalid_write_keeps_previous_document() {
        let parent = fresh_dir("atomic");
        let path = parent.join("layout.json");
        let store = LayoutStore::new(&path);
        let doc = json!({"version":1,"future":{"opaque":[1,2]}});
        store.write(&doc).unwrap();
        assert!(store.write(&json!([])).is_err());
        assert_eq!(store.read().unwrap().0, doc);
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn revisions_serialize_writers_detect_stale_edits_and_track_raw_changes() {
        let parent = fresh_dir("revisions");
        let path = parent.join("layout.json");
        let store = LayoutStore::new(&path);
        let first = json!({"version":1,"layout":"first","opaque":{"x":1}});
        let second = json!({"version":1,"layout":"second","opaque":{"x":1}});
        let alternate = json!({"version":1,"layout":"alternate","opaque":{"x":1}});
        let third = json!({"version":1,"layout":"third","opaque":{"x":1}});
        assert_eq!(store.write_revisioned(&first, 0).unwrap(), 1);
        assert_eq!(store.read().unwrap().1, 1);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            serde_json::to_vec(&first).unwrap()
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let writer = |document: serde_json::Value, barrier: std::sync::Arc<std::sync::Barrier>| {
            let path = path.clone();
            std::thread::spawn(move || {
                barrier.wait();
                LayoutStore::new(path).write_revisioned(&document, 1)
            })
        };
        let writer_a = writer(second.clone(), barrier.clone());
        let writer_b = writer(alternate.clone(), barrier.clone());
        barrier.wait();
        let result_a = writer_a.join().unwrap();
        let result_b = writer_b.join().unwrap();
        assert_ne!(
            result_a.is_ok(),
            result_b.is_ok(),
            "only one writer may use a stale revision"
        );
        assert!(
            store
                .write_revisioned(&third, 1)
                .unwrap_err()
                .to_string()
                .contains("revision conflict")
        );
        assert_eq!(store.read().unwrap().1, 2);
        let winner = store.read().unwrap().0;
        assert!(winner == second || winner == alternate);
        std::fs::write(&path, serde_json::to_vec(&third).unwrap()).unwrap();
        let reopened = store.read().unwrap();
        assert_eq!(reopened.1, 3);
        assert_eq!(reopened.0, third);
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn unreadable_revision_sidecar_preserves_read_and_fails_closed_on_write() {
        let parent = fresh_dir("bad-stamp");
        let path = parent.join("layout.json");
        let doc = json!({"version":1,"widget":"known"});
        std::fs::create_dir_all(parent.join("layout.json.rev")).unwrap();
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let store = LayoutStore::new(&path);
        assert_eq!(store.read().unwrap().0, doc);
        assert!(store.write_revisioned(&json!({"version":1}), 0).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            doc
        );
        std::fs::remove_dir_all(parent).unwrap();
    }
}
