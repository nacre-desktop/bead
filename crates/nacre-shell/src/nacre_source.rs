// SPDX-License-Identifier: GPL-3.0-only
//! Nacre v1 transport stays in the product. One link multiplexes topic Sources.
use bead::core::source::{
    self, Applied, Callback, Error, Mirror, Publisher, Snapshot, Source, Subscription,
};
use nacre_protocol::{Id, Message, Reply, framing};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::BufReader,
    net::UnixStream,
    sync::{Notify, mpsc, oneshot},
    task::JoinSet,
};

mod connection;
mod leases;
#[cfg(test)]
use leases::LeasedTopic;

type Action = Arc<dyn Fn(String, Value) -> source::Reply<'static> + Send + Sync>;
type Event = Arc<dyn Fn(&Message) + Send + Sync>;
#[derive(Clone)]
pub struct Config {
    pub socket: PathBuf,
    pub topics: Vec<String>,
    pub client: String,
    pub provider: Option<(String, BTreeMap<String, u16>, Action)>,
    pub event: Option<Event>,
}
impl Config {
    pub fn new(socket: PathBuf, topics: Vec<String>) -> Self {
        Self {
            socket,
            topics,
            client: "nacre-shell".into(),
            provider: None,
            event: None,
        }
    }
}
#[derive(Default, Clone)]
pub struct Status {
    pub boot: Option<String>,
    pub methods: BTreeMap<String, nacre_protocol::Method>,
}
struct Shared {
    link: Publisher,
    connection: connection::Connection,
    topics: BTreeMap<String, Publisher>,
    leases: leases::Registry,
    status: Mutex<Status>,
    stop: AtomicBool,
    notify: Notify,
    ended: AtomicBool,
    transport_generation: AtomicU64,
    closed: Notify,
}
impl Shared {
    fn publisher(&self, name: &str) -> Option<Publisher> {
        self.topics
            .get(name)
            .cloned()
            .or_else(|| self.leases.publisher(name))
    }
    fn publishers(&self) -> Vec<Publisher> {
        self.topics
            .values()
            .cloned()
            .chain(self.leases.publishers())
            .collect()
    }
}
struct OnExit(Arc<Shared>);
impl Drop for OnExit {
    fn drop(&mut self) {
        if let Ok(mut status) = self.0.status.lock() {
            *status = Status::default();
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.0.link.disconnect()));
        for publisher in self.0.publishers() {
            let _ =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| publisher.disconnect()));
        }
        self.0.ended.store(true, Ordering::Release);
        self.0.closed.notify_waiters();
    }
}
struct Owner {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Command>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.notify.notify_one();
        if let Ok(slot) = self.thread.get_mut()
            && let Some(thread) = slot.take()
            && thread.thread().id() != std::thread::current().id()
        {
            let _ = thread.join();
        }
    }
}
#[derive(Clone)]
pub struct Client(Arc<Owner>);
pub struct Topic {
    client: Client,
    publisher: Publisher,
}
struct Pending {
    reply: CallReply,
    deadline: Instant,
    trace_menu: bool,
}
enum Command {
    Call {
        id: Option<Id>,
        method: String,
        params: Value,
        deadline: Instant,
        reply: CallReply,
        ticket: Option<TicketDispatch>,
    },
    Reconnect(oneshot::Sender<source::Result<Value>>),
    Stop(oneshot::Sender<source::Result<Value>>),
    #[cfg(test)]
    Wire(Message, oneshot::Sender<source::Result<Value>>),
    #[cfg(test)]
    AbortProviderTasks(oneshot::Sender<()>),
    #[cfg(test)]
    Gate {
        entered: oneshot::Sender<()>,
        release: oneshot::Receiver<()>,
    },
}
const TICKET_QUEUED: u8 = 0;
const TICKET_DISPATCHED: u8 = 1;
const TICKET_CANCELLED: u8 = 2;
pub type CallWake = Arc<dyn Fn() + Send + Sync>;
#[derive(Clone, Debug)]
pub struct CallGuard {
    pub boot: String,
    pub version: u16,
}
struct TicketDispatch {
    state: Arc<std::sync::atomic::AtomicU8>,
    generation: u64,
    guard: CallGuard,
}
enum CallReply {
    Async(oneshot::Sender<source::Result<Value>>),
    Ticket(TicketCompletion),
}
impl CallReply {
    fn finish(self, result: source::Result<Value>) {
        match self {
            Self::Async(reply) => {
                let _ = reply.send(result);
            }
            Self::Ticket(reply) => reply.finish(result),
        }
    }
}
struct TicketCompletion {
    sender: Option<oneshot::Sender<source::Result<Value>>>,
    wake: Option<CallWake>,
}
impl TicketCompletion {
    fn wake(&mut self) {
        if let Some(wake) = self.wake.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wake()));
        }
    }
    fn finish(mut self, result: source::Result<Value>) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
        self.wake();
    }
}
impl Drop for TicketCompletion {
    fn drop(&mut self) {
        self.sender.take();
        self.wake();
    }
}
pub struct CallTicket {
    receiver: oneshot::Receiver<source::Result<Value>>,
    state: Arc<std::sync::atomic::AtomicU8>,
    done: bool,
}
impl CallTicket {
    pub fn try_take(&mut self) -> Option<source::Result<Value>> {
        if self.done {
            return None;
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.done = true;
                Some(result)
            }
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => {
                self.done = true;
                Some(Err(unavailable()))
            }
        }
    }
}
impl Drop for CallTicket {
    fn drop(&mut self) {
        let _ = self.state.compare_exchange(
            TICKET_QUEUED,
            TICKET_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
fn ticket_guard_matches(
    ticket: &TicketDispatch,
    generation: u64,
    boot: &str,
    methods: &BTreeMap<String, nacre_protocol::Method>,
    method: &str,
) -> bool {
    ticket.generation == generation
        && boot == ticket.guard.boot
        && methods
            .get(method)
            .is_some_and(|advertised| advertised.v == ticket.guard.version)
}
fn claim_ticket_dispatch(state: &std::sync::atomic::AtomicU8) -> bool {
    state
        .compare_exchange(
            TICKET_QUEUED,
            TICKET_DISPATCHED,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}
fn unavailable() -> Error {
    Error::new(
        "unavailable",
        "daemon disconnected; calls are not replayed",
        true,
    )
}
fn timeout() -> Error {
    Error::new(
        "timeout",
        "call deadline reached; re-read before retry",
        true,
    )
}
fn failure(err: nacre_protocol::Failure) -> Error {
    let code = serde_json::to_value(err.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "protocol".into());
    Error::new(&code, &err.message, err.retry)
}
fn check_socket(path: &std::path::Path) -> source::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if !path.is_absolute() {
        return Err(Error::new(
            "permission_denied",
            "daemon socket must be absolute",
            false,
        ));
    }
    let parent = path.parent().ok_or_else(unavailable)?;
    let uid = nix::unistd::geteuid().as_raw();
    for (p, directory) in [(parent, true), (path, false)] {
        let meta = std::fs::symlink_metadata(p).map_err(|_| unavailable())?;
        if meta.uid() != uid
            || meta.mode() & 0o777 != if directory { 0o700 } else { 0o600 }
            || if directory {
                !meta.is_dir()
            } else {
                !meta.file_type().is_socket()
            }
        {
            return Err(Error::new(
                "permission_denied",
                "unsafe daemon socket ownership/type/mode",
                false,
            ));
        }
    }
    Ok(())
}
fn check_peer(uid: u32) -> source::Result<()> {
    if uid != nix::unistd::geteuid().as_raw() {
        return Err(Error::new(
            "permission_denied",
            "daemon peer UID differs",
            false,
        ));
    }
    Ok(())
}
/// signalfd handles these on the UI loop. Threads started before that loop must
/// inherit the block too; otherwise SIGTERM can bypass graceful surface cleanup.
struct LinkSignalMask(nix::sys::signal::SigSet);
impl LinkSignalMask {
    fn block() -> source::Result<Self> {
        use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};
        let mut handled = SigSet::empty();
        handled.add(Signal::SIGINT);
        handled.add(Signal::SIGTERM);
        let mut previous = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&handled), Some(&mut previous))
            .map_err(|_| Error::new("io", "daemon source signal mask unavailable", false))?;
        Ok(Self(previous))
    }
}
impl Drop for LinkSignalMask {
    fn drop(&mut self) {
        if nix::sys::signal::pthread_sigmask(
            nix::sys::signal::SigmaskHow::SIG_SETMASK,
            Some(&self.0),
            None,
        )
        .is_err()
        {
            eprintln!("SOURCE_ERROR code=signal_mask_restore");
        }
    }
}

impl Client {
    pub fn start(mut config: Config) -> source::Result<Self> {
        if !config.socket.is_absolute() || config.topics.len() > nacre_protocol::MAX_SUBSCRIPTIONS {
            return Err(Error::new(
                "validation",
                "invalid daemon source configuration",
                false,
            ));
        }
        config.topics.sort();
        config.topics.dedup();
        framing::encode(&Message::Hello {
            proto: 1,
            min: Some(1),
            client: config.client.clone(),
        })
        .map_err(failure)?;
        framing::encode(&Message::Sub {
            topics: config.topics.clone(),
        })
        .map_err(failure)?;
        if let Some((provider, methods, _)) = &config.provider {
            if methods.keys().any(|m| !m.starts_with("ui.")) {
                return Err(Error::new(
                    "validation",
                    "only UI provider methods may be registered",
                    false,
                ));
            }
            framing::encode(&Message::Register {
                provider: provider.clone(),
                methods: methods.clone(),
            })
            .map_err(failure)?;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let epoch = format!("nacre-link-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let mut topics = BTreeMap::new();
        for topic in &config.topics {
            let p = Publisher::new(epoch.clone(), serde_json::json!({}))?;
            p.disconnect()?;
            topics.insert(topic.clone(), p);
        }
        let connection = connection::Connection::new(epoch.clone(), Instant::now())?;
        let link = Publisher::new(epoch, serde_json::json!({}))?;
        link.disconnect()?;
        let shared = Arc::new(Shared {
            link,
            connection,
            topics,
            leases: leases::Registry::default(),
            status: Mutex::new(Status::default()),
            stop: AtomicBool::new(false),
            notify: Notify::new(),
            ended: AtomicBool::new(false),
            transport_generation: AtomicU64::new(0),
            closed: Notify::new(),
        });
        let (tx, rx) = mpsc::channel(nacre_protocol::MAX_CALLS);
        let state = shared.clone();
        let thread = {
            let _signals = LinkSignalMask::block()?;
            std::thread::Builder::new()
                .name("nacre-link".into())
                .stack_size(512 * 1024)
                .spawn(move || {
                    let _exit = OnExit(state.clone());
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .max_blocking_threads(2)
                        .build();
                    if let Ok(runtime) = runtime {
                        runtime.block_on(run(config, state.clone(), rx));
                    }
                    disconnect(&state);
                })
                .map_err(|_| Error::new("io", "daemon source thread unavailable", true))?
        };
        Ok(Self(Arc::new(Owner {
            shared,
            tx,
            thread: Mutex::new(Some(thread)),
        })))
    }
    /// Always-available local monitor, not an external daemon topic.
    pub fn connection(&self) -> Topic {
        Topic {
            client: self.clone(),
            publisher: self.0.shared.connection.publisher.clone(),
        }
    }
    pub fn topic(&self, name: &str) -> source::Result<Topic> {
        Ok(Topic {
            client: self.clone(),
            publisher: self
                .0
                .shared
                .topics
                .get(name)
                .ok_or_else(|| Error::new("unknown_topic", "topic was not requested", false))?
                .clone(),
        })
    }
    pub fn status(&self) -> Status {
        self.0
            .shared
            .status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    pub fn begin_call(
        &self,
        method: String,
        params: Value,
        duration: Duration,
        guard: CallGuard,
        wake: Option<CallWake>,
    ) -> source::Result<CallTicket> {
        if duration.is_zero()
            || duration > Duration::from_secs(120)
            || method.is_empty()
            || method.len() > 128
        {
            return Err(Error::new("validation", "invalid call/deadline", false));
        }
        let generation = self.0.shared.transport_generation.load(Ordering::Acquire);
        let status = self.status();
        if self.0.shared.stop.load(Ordering::Acquire)
            || !self.0.shared.link.snapshot().connected
            || status.boot.as_deref() != Some(guard.boot.as_str())
        {
            return Err(unavailable());
        }
        let state = Arc::new(std::sync::atomic::AtomicU8::new(TICKET_QUEUED));
        let (sender, receiver) = oneshot::channel();
        let reply = CallReply::Ticket(TicketCompletion {
            sender: Some(sender),
            wake,
        });
        let command = Command::Call {
            id: None,
            method,
            params,
            deadline: Instant::now() + duration,
            reply,
            ticket: Some(TicketDispatch {
                state: state.clone(),
                generation,
                guard,
            }),
        };
        match self.0.tx.try_send(command) {
            Ok(()) => Ok(CallTicket {
                receiver,
                state,
                done: false,
            }),
            Err(error) => {
                if let Command::Call {
                    reply: CallReply::Ticket(mut completion),
                    ..
                } = error.into_inner()
                {
                    completion.wake = None;
                }
                Err(Error::new(
                    "busy",
                    "daemon call queue full or stopped",
                    true,
                ))
            }
        }
    }
    pub async fn call(
        &self,
        method: String,
        params: Value,
        duration: Duration,
    ) -> source::Result<Value> {
        self.call_id(None, method, params, duration).await
    }
    async fn call_id(
        &self,
        id: Option<Id>,
        method: String,
        params: Value,
        duration: Duration,
    ) -> source::Result<Value> {
        if duration.is_zero()
            || duration > Duration::from_secs(120)
            || method.is_empty()
            || method.len() > 128
        {
            return Err(Error::new("validation", "invalid call/deadline", false));
        }
        if self.0.shared.stop.load(Ordering::Acquire) {
            return Err(unavailable());
        }
        let (reply, rx) = oneshot::channel();
        self.0
            .tx
            .try_send(Command::Call {
                id,
                method,
                params,
                deadline: Instant::now() + duration,
                reply: CallReply::Async(reply),
                ticket: None,
            })
            .map_err(|_| Error::new("busy", "daemon call queue full or stopped", true))?;
        rx.await.map_err(|_| unavailable())?
    }
    async fn control(&self, stop: bool) -> source::Result<Value> {
        if stop {
            self.0.shared.stop.store(true, Ordering::Release);
            self.0.shared.notify.notify_one();
            loop {
                let closed = self.0.shared.closed.notified();
                if self.0.shared.ended.load(Ordering::Acquire) {
                    return Ok(Value::Null);
                }
                closed.await;
            }
        }
        let (reply, rx) = oneshot::channel();
        self.0
            .tx
            .try_send(if stop {
                Command::Stop(reply)
            } else {
                Command::Reconnect(reply)
            })
            .map_err(|_| unavailable())?;
        rx.await.map_err(|_| unavailable())?
    }
}
impl Source for Client {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.0.shared.link.snapshot()
    }
    fn subscribe(&self, cb: Callback) -> source::Result<Subscription> {
        self.0.shared.link.subscribe(cb)
    }
    fn invoke(&self, method: String, params: Value) -> source::Reply<'_> {
        Box::pin(self.call(method, params, Duration::from_secs(3)))
    }
    fn call(&self, method: String, params: Value, duration: Duration) -> source::Reply<'_> {
        Box::pin(Client::call(self, method, params, duration))
    }
    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(self.control(false))
    }
    fn stop(&self) -> source::Reply<'_> {
        Box::pin(self.control(true))
    }
}
impl Source for Topic {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, cb: Callback) -> source::Result<Subscription> {
        self.publisher.subscribe(cb)
    }
    fn invoke(&self, m: String, p: Value) -> source::Reply<'_> {
        Box::pin(self.client.call(m, p, Duration::from_secs(3)))
    }
    fn call(&self, m: String, p: Value, d: Duration) -> source::Reply<'_> {
        Box::pin(self.client.call(m, p, d))
    }
    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(self.client.control(false))
    }
    fn stop(&self) -> source::Reply<'_> {
        Box::pin(self.client.control(true))
    }
}
fn disconnect(shared: &Shared) {
    shared.connection.disconnected(Instant::now());
    let _ = shared.link.disconnect();
    shared.transport_generation.fetch_add(1, Ordering::AcqRel);
    if let Ok(mut status) = shared.status.lock() {
        *status = Status::default();
    }
    for publisher in shared.publishers() {
        let _ = publisher.disconnect();
    }
}
struct Session {
    reader: framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    boot: String,
    methods: BTreeMap<String, nacre_protocol::Method>,
    available: Vec<String>,
    advertised: BTreeMap<String, u16>,
    leased: BTreeMap<String, u64>,
}
async fn sync_leases(
    shared: &Shared,
    session: &mut Session,
    mirrors: &mut BTreeMap<String, Mirror>,
) -> source::Result<()> {
    let wanted = shared.leases.wanted();
    let removed = session
        .leased
        .iter()
        .filter(|(name, generation)| {
            wanted
                .get(*name)
                .is_none_or(|w| w.generation != **generation)
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in &removed {
        session.leased.remove(name);
        session.available.retain(|n| n != name);
        mirrors.remove(name);
    }
    if !removed.is_empty() {
        send(
            shared,
            &mut session.writer,
            &Message::Unsub { topics: removed },
        )
        .await?;
    }
    let mut added = Vec::new();
    let mut resync = Vec::new();
    for (name, wanted) in wanted {
        if session.advertised.get(&name) != Some(&1) {
            continue;
        }
        if !session.leased.contains_key(&name) {
            wanted.publisher.restart(session.boot.clone())?;
            let mut mirror = Mirror::new(Instant::now());
            mirror.reconnected(&session.boot, Instant::now())?;
            mirrors.insert(name.clone(), mirror);
            session.leased.insert(name.clone(), wanted.generation);
            session.available.push(name.clone());
            added.push(name);
        } else if wanted.resync {
            if let Some(mirror) = mirrors.get_mut(&name) {
                mirror.disconnected(Instant::now());
            }
            wanted.publisher.disconnect()?;
            resync.push(name);
        }
    }
    if !added.is_empty() {
        send(shared, &mut session.writer, &Message::Sub { topics: added }).await?;
    }
    if !resync.is_empty() {
        send(
            shared,
            &mut session.writer,
            &Message::Resync { topics: resync },
        )
        .await?;
    }
    Ok(())
}
async fn connect(config: &Config) -> source::Result<Session> {
    check_socket(&config.socket)?;
    let stream = UnixStream::connect(&config.socket)
        .await
        .map_err(|_| unavailable())?;
    check_peer(stream.peer_cred().map_err(|_| unavailable())?.uid())?;
    let (r, mut writer) = stream.into_split();
    let mut reader = framing::Reader::new(BufReader::new(r));
    framing::write(
        &mut writer,
        &Message::Hello {
            proto: 1,
            min: Some(1),
            client: config.client.clone(),
        },
    )
    .await
    .map_err(failure)?;
    let Some(Message::Welcome {
        proto,
        min,
        boot,
        methods,
        topics,
        ..
    }) = reader.next().await.map_err(failure)?
    else {
        return Err(Error::new("protocol", "welcome required", false));
    };
    if proto != nacre_protocol::VERSION || min > proto {
        return Err(Error::new(
            "unsupported_version",
            "daemon protocol overlap missing",
            false,
        ));
    }
    let available = config
        .topics
        .iter()
        .filter(|t| topics.get(*t) == Some(&1))
        .cloned()
        .collect::<Vec<_>>();
    if let Some((provider, methods, _)) = &config.provider {
        framing::write(
            &mut writer,
            &Message::Register {
                provider: provider.clone(),
                methods: methods.clone(),
            },
        )
        .await
        .map_err(failure)?;
    }
    if !available.is_empty() {
        framing::write(
            &mut writer,
            &Message::Sub {
                topics: available.clone(),
            },
        )
        .await
        .map_err(failure)?;
    }
    Ok(Session {
        reader,
        writer,
        boot,
        methods,
        available,
        advertised: topics,
        leased: BTreeMap::new(),
    })
}
async fn send(
    shared: &Shared,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    message: &Message,
) -> source::Result<()> {
    tokio::select! {
        _=shared.notify.notified()=>Err(unavailable()),
        result=tokio::time::timeout(Duration::from_secs(10),framing::write(writer,message))=>result.map_err(|_|timeout())?.map_err(failure),
    }
}
fn apply_snapshot(
    shared: &Shared,
    mirrors: &mut BTreeMap<String, Mirror>,
    boot: &str,
    topic: &str,
    seq: u64,
    data: Value,
) -> source::Result<()> {
    if let Some(mirror) = mirrors.get_mut(topic) {
        let snapshot = Arc::new(Snapshot {
            epoch: boot.into(),
            revision: seq,
            data: Arc::new(data),
            connected: true,
        });
        if mirror.accept(snapshot, Instant::now())? == Applied::Updated
            && let Some(snapshot) = mirror.snapshot()
            && let Some(publisher) = shared.publisher(topic)
        {
            publisher.publish_shared(snapshot.data.clone())?;
        }
    }
    Ok(())
}
fn apply_patch(
    shared: &Shared,
    mirrors: &mut BTreeMap<String, Mirror>,
    boot: &str,
    topic: &str,
    base: u64,
    seq: u64,
    data: &Value,
) -> source::Result<Applied> {
    let Some(mirror) = mirrors.get_mut(topic) else {
        return Ok(Applied::Ignored);
    };
    let applied = mirror.patch(boot, base, seq, data, Instant::now())?;
    if applied == Applied::Updated
        && let Some(snapshot) = mirror.snapshot()
        && let Some(publisher) = shared.publisher(topic)
    {
        publisher.publish_shared(snapshot.data.clone())?;
    }
    if applied == Applied::Resync
        && let Some(publisher) = shared.publisher(topic)
    {
        publisher.disconnect()?;
    }
    Ok(applied)
}
fn expire(pending: &mut BTreeMap<Id, Pending>) {
    let now = Instant::now();
    let ids = pending
        .iter()
        .filter(|(_, p)| p.deadline <= now)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(call) = pending.remove(&id) {
            call.reply.finish(Err(timeout()));
        }
    }
}
fn resolve(pending: &mut BTreeMap<Id, Pending>, id: &Id, reply: Reply) {
    if let Some(call) = pending.remove(id) {
        let result = if call.deadline <= Instant::now() {
            Err(timeout())
        } else {
            match reply {
                Reply::Ok { ok } => Ok(ok),
                Reply::Err { err } => Err(failure(err)),
            }
        };
        if call.trace_menu {
            trace_menu_return(id, &result);
        }
        call.reply.finish(result);
    }
}
// Numeric method readiness only: never query text or result contents.
fn trace_menu_return(id: &Id, result: &source::Result<Value>) {
    let Id::Integer(id) = id else {
        return;
    };
    match result {
        Ok(value) => match (
            value.get("catalog_rev").and_then(Value::as_u64),
            value.get("order").and_then(Value::as_array),
        ) {
            (Some(revision), Some(order)) => println!(
                "MENU_RPC_RETURN id={id} ok=true catalog_rev={revision} count={}",
                order.len()
            ),
            _ => println!("MENU_RPC_RETURN id={id} ok=true payload=false"),
        },
        Err(error) => println!(
            "MENU_RPC_RETURN id={id} ok=false code={} retry={}",
            error.code, error.retry
        ),
    }
}
async fn run(config: Config, shared: Arc<Shared>, mut commands: mpsc::Receiver<Command>) {
    let mut backoff = source::Reconnect::default();
    let mut next = 0i64;
    'outer: while !shared.stop.load(Ordering::Acquire) {
        let socket_watch = connection::SocketWatch::new(&config.socket);
        let handshake = tokio::time::timeout(Duration::from_secs(5), connect(&config));
        tokio::pin!(handshake);
        let mut session = loop {
            tokio::select! {
                _=shared.notify.notified()=>break 'outer,
                _=shared.connection.delayed_mark()=>{},
                result=&mut handshake=>match result {Ok(Ok(s))=>break Some(s),_=>break None},
                command=commands.recv()=>match command {
                    Some(Command::Stop(reply))=>{shared.stop.store(true,Ordering::Release);let _=reply.send(Ok(Value::Null));break 'outer;},
                    Some(Command::Reconnect(reply))=>{let _=reply.send(Ok(Value::Null));},
                    Some(Command::Call{reply,..})=>reply.finish(Err(unavailable())),
                    #[cfg(test)] Some(Command::Wire(_,reply))=>{let _=reply.send(Err(unavailable()));},
                    #[cfg(test)] Some(Command::AbortProviderTasks(reply))=>{let _=reply.send(());},
                    #[cfg(test)] Some(Command::Gate{entered,release})=>{let _=entered.send(());let _=release.await;},
                    None=>break 'outer,
                }
            }
        };
        if let Some(ref mut session) = session {
            backoff.reset();
            if let Ok(mut status) = shared.status.lock() {
                *status = Status {
                    boot: Some(session.boot.clone()),
                    methods: session.methods.clone(),
                };
            }
            for topic in shared.publishers() {
                if topic.restart(session.boot.clone()).is_err() {
                    break 'outer;
                }
            }
            let _ = shared
                .link
                .publish(serde_json::json!({"boot":session.boot,"methods":session.methods}));
            shared.connection.connected();
            let mut mirrors = config
                .topics
                .iter()
                .map(|t| (t.clone(), Mirror::new(Instant::now())))
                .collect::<BTreeMap<_, _>>();
            for mirror in mirrors.values_mut() {
                let _ = mirror.reconnected(&session.boot, Instant::now());
            }
            // Initial subscription failure uses the same disconnect/backoff path.
            if sync_leases(&shared, session, &mut mirrors).await.is_ok() {
                let mut pending = BTreeMap::<Id, Pending>::new();
                let mut handlers: JoinSet<source::Result<Value>> = JoinSet::new();
                let mut handler_requests = HashMap::<tokio::task::Id, Id>::new();
                loop {
                    if shared.stop.load(Ordering::Acquire) {
                        break;
                    }
                    let deadline = pending.values().map(|p| p.deadline).min();
                    let alarm = async {
                        match deadline {
                            Some(d) => tokio::time::sleep_until(d.into()).await,
                            None => std::future::pending::<()>().await,
                        }
                    };
                    tokio::pin!(alarm);
                    tokio::select! {
                        _=shared.notify.notified()=>break,
                        _=shared.leases.changed.notified()=>{if sync_leases(&shared,session,&mut mirrors).await.is_err(){break;}},
                        _=&mut alarm=>expire(&mut pending),
                        command=commands.recv()=>match command {
                            Some(Command::Stop(reply))=>{shared.stop.store(true,Ordering::Release);disconnect(&shared);let _=reply.send(Ok(Value::Null));break;},
                            Some(Command::Reconnect(reply))=>{disconnect(&shared);let _=reply.send(Ok(Value::Null));break;},
                            Some(Command::Call{id,method,params,deadline,reply,ticket})=>{
                                if let Some(ticket) = &ticket {
                                    let available = ticket_guard_matches(ticket, shared.transport_generation.load(Ordering::Acquire), &session.boot, &session.methods, &method);
                                    if !available { reply.finish(Err(unavailable())); continue; }
                                    if ticket.state.load(Ordering::Acquire) == TICKET_CANCELLED { drop(reply); continue; }
                                }
                                if deadline<=Instant::now(){reply.finish(Err(timeout()));continue;}
                                if pending.len()>=nacre_protocol::MAX_CALLS{reply.finish(Err(Error::new("busy","daemon outstanding-call limit",true)));continue;}
                                next+=1;if next as u64>nacre_protocol::MAX_SAFE_INTEGER{reply.finish(Err(Error::new("internal","call id exhausted",false)));break;}
                                let id=id.unwrap_or(Id::Integer(next));
                                if pending.contains_key(&id){reply.finish(Err(Error::new("validation","duplicate call id",false)));continue;}
                                let remaining=deadline.saturating_duration_since(Instant::now());
                                let trace_menu=bead::core::profile::enabled() && method=="menu.query";
                                let message=Message::Call{id:id.clone(),m:method,p:params,timeout:Some(remaining.as_millis().clamp(1,120000) as u32)};
                                if let Err(e)=framing::encode(&message){reply.finish(Err(failure(e)));continue;}
                                if let Some(ticket) = &ticket
                                    && !claim_ticket_dispatch(&ticket.state)
                                { drop(reply); continue; }
                                pending.insert(id.clone(),Pending{reply,deadline,trace_menu});
                                if !tokio::time::timeout(remaining.min(Duration::from_secs(10)),send(&shared,&mut session.writer,&message)).await.is_ok_and(|r|r.is_ok()){break;}
                                if trace_menu && let Id::Integer(id)=&id { println!("MENU_RPC_SENT id={id}"); }
                            },
                            #[cfg(test)] Some(Command::Wire(message,reply))=>{let result=send(&shared,&mut session.writer,&message).await.map(|_|Value::Null);let _=reply.send(result);},
                            #[cfg(test)] Some(Command::AbortProviderTasks(reply))=>{handlers.abort_all();let _=reply.send(());},
                            #[cfg(test)] Some(Command::Gate{entered,release})=>{let _=entered.send(());let _=release.await;},
                            None=>break,
                        },
                        complete=handlers.join_next_with_id(),if !handlers.is_empty()=>{
                            if let Some(completion)=complete {
                                let (task_id,result)=match completion {
                                    Ok((task_id,result))=>(task_id,Some(result)),
                                    Err(error)=>{
                                        let task_id=error.id();
                                        eprintln!("UI_PROVIDER_TASK_FAILED panic={} cancelled={}",error.is_panic(),error.is_cancelled());
                                        (task_id,None)
                                    },
                                };
                                let Some(id)=handler_requests.remove(&task_id) else {
                                    eprintln!("UI_PROVIDER_TASK_FAILED metadata_missing=true");
                                    continue;
                                };
                                let reply=match result {
                                    Some(Ok(ok))=>Reply::Ok{ok},
                                    Some(Err(e))=>Reply::Err{err:nacre_protocol::Failure::new(nacre_protocol::ErrorCode::Unavailable,e.message,e.retry).dispatch_state(true)},
                                    None=>Reply::Err{err:nacre_protocol::Failure::new(nacre_protocol::ErrorCode::Internal,"UI handler task failed",false).dispatch_state(true)},
                                };
                                if send(&shared,&mut session.writer,&Message::Ret{id,reply}).await.is_err(){break;}
                            }
                        },
                        message=session.reader.next()=>{
                            let Ok(Some(message))=message else{break;};
                            match message {
                                Message::Snap{topic,seq,data}=>{
                                    if !session.available.contains(&topic){continue;}
                                    if apply_snapshot(&shared,&mut mirrors,&session.boot,&topic,seq,data).is_err(){break;}
                                },
                                Message::Patch{topic,base,seq,data}=>{
                                    if !session.available.contains(&topic){continue;}
                                    match apply_patch(&shared,&mut mirrors,&session.boot,&topic,base,seq,&data) {
                                        Ok(Applied::Resync)=>{
                                            if send(&shared,&mut session.writer,&Message::Resync{topics:vec![topic]}).await.is_err(){break;}
                                        },
                                        Err(_)=>break,
                                        _=>{},
                                    }
                                },
                                Message::Ret{id,reply}=>resolve(&mut pending,&id,reply),
                                Message::Ping{id}=>{if send(&shared,&mut session.writer,&Message::Pong{id}).await.is_err(){break;}},
                                Message::Call{id,m,p,timeout:limit}=>{
                                    let Some((_,methods,handler))=&config.provider else {
                                        let reply=Reply::Err{err:nacre_protocol::Failure::new(nacre_protocol::ErrorCode::UnknownMethod,"UI method unavailable",false).dispatch_state(false)};
                                        if send(&shared,&mut session.writer,&Message::Ret{id,reply}).await.is_err(){break;}
                                        continue;
                                    };
                                    if !methods.contains_key(&m) {
                                        let reply=Reply::Err{err:nacre_protocol::Failure::new(nacre_protocol::ErrorCode::UnknownMethod,"UI method unavailable",false).dispatch_state(false)};
                                        if send(&shared,&mut session.writer,&Message::Ret{id,reply}).await.is_err(){break;}
                                        continue;
                                    }
                                    if handlers.len()>=nacre_protocol::MAX_CALLS {
                                        let reply=Reply::Err{err:nacre_protocol::Failure::new(nacre_protocol::ErrorCode::Busy,"UI handler request limit reached",true).dispatch_state(false)};
                                        if send(&shared,&mut session.writer,&Message::Ret{id,reply}).await.is_err(){break;}
                                        continue;
                                    }
                                    let handler=handler.clone();
                                    let task=handlers.spawn(async move{
                                        let future=handler(m,p);
                                        tokio::time::timeout(Duration::from_millis(limit.unwrap_or(1000) as u64),future).await.unwrap_or_else(|_|Err(timeout()))
                                    });
                                    handler_requests.insert(task.id(),id);
                                },
                                Message::Ev{..}|Message::Lost{..}|Message::Pong{..}=>{if let Some(event)=&config.event {event(&message);}},
                                Message::Error{..}=>break,
                                _=>break,
                            }
                        }
                    }
                }
                handlers.abort_all();
                for (_, p) in pending {
                    p.reply.finish(Err(if p.deadline <= Instant::now() {
                        timeout()
                    } else {
                        unavailable()
                    }));
                }
            }
        }
        drop(session);
        disconnect(&shared);
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let sleep = tokio::time::sleep(backoff.delay());
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _=shared.notify.notified()=>break 'outer,
                _=&mut sleep=>break,
                _=socket_watch.changed()=>break,
                _=shared.connection.delayed_mark()=>{},
                command=commands.recv()=>match command {
                    Some(Command::Stop(reply))=>{shared.stop.store(true,Ordering::Release);let _=reply.send(Ok(Value::Null));break 'outer;},
                    Some(Command::Reconnect(reply))=>{let _=reply.send(Ok(Value::Null));break;},
                    Some(Command::Call{reply,..})=>reply.finish(Err(unavailable())),
                    #[cfg(test)] Some(Command::Wire(_,reply))=>{let _=reply.send(Err(unavailable()));},
                    #[cfg(test)] Some(Command::AbortProviderTasks(reply))=>{let _=reply.send(());},
                    #[cfg(test)] Some(Command::Gate{entered,release})=>{let _=entered.send(());let _=release.await;},
                    None=>break 'outer,
                }
            }
        }
    }
    disconnect(&shared);
}

/// Explicit socket only: used by the nested integration proof, never discovers a bus.
pub fn check(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::start(Config::new(path, vec!["compositor".into()]))?;
    let topic = client.topic("compositor")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !topic.snapshot().connected
        || topic
            .snapshot()
            .data
            .get("available")
            .and_then(Value::as_bool)
            != Some(true)
    {
        if Instant::now() >= deadline {
            return Err("daemon source did not become fresh".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    let status = client.status();
    let result = runtime.block_on(topic.call(
        "daemon.status".into(),
        serde_json::json!({}),
        Duration::from_secs(1),
    ))?;
    let snapshot = topic.snapshot();
    let count = snapshot
        .data
        .get("workspaces")
        .and_then(Value::as_object)
        .ok_or("workspace map missing")?
        .len();
    let label = format!("Workspaces: {count}");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bead-test/tests/fixtures");
    let mut ui = bead_test::Harness::load(
        &root,
        std::path::Path::new("main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([("query".into(), bead::core::Value::Text(label.clone()))]),
        bead_test::Config {
            width: 384,
            height: 244,
            scale: 1.5,
        },
    )?;
    if !ui
        .accessibility()
        .nodes
        .iter()
        .any(|(_, n)| n.value() == Some(label.as_str()))
    {
        return Err("source value did not reach UI".into());
    }
    let rendered = ui
        .render()?
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    if rendered == 0 {
        return Err("source UI render empty".into());
    }
    println!(
        "{}",
        serde_json::json!({"connected":snapshot.connected,"boot":status.boot,"method_count":status.methods.len(),"rendered_pixels":rendered,"daemon_boot":result.get("boot"),"available":snapshot.data.get("available"),"output_count":snapshot.data.get("outputs").and_then(Value::as_object).map(|v|v.len()),"workspace_count":snapshot.data.get("workspaces").and_then(Value::as_object).map(|v|v.len())})
    );
    runtime.block_on(topic.stop())?;
    Ok(())
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod core_compatibility_tests;

#[cfg(test)]
mod call_ticket_tests;
