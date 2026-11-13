// SPDX-License-Identifier: MIT
//! Event-driven client for the public MPRIS MediaPlayer2 and Player interfaces.
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
};
use zbus::{
    Connection, Proxy,
    fdo::DBusProxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    pub id: String,
    pub identity: String,
    pub status: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length_ms: i64,
    pub position_ms: i64,
    pub position_at: u64,
    pub art_url: String,
    pub track_id: String,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSnapshot {
    pub revision: u64,
    pub players: Vec<Player>,
    pub active: Option<String>,
}
#[derive(Clone, Debug)]
pub enum MediaAction {
    PlayPause(String),
    Next(String),
    Previous(String),
    Seek(String, i64),
}
type Callback = Arc<dyn Fn(Arc<MediaSnapshot>) + Send + Sync>;
struct Shared {
    snapshot: Mutex<Arc<MediaSnapshot>>,
    playing_order: Mutex<Vec<String>>,
    callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    stopped: tokio::sync::Notify,
}
impl Shared {
    fn change(&self, f: impl FnOnce(&mut MediaSnapshot)) {
        let mut old = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = (**old).clone();
        f(&mut next);
        if next.players == old.players && next.active == old.active {
            return;
        }
        next.revision = old.revision.saturating_add(1);
        let next = Arc::new(next);
        *old = next.clone();
        drop(old);
        let callbacks = self
            .callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        callbacks.into_iter().for_each(|cb| cb(next.clone()));
    }
}
pub struct Subscription {
    shared: Weak<Shared>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(s) = self.shared.upgrade() {
            s.callbacks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.id);
        }
    }
}
/// Owns the event worker; dropping the final Arc stops it and releases the bus connection.
pub struct Media {
    shared: Arc<Shared>,
    connection: Connection,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Media {
    /// Connect to the session bus and start, returning `None` if it is unavailable.
    #[doc(hidden)]
    pub fn start_session() -> Option<Arc<Self>> {
        let (runtime, connection) = crate::bus::connect_blocking(Connection::session())?;
        Some(Self::spawn(Some(runtime), connection))
    }
    pub fn start(connection: Connection) -> Arc<Self> {
        Self::spawn(None, connection)
    }
    fn spawn(runtime: Option<tokio::runtime::Runtime>, connection: Connection) -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(MediaSnapshot::default())),
            playing_order: Mutex::new(Vec::new()),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stopped: tokio::sync::Notify::new(),
        });
        let conn = connection.clone();
        let worker = crate::bus::spawn_worker(
            runtime,
            shared.clone(),
            |s| &s.stopped,
            |s| {
                s.change(|s| {
                    s.players.clear();
                    s.active = None;
                })
            },
            move |s| run(conn.clone(), s),
        );
        Arc::new(Self {
            shared,
            connection,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn snapshot(&self) -> MediaSnapshot {
        (**self
            .shared
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner()))
        .clone()
    }
    pub fn subscribe(
        &self,
        callback: impl Fn(Arc<MediaSnapshot>) + Send + Sync + 'static,
    ) -> Subscription {
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let callback: Callback = Arc::new(callback);
        self.shared
            .callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, callback.clone());
        callback(
            self.shared
                .snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        );
        Subscription {
            shared: Arc::downgrade(&self.shared),
            id,
        }
    }
    pub fn action(&self, action: MediaAction) -> Result<(), String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(action_async(&self.connection, &self.shared, action))
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        self.shared.stopped.notify_one();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}

async fn run(connection: Connection, shared: Arc<Shared>) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&connection).await?;
    let mut owners = dbus.receive_name_owner_changed().await?;
    let mut signals = zbus::MessageStream::for_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .build(),
        &connection,
        Some(128),
    )
    .await?;
    let mut names_by_owner = HashMap::new();
    for name in dbus.list_names().await? {
        if is_player(name.as_str()) {
            let name = name.to_string();
            if let Ok(owner) = dbus
                .get_name_owner(name.as_str().try_into().expect("listed bus name is valid"))
                .await
            {
                names_by_owner.insert(owner.to_string(), name.clone());
            }
            refresh(&connection, &shared, name).await;
        }
    }
    loop {
        tokio::select! {
            owner = owners.next() => {
                let signal = owner.ok_or(zbus::Error::Failure("owner stream ended".into()))?;
                let change = signal.args()?;
                let name = change.name().as_str().to_owned();
                if is_player(&name) {
                    if let Some(owner) = change.old_owner().as_ref() {
                        names_by_owner.remove(owner.as_str());
                    }
                    if let Some(owner) = change.new_owner().as_ref().filter(|owner| !owner.as_str().is_empty()) {
                        names_by_owner.insert(owner.to_string(), name.clone());
                        refresh(&connection, &shared, name).await;
                    } else {
                        names_by_owner.retain(|_, player| player != &name);
                        shared.change(|s| {
                            s.players.retain(|p| p.id != name);
                            let mut order = shared.playing_order.lock().unwrap_or_else(|e| e.into_inner());
                            order.retain(|id| id != &name);
                            choose_active(s, &order);
                        });
                    }
                }
            },
            message = signals.next() => {
                if let Some(Ok(message)) = message {
                    let h=message.header();
                    if h.interface().is_some_and(|i|i.as_str()=="org.freedesktop.DBus.Properties") && h.member().is_some_and(|m|m.as_str()=="PropertiesChanged") {
                        if let Some(name) = h.sender().and_then(|sender| names_by_owner.get(sender.as_str())).cloned() { refresh(&connection,&shared,name).await; }
                    } else if h.interface().is_some_and(|i|i.as_str()==PLAYER) && h.member().is_some_and(|m|m.as_str()=="Seeked") && let (Some(sender),Ok(position))=(h.sender(),message.body().deserialize::<i64>()) && let Some(name)=names_by_owner.get(sender.as_str()) {
                        let id=name.clone(); shared.change(|s| { if let Some(p)=s.players.iter_mut().find(|p|p.id==id) { p.position_ms=(position/1000).max(0); p.position_at=now_ms(); } });
                    }
                }
            }
        }
    }
}
fn is_player(name: &str) -> bool {
    name.starts_with("org.mpris.MediaPlayer2.")
}
#[doc(hidden)]
pub fn monotonic_millis() -> u64 {
    let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
    unsafe {
        if libc::clock_gettime(libc::CLOCK_MONOTONIC, time.as_mut_ptr()) != 0 {
            return 0;
        }
        let t = time.assume_init();
        (t.tv_sec as u64)
            .saturating_mul(1000)
            .saturating_add(t.tv_nsec as u64 / 1_000_000)
    }
}
fn now_ms() -> u64 {
    monotonic_millis()
}
async fn refresh(connection: &Connection, shared: &Shared, name: String) {
    let old = shared
        .snapshot
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .players
        .iter()
        .find(|p| p.id == name)
        .cloned();
    let result = read_player(connection, &name).await;
    if let Ok(player) = result {
        let mut player = player;
        if old.as_ref().is_none_or(|old| old.status != player.status) {
            if let Ok(position) = read_position(connection, &name).await {
                player.position_ms = position;
                player.position_at = now_ms();
            }
        } else if let Some(old) = &old {
            player.position_ms = old.position_ms;
            player.position_at = old.position_at;
        }
        shared.change(|s| {
            let was_playing = s
                .players
                .iter()
                .find(|p| p.id == name)
                .is_some_and(|p| p.status == "Playing");
            let entered = player.status == "Playing" && !was_playing;
            if let Some(old) = s.players.iter_mut().find(|p| p.id == name) {
                *old = player;
            } else {
                s.players.push(player);
                s.players.sort_by(|a, b| a.id.cmp(&b.id));
            }
            let mut order = shared
                .playing_order
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if entered {
                order.retain(|id| id != &name);
                order.push(name.clone());
            }
            choose_active(s, &order);
        });
    }
}
fn choose_active(s: &mut MediaSnapshot, order: &[String]) {
    let playing = order.iter().rev().find(|id| {
        s.players
            .iter()
            .any(|player| &player.id == *id && player.status == "Playing")
    });
    if let Some(playing) = playing {
        s.active = Some(playing.clone());
    } else if s
        .active
        .as_ref()
        .is_none_or(|id| !s.players.iter().any(|p| &p.id == id))
    {
        s.active = s.players.first().map(|p| p.id.clone());
    }
}
async fn read_player(c: &Connection, name: &str) -> zbus::Result<Player> {
    let base = Proxy::new(c, name, PATH, "org.freedesktop.DBus.Properties").await?;
    let root: HashMap<String, OwnedValue> = base.call("GetAll", &(ROOT)).await?;
    let props: HashMap<String, OwnedValue> = base.call("GetAll", &(PLAYER)).await?;
    let metadata = props
        .get("Metadata")
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v.clone()).ok())
        .unwrap_or_default();
    let status = match val_string(&props, "PlaybackStatus").as_str() {
        "Playing" => "Playing",
        "Paused" => "Paused",
        "Stopped" => "Stopped",
        _ => "Stopped",
    }
    .to_owned();
    Ok(Player {
        id: name.into(),
        identity: val_string(&root, "Identity"),
        status,
        title: val_string(&metadata, "xesam:title"),
        artist: val_strings(&metadata, "xesam:artist").join(", "),
        album: val_string(&metadata, "xesam:album"),
        length_ms: val_i64(&metadata, "mpris:length") / 1000,
        position_ms: 0,
        position_at: now_ms(),
        art_url: val_string(&metadata, "mpris:artUrl"),
        track_id: val_path(&metadata, "mpris:trackid"),
        can_play_pause: val_bool(&props, "CanPlayPause"),
        can_next: val_bool(&props, "CanGoNext"),
        can_previous: val_bool(&props, "CanGoPrevious"),
        can_seek: val_bool(&props, "CanSeek"),
    })
}
async fn read_position(c: &Connection, name: &str) -> zbus::Result<i64> {
    Proxy::new(c, name, PATH, "org.freedesktop.DBus.Properties")
        .await?
        .call::<_, _, OwnedValue>("Get", &(PLAYER, "Position"))
        .await
        .map(|v| i64::try_from(v).unwrap_or(0))
}
fn val_string(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    m.get(k)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("")
        .into()
}
fn val_path(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    m.get(k)
        .and_then(|v| OwnedObjectPath::try_from(v.clone()).ok())
        .map(|p| p.to_string())
        .unwrap_or_default()
}
fn val_i64(m: &HashMap<String, OwnedValue>, k: &str) -> i64 {
    m.get(k).and_then(|v| i64::try_from(v).ok()).unwrap_or(0)
}
fn val_bool(m: &HashMap<String, OwnedValue>, k: &str) -> bool {
    m.get(k)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn val_strings(m: &HashMap<String, OwnedValue>, k: &str) -> Vec<String> {
    m.get(k)
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .unwrap_or_default()
}
async fn action_async(c: &Connection, s: &Shared, a: MediaAction) -> Result<(), String> {
    let (id, method, seek) = match a {
        MediaAction::PlayPause(id) => (id, "PlayPause", None),
        MediaAction::Next(id) => (id, "Next", None),
        MediaAction::Previous(id) => (id, "Previous", None),
        MediaAction::Seek(id, position) => (id, "SetPosition", Some(position)),
    };
    let p = s
        .snapshot
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .players
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or("player is unavailable")?;
    if let Some(position) = seek {
        if position < 0 {
            return Err("seek position must not be negative".into());
        }
        if !p.can_seek {
            return Err("player cannot seek".into());
        }
        let current = read_player(c, &id).await.map_err(|e| e.to_string())?;
        if current.track_id != p.track_id {
            return Err("track changed before seek".into());
        }
        let track: OwnedObjectPath = p
            .track_id
            .clone()
            .try_into()
            .map_err(|e: zbus::zvariant::Error| e.to_string())?;
        let proxy = Proxy::new(c, id.as_str(), PATH, PLAYER)
            .await
            .map_err(|e| e.to_string())?;
        proxy
            .call::<_, _, ()>(method, &(&track, position.saturating_mul(1000)))
            .await
            .map_err(|e| e.to_string())?;
    } else {
        match method {
            "PlayPause" if !p.can_play_pause => return Err("player cannot play or pause".into()),
            "Next" if !p.can_next => return Err("player cannot go to next track".into()),
            "Previous" if !p.can_previous => {
                return Err("player cannot go to previous track".into());
            }
            _ => {}
        }
        Proxy::new(c, id.as_str(), PATH, PLAYER)
            .await
            .map_err(|e| e.to_string())?
            .call::<_, _, ()>(method, &())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
