// SPDX-License-Identifier: MIT
//! Event-driven client for the public power-profiles-daemon D-Bus interface.
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
};
use zbus::{Connection, Proxy, fdo::DBusProxy, zvariant::OwnedValue};

const PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const INTERFACE: &str = "org.freedesktop.UPower.PowerProfiles";
const NAMES: [&str; 2] = [
    "org.freedesktop.UPower.PowerProfiles",
    "net.hadess.PowerProfiles",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerProfilesSnapshot {
    pub revision: u64,
    pub available: bool,
    pub active: String,
    pub profiles: Vec<String>,
    pub degraded: String,
}
#[derive(Clone, Debug)]
pub enum PowerProfilesAction {
    SetProfile(String),
}
type Callback = Arc<dyn Fn(Arc<PowerProfilesSnapshot>) + Send + Sync>;
struct Shared {
    snapshot: Mutex<Arc<PowerProfilesSnapshot>>,
    callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    stopped: tokio::sync::Notify,
}
impl Shared {
    fn change(&self, update: impl FnOnce(&mut PowerProfilesSnapshot)) {
        let mut old = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = (**old).clone();
        update(&mut next);
        if next.available == old.available
            && next.active == old.active
            && next.profiles == old.profiles
            && next.degraded == old.degraded
        {
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
        callbacks
            .into_iter()
            .for_each(|callback| callback(next.clone()));
    }
}
pub struct Subscription {
    shared: Weak<Shared>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared
                .callbacks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.id);
        }
    }
}
/// Owns the event worker for the system bus connection.
pub struct PowerProfiles {
    shared: Arc<Shared>,
    connection: Connection,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl PowerProfiles {
    /// Connect to the system bus and start when it is available.
    #[doc(hidden)]
    pub fn start_system() -> Option<Arc<Self>> {
        let (runtime, connection) = crate::bus::connect_blocking(crate::bus::system_connection())?;
        Some(Self::spawn(Some(runtime), connection))
    }
    pub fn start(connection: Connection) -> Arc<Self> {
        Self::spawn(None, connection)
    }
    fn spawn(runtime: Option<tokio::runtime::Runtime>, connection: Connection) -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(PowerProfilesSnapshot::default())),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stopped: tokio::sync::Notify::new(),
        });
        let conn = connection.clone();
        let worker = crate::bus::spawn_worker(
            runtime,
            shared.clone(),
            |s| &s.stopped,
            |s| s.change(unavailable),
            move |s| run(conn.clone(), s),
        );
        Arc::new(Self {
            shared,
            connection,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn snapshot(&self) -> PowerProfilesSnapshot {
        (**self
            .shared
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner()))
        .clone()
    }
    pub fn subscribe(
        &self,
        callback: impl Fn(Arc<PowerProfilesSnapshot>) + Send + Sync + 'static,
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
    pub fn action(&self, action: PowerProfilesAction) -> Result<(), String> {
        let PowerProfilesAction::SetProfile(profile) = action;
        if !self.snapshot().profiles.contains(&profile) {
            return Err(format!("unknown power profile '{profile}'"));
        }
        crate::bus::runtime()
            .map_err(|e| e.to_string())?
            .block_on(async {
                let name = current_name(&self.connection).await?;
                let proxy = Proxy::new(&self.connection, name, PATH, INTERFACE)
                    .await
                    .map_err(|e| e.to_string())?;
                proxy
                    .set_property("ActiveProfile", profile)
                    .await
                    .map_err(|e| e.to_string())
            })
    }
}
impl Drop for PowerProfiles {
    fn drop(&mut self) {
        self.shared.stopped.notify_one();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}
fn unavailable(snapshot: &mut PowerProfilesSnapshot) {
    snapshot.available = false;
    snapshot.active.clear();
    snapshot.profiles.clear();
    snapshot.degraded.clear();
}
async fn current_name(connection: &Connection) -> Result<&'static str, String> {
    let dbus = DBusProxy::new(connection)
        .await
        .map_err(|e| e.to_string())?;
    for name in NAMES {
        let bus_name = name.try_into().expect("constant bus name is valid");
        if dbus.name_has_owner(bus_name).await.unwrap_or(false) {
            return Ok(name);
        }
    }
    Err("power-profiles-daemon is unavailable".into())
}
async fn run(connection: Connection, shared: Arc<Shared>) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&connection).await?;
    let mut owners = dbus.receive_name_owner_changed().await?;
    let rules = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(rules, &connection, Some(64)).await?;
    refresh(&connection, &shared).await;
    loop {
        tokio::select! {
            owner = owners.next() => {
                let signal = owner.ok_or(zbus::Error::Failure("owner stream ended".into()))?;
                if NAMES.contains(&signal.args()?.name().as_str()) { refresh(&connection, &shared).await; }
            },
            message = signals.next() => {
                if let Some(Ok(message)) = message {
                    let header = message.header();
                    if header.interface().is_some_and(|i| i.as_str() == "org.freedesktop.DBus.Properties")
                        && header.member().is_some_and(|m| m.as_str() == "PropertiesChanged")
                        && header.path().is_some_and(|p| p.as_str() == PATH)
                    { refresh(&connection, &shared).await; }
                }
            }
        }
    }
}
async fn refresh(connection: &Connection, shared: &Shared) {
    let Ok(name) = current_name(connection).await else {
        shared.change(unavailable);
        return;
    };
    let Ok(proxy) = Proxy::new(connection, name, PATH, INTERFACE).await else {
        return;
    };
    let (Ok(active), Ok(profile_dicts), Ok(degraded)) = (
        proxy.get_property::<String>("ActiveProfile").await,
        proxy
            .get_property::<Vec<HashMap<String, OwnedValue>>>("Profiles")
            .await,
        proxy.get_property::<String>("PerformanceDegraded").await,
    ) else {
        return;
    };
    let profiles = profile_dicts
        .into_iter()
        .filter_map(|mut profile| {
            profile
                .remove("Profile")
                .and_then(|value| <String>::try_from(value).ok())
        })
        .collect();
    shared.change(|s| {
        s.available = true;
        s.active = active;
        s.profiles = profiles;
        s.degraded = degraded;
    });
}
