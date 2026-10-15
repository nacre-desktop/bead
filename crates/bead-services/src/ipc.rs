// SPDX-License-Identifier: MIT OR Apache-2.0
//! Versioned, owner-only, bounded Unix command transport. No display discovery.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 64 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub version: u16,
    pub id: u64,
    pub target: String,
    pub method: String,
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(default)]
    pub output: Option<String>,
}
impl Call {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("unsupported protocol version".into());
        }
        if self.id == 0
            || self.target.len() > 256
            || self.method.len() > 128
            || self.args.len() > 16
            || self.target.contains('\0')
            || self.method.contains('\0')
        {
            return Err("invalid command envelope".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u16,
    pub id: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub result: Option<Value>,
    pub error: Option<Fault>,
}
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
impl Reply {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            version: VERSION,
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn error(id: u64, code: &str, message: impl ToString) -> Self {
        Self {
            version: VERSION,
            id,
            result: None,
            error: Some(Fault {
                code: code.into(),
                message: message.to_string().chars().take(1024).collect(),
            }),
        }
    }
}
fn ready(fd: i32, events: i16, deadline: Instant) -> Result<(), String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("command deadline exceeded".into());
    }
    let mut p = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    let n = unsafe {
        libc::poll(
            &mut p,
            1,
            remaining.as_millis().min(i32::MAX as u128) as i32,
        )
    };
    if n < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if n == 0 {
        return Err("command deadline exceeded".into());
    }
    Ok(())
}
fn receive(stream: &mut UnixStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        ready(stream.as_raw_fd(), libc::POLLIN, deadline)?;
        match stream.read(&mut buffer) {
            Ok(0) => return Err("command peer closed without newline".into()),
            Ok(n) => {
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.len() > MAX_FRAME {
                    return Err("command frame exceeds64KiB".into());
                }
                if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
                    if end + 1 != bytes.len() {
                        return Err("one request per connection".into());
                    }
                    bytes.pop();
                    return Ok(bytes);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}
fn send(stream: &mut UnixStream, value: &impl Serialize, deadline: Instant) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err("command reply exceeds64KiB".into());
    }
    let mut offset = 0;
    while offset < bytes.len() {
        ready(stream.as_raw_fd(), libc::POLLOUT, deadline)?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err("command peer stopped reading".into()),
            Ok(n) => offset += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
fn owner(stream: &UnixStream) -> Result<(), String> {
    let mut cred = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ok = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            cred.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if ok != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if unsafe { cred.assume_init().uid != libc::geteuid() } {
        return Err("command peer has a different owner".into());
    }
    Ok(())
}
pub struct Server {
    path: PathBuf,
    identity: (u64, u64),
    stop: UnixStream,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn bind(
        path: PathBuf,
        handler: impl Fn(Call) -> Reply + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let parent = path.parent().ok_or("socket parent missing")?;
        let meta = std::fs::metadata(parent).map_err(|e| e.to_string())?;
        if meta.uid() != unsafe { libc::geteuid() } || meta.permissions().mode() & 0o077 != 0 {
            return Err("socket parent must be owner-only".into());
        }
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                let stale = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !stale.file_type().is_socket()
                    || stale.uid() != unsafe { libc::geteuid() }
                    || stale.permissions().mode() & 0o077 != 0
                {
                    return Err("command path is not an owner-only socket".into());
                }
                match connect(&path) {
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                    _ => return Err("command socket already active or unavailable".into()),
                }
                let current = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if (current.dev(), current.ino()) != (stale.dev(), stale.ino()) {
                    return Err("command socket changed during recovery".into());
                }
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                UnixListener::bind(&path).map_err(|e| e.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        let identity = (meta.dev(), meta.ino());
        let (stop, mut wake) = UnixStream::pair().map_err(|e| e.to_string())?;
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            loop {
                let mut fds = [
                    libc::pollfd {
                        fd: listener.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: wake.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                if n < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
                if fds[1].revents != 0 {
                    let mut b = [0];
                    let _ = wake.read(&mut b);
                    break;
                }
                if fds[0].revents == 0 {
                    continue;
                }
                if let Ok((mut stream, _)) = listener.accept() {
                    if owner(&stream).is_err() || stream.set_nonblocking(true).is_err() {
                        continue;
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    let reply = match receive(&mut stream, deadline)
                        .and_then(|b| serde_json::from_slice::<Call>(&b).map_err(|e| e.to_string()))
                    {
                        Ok(call) => match call.validate() {
                            Ok(()) => handler(call),
                            Err(e) => Reply::error(call.id, "invalid-request", e),
                        },
                        Err(e) => Reply::error(0, "invalid-request", e),
                    };
                    let _ = send(&mut stream, &reply, Instant::now() + Duration::from_secs(1));
                }
            }
        });
        Ok(Self {
            path,
            identity,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
pub(crate) fn connect(path: &Path) -> std::io::Result<UnixStream> {
    use std::os::{fd::FromRawFd, unix::ffi::OsStrExt};
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as _;
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid command socket path",
        ));
    }
    for (dst, src) in address.sun_path.iter_mut().zip(bytes) {
        *dst = *src as _;
    }
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    let connected = unsafe {
        libc::connect(
            fd,
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    if connected != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(stream)
}
pub fn call(path: &Path, request: &Call, timeout: Duration) -> Result<Reply, String> {
    request.validate()?;
    if timeout > Duration::from_secs(10) {
        return Err("invalid command timeout".into());
    }
    let mut stream = connect(path).map_err(|e| e.to_string())?;
    owner(&stream)?;
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    send(&mut stream, request, deadline)?;
    let reply: Reply =
        serde_json::from_slice(&receive(&mut stream, deadline)?).map_err(|e| e.to_string())?;
    if reply.id != request.id
        || reply.version != VERSION
        || reply.result.is_some() == reply.error.is_some()
    {
        return Err("invalid command reply correlation".into());
    }
    Ok(reply)
}
#[cfg(test)]
mod tests {
    #[test]
    fn null_success_remains_distinct_from_absent_result() {
        let success = super::Reply::ok(1, serde_json::Value::Null);
        let decoded: super::Reply =
            serde_json::from_slice(&serde_json::to_vec(&success).unwrap()).unwrap();
        assert_eq!(decoded.result, Some(serde_json::Value::Null));
        assert!(decoded.error.is_none());
        let error = super::Reply::error(1, "fixture", "failed");
        let decoded: super::Reply =
            serde_json::from_slice(&serde_json::to_vec(&error).unwrap()).unwrap();
        assert!(decoded.result.is_none());
        assert!(decoded.error.is_some());
    }
    use super::*;
    #[test]
    fn recovers_owned_dead_listener_without_replacing_regular_file() {
        let dir = std::env::temp_dir().join(format!("bead-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("command.sock");
        let dead = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        drop(dead);
        let server = Server::bind(path.clone(), |c| Reply::ok(c.id, Value::Null)).unwrap();
        drop(server);
        std::fs::write(&path, "preserve").unwrap();
        assert!(Server::bind(path.clone(), |c| Reply::ok(c.id, Value::Null)).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "preserve");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn owner_socket_version_errors_and_cleanup() {
        let dir = std::env::temp_dir().join(format!("bead-ipc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("c.sock");
        let server = Server::bind(path.clone(), |c| {
            Reply::ok(
                c.id,
                serde_json::json!({"target":c.target,"method":c.method,"args":c.args}),
            )
        })
        .unwrap();
        assert!(Server::bind(path.clone(), |c| Reply::ok(c.id, Value::Null)).is_err());
        let request = Call {
            version: 1,
            id: 7,
            target: "header-fixture".into(),
            method: "state".into(),
            args: vec![],
            output: Some("fixture".into()),
        };
        let reply = call(&path, &request, Duration::from_secs(1)).unwrap();
        assert_eq!(reply.result.unwrap()["target"], "header-fixture");
        let mut bad = request;
        bad.version = 2;
        assert!(call(&path, &bad, Duration::from_secs(1)).is_err());
        drop(server);
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
