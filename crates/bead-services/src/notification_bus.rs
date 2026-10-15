// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hostable notification service. The host injects its connection and history path.
//! `Server` is the compatibility wrapper that creates a private bus and runtime.
use crate::notifications::{Center, Notice};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Notify, mpsc, oneshot, watch};
const PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";
struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[derive(Clone, Debug)]
pub enum Control {
    Action(u32, String),
    Dismiss(u32),
    Hover(u32, bool),
    Dnd(bool),
    HidePopups,
    Details(u32),
    ClearHistory,
    Osd {
        channel: String,
        value: f64,
        muted: bool,
    },
    Stop,
}
struct Shared {
    closing: AtomicBool,
    center: Mutex<Center>,
    changed: Notify,
}
impl Shared {
    fn with<T>(&self, f: impl FnOnce(&mut Center) -> Result<T, String>) -> zbus::fdo::Result<T> {
        let mut center = self
            .center
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("notification state poisoned".into()))?;
        if self.closing.load(Ordering::Acquire) {
            return Err(zbus::fdo::Error::Failed(
                "notification service stopped".into(),
            ));
        }
        f(&mut center).map_err(zbus::fdo::Error::InvalidArgs)
    }
}
struct Interface(Arc<Shared>);
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Interface {
    fn get_capabilities(&self) -> Vec<&str> {
        vec!["actions", "body", "persistence"]
    }
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Bead", "Nacre", "0.1.0", "1.2")
    }
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        if !actions.len().is_multiple_of(2) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "actions need key/label pairs".into(),
            ));
        }
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1)
            .min(2);
        let transient = hints
            .get("transient")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let resident = hints
            .get("resident")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let actions = actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| (p[0].clone(), p[1].clone()))
            .collect();
        let id = self.0.with(|c| {
            c.notify(
                Notice {
                    id: 0,
                    app: app_name,
                    icon: app_icon,
                    summary,
                    body,
                    actions,
                    urgency,
                    transient,
                    resident,
                    popup: false,
                    expanded: false,
                    timeout_ms: 0,
                    time_unix_ms: 0,
                    deadline: None,
                    remaining: None,
                    hovered: false,
                },
                replaces_id,
                expire_timeout,
                Instant::now(),
            )
        })?;
        self.0.changed.notify_one();
        Ok(id)
    }
    async fn close_notification(
        &self,
        id: u32,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        self.0.with(|c| c.close(id).map(|_| ()))?;
        signal(connection, "NotificationClosed", &(id, 3u32))
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        self.0.changed.notify_one();
        Ok(())
    }
    #[zbus(signal)]
    async fn notification_closed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn action_invoked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}
struct Policy(Arc<Shared>, mpsc::Sender<Request>);
#[zbus::interface(name = "org.nacre.Bead.Notifications")]
impl Policy {
    fn set_do_not_disturb(&self, value: bool) -> zbus::fdo::Result<()> {
        self.0.with(|c| {
            c.set_dnd(value);
            Ok(())
        })?;
        self.0.changed.notify_one();
        Ok(())
    }
    fn inhibit(
        &self,
        reason: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let owner = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::Failed("missing owner".into()))?
            .to_string();
        let id = self.0.with(|c| c.inhibit(owner, reason))?;
        self.0.changed.notify_one();
        Ok(id)
    }
    fn uninhibit(
        &self,
        cookie: u32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let owner = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::Failed("missing owner".into()))?
            .to_string();
        self.0.with(|c| c.uninhibit(&owner, cookie))?;
        self.0.changed.notify_one();
        Ok(())
    }
    fn get_history(&self) -> zbus::fdo::Result<String> {
        self.0
            .with(|c| serde_json::to_string(&c.retained()).map_err(|e| e.to_string()))
    }
    fn invoke_action(&self, id: u32, key: String) -> zbus::fdo::Result<()> {
        self.0.with(|c| c.action(id, &key).map(|_| ()))?;
        self.1
            .try_send(Request::new(Control::Action(id, key)))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    fn dismiss(&self, id: u32) -> zbus::fdo::Result<()> {
        self.1
            .try_send(Request::new(Control::Dismiss(id)))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    fn clear_history(&self) -> zbus::fdo::Result<()> {
        self.1
            .try_send(Request::new(Control::ClearHistory))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    fn show_osd(&self, channel: String, value: f64, muted: bool) -> zbus::fdo::Result<()> {
        if !["volume", "brightness", "microphone", "capslock"].contains(&channel.as_str())
            || !value.is_finite()
            || !(0. ..=1.).contains(&value)
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "invalid OSD channel/level".into(),
            ));
        }
        self.1
            .try_send(Request::new(Control::Osd {
                channel,
                value,
                muted,
            }))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
    #[zbus(property)]
    fn inhibited(&self) -> zbus::fdo::Result<bool> {
        self.0.with(|c| Ok(c.suppressed()))
    }
}
async fn signal<T: serde::Serialize + zbus::zvariant::DynamicType>(
    connection: &zbus::Connection,
    name: &str,
    value: &T,
) -> zbus::Result<()> {
    connection
        .emit_signal(None::<&str>, PATH, INTERFACE, name, value)
        .await
}
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}
fn time_label(time: u64) -> String {
    let seconds = wall_ms().saturating_sub(time) / 1000;
    if seconds < 60 {
        "now".into()
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}
/// No connection discovery, bus spawning, runtime or thread creation occurs here.
/// Call from the host's existing Tokio runtime (including current-thread).
pub struct Config {
    pub connection: zbus::Connection,
    pub history: PathBuf,
}
struct Request {
    action: Control,
    reply: Option<oneshot::Sender<Result<(), String>>>,
}
impl Request {
    fn new(action: Control) -> Self {
        Self {
            action,
            reply: None,
        }
    }
}
/// Latest-wins subscriptions retain one bounded snapshot, never a queue of updates.
pub struct Handle {
    sender: mpsc::Sender<Request>,
    snapshots: watch::Receiver<Value>,
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
    stop: Arc<Notify>,
}
impl Handle {
    pub fn snapshot(&self) -> Value {
        self.snapshots.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Value> {
        self.snapshots.clone()
    }
    /// A successful result means the action was processed, not merely enqueued.
    pub async fn call(&self, action: Control) -> Result<(), String> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .try_send(Request {
                action,
                reply: Some(reply),
            })
            .map_err(|_| "notification service busy or stopped")?;
        tokio::time::timeout(Duration::from_secs(3), receive)
            .await
            .map_err(|_| "notification call timed out")?
            .map_err(|_| "notification service stopped")?
    }
    pub async fn stop(mut self) -> Result<(), String> {
        self.stop.notify_one();
        if let Some(task) = self.task.take() {
            task.await.map_err(|e| e.to_string())??;
        }
        Ok(())
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // Nonblocking drop also works when a host cancels the owning service task.
        self.stop.notify_one();
    }
}
pub async fn start(config: Config) -> Result<Handle, String> {
    let (sender, receiver) = mpsc::channel(64);
    let (publish, snapshots) = watch::channel(Value::Null);
    let (ready, answer) = oneshot::channel();
    let tx = sender.clone();
    let stop = Arc::new(Notify::new());
    let stopped = stop.clone();
    let task = tokio::spawn(async move {
        serve(
            config.connection,
            &config.history,
            move |value| {
                publish.send_replace(value);
            },
            tx,
            receiver,
            ready,
            stopped,
        )
        .await
    });
    let mut handle = Handle {
        sender,
        snapshots,
        task: Some(task),
        stop,
    };
    match answer.await {
        Ok(Ok(())) => Ok(handle),
        result => {
            handle.stop.notify_one();
            let task = handle
                .task
                .take()
                .ok_or("notification startup task missing")?;
            match task.await {
                Ok(Err(error)) => Err(error),
                Err(error) => Err(error.to_string()),
                _ => Err(format!("notification startup: {result:?}")),
            }
        }
    }
}

/// Explicit private bus used by fixtures and the transitional in-process shell.
/// Never discovers or connects to the user's session bus.
pub struct PrivateBus {
    bus: Bus,
    socket: PathBuf,
    pub address: String,
}
impl PrivateBus {
    pub fn start(socket: PathBuf) -> Result<Self, String> {
        let parent = socket.parent().ok_or("bus socket needs parent")?;
        let meta = std::fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !meta.is_dir()
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err("private bus needs an owned mode0700 directory and unused socket".into());
        }
        let daily = PathBuf::from(format!("/run/user/{}/bus", unsafe { libc::geteuid() }));
        if socket == daily
            || std::env::var_os("XDG_RUNTIME_DIR")
                .is_some_and(|d| socket == PathBuf::from(d).join("bus"))
        {
            return Err("the session bus path is forbidden".into());
        }
        if let Ok(old) = std::fs::symlink_metadata(&socket) {
            if !old.file_type().is_socket() || old.uid() != unsafe { libc::geteuid() } {
                return Err("private bus path is occupied".into());
            }
            match crate::ipc::connect(&socket) {
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                _ => return Err("private bus already active or unavailable".into()),
            }
            let current = std::fs::symlink_metadata(&socket).map_err(|e| e.to_string())?;
            if old.dev() != current.dev() || old.ino() != current.ino() {
                return Err("private bus path changed during recovery".into());
            }
            std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
        }
        if socket.as_os_str().as_encoded_bytes().len() > 90 {
            return Err("private bus socket path too long".into());
        }

        let parent_pid = std::process::id() as libc::pid_t;
        // The owned bus must also die if this process/thread is killed.
        let mut bus = Bus(unsafe {
            crate::process::command("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .arg(format!("--address=unix:path={}", socket.display()))
                .pre_exec(move || {
                    if libc::prctl(
                        libc::PR_SET_PDEATHSIG,
                        libc::SIGTERM as libc::c_ulong,
                        0 as libc::c_ulong,
                        0 as libc::c_ulong,
                        0 as libc::c_ulong,
                    ) != 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != parent_pid {
                        libc::_exit(127);
                    }
                    Ok(())
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?
        });
        let stdout = bus.0.stdout.take().ok_or("private bus stdout missing")?;
        let fd = stdout.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut reader = BufReader::new(stdout);
        let mut address = String::new();
        let started = Instant::now();
        while !address.ends_with('\n') {
            if started.elapsed() > Duration::from_secs(4) || address.len() > 1024 {
                return Err("private bus address deadline/size limit".into());
            }
            match reader.read_line(&mut address) {
                Ok(0) => return Err("private bus exited before its address".into()),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut poll = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    unsafe {
                        libc::poll(&mut poll, 1, 50);
                    }
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        let address = address.trim().to_owned();
        Ok(PrivateBus {
            bus,
            socket: socket.to_owned(),
            address,
        })
    }
    pub async fn connect(&self) -> Result<zbus::Connection, String> {
        zbus::connection::Builder::address(self.address.as_str())
            .map_err(|e| e.to_string())?
            .build()
            .await
            .map_err(|e| e.to_string())
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.bus.0.kill();
        let _ = self.bus.0.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}
pub struct Server {
    sender: mpsc::Sender<Request>,
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<Notify>,
    pub address: String,
}
impl Server {
    /// Host API processing acknowledgement on the caller's existing Tokio runtime.
    pub async fn call(&self, action: Control) -> Result<(), String> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .try_send(Request {
                action,
                reply: Some(reply),
            })
            .map_err(|_| "notification service busy or stopped")?;
        tokio::time::timeout(Duration::from_secs(3), receive)
            .await
            .map_err(|_| "notification call timed out")?
            .map_err(|_| "notification service stopped")?
    }
    pub fn control(&self, command: Control) -> Result<(), String> {
        self.sender
            .try_send(Request::new(command))
            .map_err(|e| e.to_string())
    }
    pub fn start(
        socket: PathBuf,
        history: PathBuf,
        callback: impl Fn(Value) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        Self::start_with_lifecycle(socket, history, callback, || {})
    }
    pub fn start_with_lifecycle(
        socket: PathBuf,
        history: PathBuf,
        callback: impl Fn(Value) + Send + Sync + 'static,
        on_stop: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let (ready, answer) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("bead-notifications".into())
            .spawn(move || {
                let result = (|| -> Result<(), String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .max_blocking_threads(2)
                        .build()
                        .map_err(|e| e.to_string())?;
                    let bus = PrivateBus::start(socket)?;
                    runtime.block_on(async {
                        let service = start(Config {
                            connection: bus.connect().await?,
                            history,
                        })
                        .await?;
                        let mut updates = service.subscribe();
                        callback(service.snapshot());
                        ready
                            .send(Ok((
                                bus.address.clone(),
                                service.sender.clone(),
                                service.stop.clone(),
                            )))
                            .map_err(|_| "notification host cancelled startup")?;
                        while updates.changed().await.is_ok() {
                            callback(updates.borrow_and_update().clone());
                        }
                        service.stop().await
                    })
                })();
                on_stop();
                if let Err(error) = result {
                    let _ = ready.send(Err(error.clone()));
                    eprintln!("NOTIFICATION_ERROR {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        match answer.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok((address, sender, stop))) => Ok(Self {
                sender,
                thread: Some(thread),
                stop,
                address,
            }),
            result => {
                let _ = thread.join();
                Err(format!("notification bus startup: {result:?}"))
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        // Ensure stop is delivered even if the bounded command queue is full.
        self.stop.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
async fn serve(
    connection: zbus::Connection,
    history: &Path,
    callback: impl Fn(Value) + Send + Sync + 'static,
    sender: mpsc::Sender<Request>,
    mut receiver: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), String>>,
    stop: Arc<Notify>,
) -> Result<(), String> {
    let mut center = Center::default();
    let mut history_writable = true;
    let mut history_error = String::new();
    if history.exists() {
        match crate::files::read(history, 4 * 1024 * 1024)
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
            .and_then(|value| center.restore_state(value))
        {
            Ok(()) => {}
            Err(e) => {
                history_writable = false;
                history_error =
                    "Notification history could not be read; protected from overwrite".into();
                eprintln!("NOTIFICATION_HISTORY_ERROR preserving unreadable file: {e}");
            }
        }
    }
    let shared = Arc::new(Shared {
        closing: AtomicBool::new(false),
        center: Mutex::new(center),
        changed: Notify::new(),
    });
    let (mut installed, mut policy_installed, mut owned) = (false, false, false);
    let result = async {
    if !connection.object_server().at(PATH, Interface(shared.clone())).await.map_err(|e| e.to_string())? {
        return Err("notification interface already installed".into());
    }
    installed = true;
    if !connection.object_server().at(PATH, Policy(shared.clone(), sender)).await.map_err(|e| e.to_string())? {
        return Err("notification policy already installed".into());
    }
    policy_installed = true;
    // Never replace an existing server, even if that server permits replacement.
    let ownership = connection.request_name_with_flags(INTERFACE, zbus::fdo::RequestNameFlags::DoNotQueue.into()).await.map_err(|e| e.to_string())?;
    if ownership != zbus::fdo::RequestNameReply::PrimaryOwner { return Err("notification name already owned".into()); }
    owned = true;
    let dbus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|e| e.to_string())?;
    let mut owners = dbus
        .receive_name_owner_changed()
        .await
        .map_err(|e| e.to_string())?;
    let mut osd = Value::Null;
    let mut osd_deadline = None;
    let mut persisted = String::new();
    let mut last_inhibited = None;
    let mut ready = Some(ready);
    loop {
        let (snapshot, deadline, history_json) = {
            let c = shared
                .center
                .lock()
                .map_err(|_| "notification state poisoned")?;
            let mut visible: Vec<_> = c
                .active
                .values()
                .filter(|n| n.popup)
                .chain(c.exiting.values().map(|(n, _)| n))
                .collect();
            visible.sort_by_key(|n| n.id);
            let mut rows:Vec<_>=visible.into_iter().rev().take(5).map(|n|json!({"id":n.id.to_string(),"app":n.app,"summary":n.summary,"body":n.body,"icon":n.icon,"urgency":n.urgency,"expanded":n.expanded,"time_str":time_label(n.time_unix_ms),"actions":n.actions.iter().filter(|(key,_)|key!="default").map(|(key,label)|json!({"id":format!("{}|{}",n.id,key),"label":label})).collect::<Vec<_>>()})).collect();
            rows.reverse();
            (
                json!({"rows":rows,"dnd":c.dnd,"inhibited":c.suppressed(),"history_ready":history_writable,"error":history_error,"history_count":c.history.len()+c.active.values().filter(|n|!n.transient).count(),"popup_count":c.active.values().filter(|n|n.popup).count(),"timers":c.active.values().filter(|n|n.popup).map(|n|json!({"hovered":n.hovered,"timeoutMs":n.timeout_ms,"intervalMs":n.timeout_ms,"running":n.deadline.is_some()})).collect::<Vec<_>>(),"osd":osd}),
                c.deadline()
                    .into_iter()
                    .chain(c.active.values().filter(|n| n.popup).map(|n| {
                        Instant::now()
                            + Duration::from_millis(
                                60_000 - wall_ms().saturating_sub(n.time_unix_ms) % 60_000,
                            )
                    }))
                    .min(),
                c.persisted()?,
            )
        };
        let inhibited = snapshot["inhibited"].as_bool().unwrap_or(false);
        if last_inhibited != Some(inhibited) {
            let changed =
                HashMap::from([("Inhibited", zbus::zvariant::OwnedValue::from(inhibited))]);
            connection
                .emit_signal(
                    None::<&str>,
                    PATH,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(
                        "org.nacre.Bead.Notifications",
                        changed,
                        Vec::<String>::new(),
                    ),
                )
                .await
                .map_err(|e| e.to_string())?;
            last_inhibited = Some(inhibited);
        }
        callback(snapshot);
        if let Some(ready) = ready.take() { let _ = ready.send(Ok(())); }
        let encoded = history_json.to_string();
        if history_writable && encoded != persisted {
            if let Err(e) = crate::files::write_json(history, &history_json, 4 * 1024 * 1024) {
                eprintln!("NOTIFICATION_HISTORY_ERROR {e}");
                if history_error != e {
                    history_error = e;
                    shared.changed.notify_one();
                }
            } else if !history_error.is_empty() {
                history_error.clear();
                shared.changed.notify_one();
            }
            persisted = encoded;
        }
        let deadline = deadline.into_iter().chain(osd_deadline).min();
        let sleep = async {
            match deadline {
                Some(d) => tokio::time::sleep_until(tokio::time::Instant::from_std(d)).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _=connection.closed()=>return Err("notification bus disconnected".into()),
            _=stop.notified()=>break,
            _=shared.changed.notified()=>{},
            _=sleep=>{
                let expired=shared.with(|c|Ok(c.expire(Instant::now()))).map_err(|e|e.to_string())?;
                for id in expired {signal(&connection,"NotificationClosed",&(id,1u32)).await.map_err(|e|e.to_string())?;}
                if osd_deadline.is_some_and(|d|d<=Instant::now()){osd=Value::Null;osd_deadline=None;}
            },
            Some(change)=owners.next()=>{if let Ok(args)=change.args() && args.new_owner().as_ref().is_none(){let _=shared.with(|c|{c.owner_left(args.name().as_str());Ok(())});}},
            request=receiver.recv()=> {
                let Some(request) = request else { break; };
                let stop = matches!(request.action, Control::Stop);
                let result = apply(&shared, &connection, request.action, &mut osd, &mut osd_deadline).await;
                if let Some(reply) = request.reply { let _ = reply.send(result); }
                if stop { break; }
            }

        }
    }
    Ok(())
    }.await;
    // A Notify can race a stop request after the previous persistence pass.
    // Close admission under the same lock, then flush every accepted notice.
    shared.closing.store(true, Ordering::Release);
    let final_state = match shared.center.lock() {
        Ok(center) => center.persisted(),
        Err(_) => Err("notification state poisoned".into()),
    };
    let flushed = if history_writable && owned {
        final_state.and_then(|value| crate::files::write_json(history, &value, 4 * 1024 * 1024))
    } else {
        Ok(())
    };
    if installed {
        let _ = connection
            .object_server()
            .remove::<Interface, _>(PATH)
            .await;
    }
    if policy_installed {
        let _ = connection.object_server().remove::<Policy, _>(PATH).await;
    }
    if owned {
        let _ = connection.release_name(INTERFACE).await;
    }
    result.and(flushed)
}
async fn apply(
    shared: &Shared,
    connection: &zbus::Connection,
    action: Control,
    osd: &mut Value,
    osd_deadline: &mut Option<Instant>,
) -> Result<(), String> {
    match action {
        Control::Stop => {}
        Control::Action(id, key) => {
            let close = shared
                .with(|c| c.action(id, &key))
                .map_err(|e| e.to_string())?;
            signal(connection, "ActionInvoked", &(id, key))
                .await
                .map_err(|e| e.to_string())?;
            if close {
                shared
                    .with(|c| c.close(id).map(|_| ()))
                    .map_err(|e| e.to_string())?;
                signal(connection, "NotificationClosed", &(id, 2u32))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        Control::Dismiss(id) => {
            shared.with(|c| c.close(id)).map_err(|e| e.to_string())?;
            signal(connection, "NotificationClosed", &(id, 2u32))
                .await
                .map_err(|e| e.to_string())?;
        }
        Control::Hover(id, value) => {
            shared
                .with(|c| c.hover(id, value, Instant::now()))
                .map_err(|e| e.to_string())?;
        }
        Control::Dnd(value) => {
            shared
                .with(|c| {
                    c.set_dnd(value);
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Control::HidePopups => {
            shared
                .with(|c| {
                    c.hide_popups();
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Control::Details(id) => {
            shared
                .with(|c| {
                    let n = c.active.get_mut(&id).ok_or("unknown notification")?;
                    n.expanded = !n.expanded;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
        }
        Control::ClearHistory => {
            let ids = shared
                .with(|c| {
                    let ids: Vec<_> = c
                        .active
                        .values()
                        .filter(|n| !n.transient)
                        .map(|n| n.id)
                        .collect();
                    for id in &ids {
                        c.active.remove(id);
                    }
                    c.history.clear();
                    c.exiting.clear();
                    Ok(ids)
                })
                .map_err(|e| e.to_string())?;
            for id in ids {
                signal(connection, "NotificationClosed", &(id, 2u32))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        Control::Osd {
            channel,
            value,
            muted,
        } => {
            if !["volume", "brightness", "microphone", "capslock"].contains(&channel.as_str())
                || !value.is_finite()
                || !(0. ..=1.).contains(&value)
            {
                return Err("invalid OSD channel/level".into());
            }
            *osd = json!({"channel":channel,"value":value,"muted":muted});
            *osd_deadline = Some(Instant::now() + Duration::from_secs(2));
        }
    }
    Ok(())
}
#[cfg(test)]
mod shutdown_tests {
    use super::*;
    #[test]
    fn stop_flushes_mutation_after_last_publish_and_failed_start_cannot_write()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("bead-final-flush-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let bus = PrivateBus::start(root.join("bus"))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(2)
            .build()?;
        runtime.block_on(async {
            let connection = bus.connect().await?;
            let history = root.join("history.json");
            let service = start(Config {
                connection: connection.clone(),
                history: history.clone(),
            })
            .await?;
            // Simulate an accepted mutation after the last publish, with Stop next in queue.
            let interface = connection
                .object_server()
                .interface::<_, Interface>(PATH)
                .await?;
            interface.get().await.0.with(|c| {
                c.set_dnd(true);
                Ok(())
            })?;
            let prior = std::fs::read(&history)?;
            let inode = std::fs::metadata(&history)?.ino();
            assert!(
                start(Config {
                    connection: connection.clone(),
                    history: history.clone()
                })
                .await
                .is_err()
            );
            assert!(
                std::fs::read(&history)? == prior,
                "failed competing start overwrote owner history"
            );
            assert_eq!(
                std::fs::metadata(&history)?.ino(),
                inode,
                "failed start replaced owner file"
            );
            service.stop().await?;
            let saved: Value = serde_json::from_slice(&std::fs::read(history)?)?;
            assert_eq!(saved["dnd"], true);
            assert!(interface.get().await.0.with(|_| Ok(())).is_err());
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
