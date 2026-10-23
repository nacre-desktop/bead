// SPDX-License-Identifier: GPL-3.0-only
//! Local connection status is separate from the last-good external topic data.
use bead::core::source::{self, Publisher};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

pub(super) struct Connection {
    pub(super) publisher: Publisher,
    since: Mutex<Option<Instant>>,
}
impl Connection {
    pub(super) fn new(epoch: String, now: Instant) -> source::Result<Self> {
        Ok(Self {
            publisher: Publisher::new(
                epoch,
                serde_json::json!({"connected":false,"reconnecting":false}),
            )?,
            since: Mutex::new(Some(now)),
        })
    }
    pub(super) fn connected(&self) {
        *self.since.lock().unwrap_or_else(|p| p.into_inner()) = None;
        let _ = self
            .publisher
            .publish(serde_json::json!({"connected":true,"reconnecting":false}));
    }
    pub(super) fn disconnected(&self, now: Instant) {
        let since = *self
            .since
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_or_insert(now);
        let mark = now.saturating_duration_since(since) >= Duration::from_secs(2);
        let _ = self
            .publisher
            .publish(serde_json::json!({"connected":false,"reconnecting":mark}));
    }
    // Polled only while connecting/backing off. One delayed state change per
    // outage; once visible there is no periodic status wakeup or extra task.
    pub(super) async fn delayed_mark(&self) {
        let since = *self.since.lock().unwrap_or_else(|p| p.into_inner());
        if self.publisher.snapshot().data["reconnecting"] == true || since.is_none() {
            return std::future::pending().await;
        }
        if let Some(since) = since {
            tokio::time::sleep_until((since + Duration::from_secs(2)).into()).await;
            self.disconnected(Instant::now());
        }
    }
}
/// Watch only an owned private directory. Socket creation interrupts reconnect
/// backoff without polling or another thread; SO_PEERCRED checks remain in connect.
struct Descriptor(nix::sys::inotify::Inotify);
impl std::os::fd::AsRawFd for Descriptor {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsFd;
        self.0.as_fd().as_raw_fd()
    }
}
pub(super) struct SocketWatch(Option<tokio::io::unix::AsyncFd<Descriptor>>);
impl SocketWatch {
    pub(super) fn new(socket: &std::path::Path) -> Self {
        use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
        use std::os::unix::fs::MetadataExt;
        let watch = || -> Option<_> {
            let mut parent = socket.parent()?;
            loop {
                match std::fs::symlink_metadata(parent) {
                    Ok(meta) => {
                        if !meta.is_dir()
                            || meta.uid() != nix::unistd::Uid::current().as_raw()
                            || meta.mode() & 0o077 != 0
                        {
                            return None;
                        }
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        parent = parent.parent()?
                    }
                    Err(_) => return None,
                }
            }
            let fd = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC).ok()?;
            fd.add_watch(
                parent,
                AddWatchFlags::IN_CREATE
                    | AddWatchFlags::IN_MOVED_TO
                    | AddWatchFlags::IN_DELETE_SELF
                    | AddWatchFlags::IN_MOVE_SELF,
            )
            .ok()?;
            // SAFETY: `Descriptor` owns the inotify descriptor, so it stays open and
            // unchanged for the whole life of the `AsyncFd`.
            unsafe { tokio::io::unix::AsyncFd::register(Descriptor(fd)) }.ok()
        };
        Self(watch())
    }
    pub(super) async fn changed(&self) {
        let Some(fd) = &self.0 else {
            return std::future::pending().await;
        };
        let result = fd
            .async_io(tokio::io::Interest::READABLE, |inner| {
                inner
                    .0
                    .read_events()
                    .map(|_| ())
                    .map_err(std::io::Error::from)
            })
            .await;
        if result.is_err() {
            std::future::pending::<()>().await;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_mark_is_once_per_outage_and_recovery_resets_it() {
        let now = Instant::now();
        let c = Connection::new("local-link".into(), now).unwrap();
        c.disconnected(now + Duration::from_millis(1999));
        assert_eq!(c.publisher.snapshot().data["reconnecting"], false);
        c.disconnected(now + Duration::from_secs(2));
        assert_eq!(c.publisher.snapshot().data["reconnecting"], true);
        let revision = c.publisher.snapshot().revision;
        c.disconnected(now + Duration::from_secs(20));
        assert_eq!(c.publisher.snapshot().revision, revision);
        c.connected();
        assert_eq!(
            c.publisher.snapshot().data,
            std::sync::Arc::new(serde_json::json!({"connected":true,"reconnecting":false}))
        );
        c.disconnected(now + Duration::from_secs(21));
        assert_eq!(c.publisher.snapshot().data["reconnecting"], false);
        c.disconnected(now + Duration::from_secs(23));
        assert_eq!(c.publisher.snapshot().data["reconnecting"], true);
    }
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn private_socket_creation_wakes_without_waiting_for_backoff() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bead-link-watch-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.join("daemon.sock");
        let watch = SocketWatch::new(&socket);
        assert!(watch.0.is_some());
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        tokio::time::timeout(Duration::from_millis(500), watch.changed())
            .await
            .unwrap();
        drop(listener);
        drop(watch);
        std::fs::remove_file(socket).unwrap();
        std::fs::remove_dir(dir).unwrap();
        assert!(
            SocketWatch::new(std::path::Path::new("/tmp/not-a-private-watch.sock"))
                .0
                .is_none()
        );
    }
}
