// SPDX-License-Identifier: MIT
//! Optional session-bus StatusNotifierWatcher for hosts without another watcher.
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex}};
use futures_util::StreamExt;
use zbus::{Connection, fdo::{DBusProxy, RequestNameFlags, RequestNameReply}, message::Header, object_server::SignalEmitter};

const NAME: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";

#[derive(Default)]
struct State {
    items: BTreeMap<String, String>,
    hosts: BTreeSet<String>,
}

struct Watcher(Arc<Mutex<State>>);

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).items.keys().cloned().collect()
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        !self.0.lock().unwrap_or_else(|e| e.into_inner()).hosts.is_empty()
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 { 0 }

    async fn register_status_notifier_item(&self, service: &str, #[zbus(header)] header: Header<'_>, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let sender = header.sender().ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing sender".into()))?.to_string();
        let address = if service.starts_with('/') { format!("{sender}{service}") } else if service.contains('/') { service.to_owned() } else { format!("{service}/StatusNotifierItem") };
        let inserted = self.0.lock().unwrap_or_else(|e| e.into_inner()).items.insert(address.clone(), sender).is_none();
        if inserted {
            Self::status_notifier_item_registered(&emitter, &address).await?;
        }
        Ok(())
    }
    async fn register_status_notifier_host(&self, service: &str, #[zbus(header)] header: Header<'_>, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let owner = header.sender().ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing sender".into()))?.to_string();
        let inserted = self.0.lock().unwrap_or_else(|e| e.into_inner()).hosts.insert(format!("{owner}:{service}"));
        if inserted {
            Self::status_notifier_host_registered(&emitter).await?;
        }
        Ok(())
    }
    #[zbus(signal)] async fn status_notifier_item_registered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)] async fn status_notifier_item_unregistered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)] async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)] async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Owns the fallback task. Dropping it cancels service and releases the bus name.
pub struct Handle(Option<tokio::sync::oneshot::Sender<()>>);
impl Drop for Handle { fn drop(&mut self) { if let Some(stop) = self.0.take() { let _ = stop.send(()); } } }

/// Serve the standard watcher only while its well-known name is available.
pub fn spawn(connection: Connection) -> Handle {
    let (stop, stopped) = tokio::sync::oneshot::channel();
    tokio::spawn(async move { let _ = run(connection, stopped).await; });
    Handle(Some(stop))
}

async fn run(connection: Connection, mut stop: tokio::sync::oneshot::Receiver<()>) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&connection).await?;
    let mut owners = dbus.receive_name_owner_changed().await?;
    let state = Arc::new(Mutex::new(State::default()));
    let mut active = false;
    loop {
        if !active {
            if dbus.get_name_owner(NAME.try_into().expect("valid watcher bus name")).await.is_err() {
                connection.object_server().at(PATH, Watcher(state.clone())).await?;
                let flags = RequestNameFlags::DoNotQueue | RequestNameFlags::AllowReplacement;
                active = connection.request_name_with_flags(NAME, flags).await? == RequestNameReply::PrimaryOwner;
                if !active { let _ = connection.object_server().remove::<Watcher, _>(PATH).await; }
            }
        }
        let change = tokio::select! { _ = &mut stop => break, change = owners.next() => change };
        let Some(change) = change else { break };
        let change = change.args()?;
        if change.name().as_str() == NAME {
            let mine = connection.unique_name().is_some_and(|name| change.new_owner().as_ref() == Some(name));
            if active && !mine && !change.new_owner().is_some_and(|owner| owner.as_str().is_empty()) {
                active = false;
                clear(&connection, &state).await?;
                let _ = connection.object_server().remove::<Watcher, _>(PATH).await;
            } else if !active && change.new_owner().is_none_or(|owner| owner.as_str().is_empty()) {
                continue;
            }
        } else if active && change.new_owner().is_none_or(|owner| owner.as_str().is_empty()) {
            remove_owner(&connection, &state, change.name().as_str()).await?;
        }
    }
    if active { clear(&connection, &state).await?; let _ = connection.object_server().remove::<Watcher, _>(PATH).await; let _ = connection.release_name(NAME).await; }
    Ok(())
}

async fn remove_owner(connection: &Connection, state: &Arc<Mutex<State>>, owner: &str) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(connection, PATH)?;
    let (items, hosts) = {
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let items: Vec<_> = state.items.iter().filter(|(_, value)| value == &owner).map(|(key, _)| key.clone()).collect();
        for item in &items { state.items.remove(item); }
        let old_len = state.hosts.len();
        state.hosts.retain(|host| !host.starts_with(&format!("{owner}:")));
        (items, old_len != state.hosts.len())
    };
    for item in items { Watcher::status_notifier_item_unregistered(&emitter, &item).await?; }
    if hosts { Watcher::status_notifier_host_unregistered(&emitter).await?; }
    Ok(())
}

async fn clear(connection: &Connection, state: &Arc<Mutex<State>>) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(connection, PATH)?;
    let (items, hosts) = { let mut state = state.lock().unwrap_or_else(|e| e.into_inner()); let items = std::mem::take(&mut state.items).into_keys().collect::<Vec<_>>(); let hosts = std::mem::take(&mut state.hosts); (items, hosts) };
    for item in items { Watcher::status_notifier_item_unregistered(&emitter, &item).await?; }
    for _ in hosts { Watcher::status_notifier_host_unregistered(&emitter).await?; }
    Ok(())
}
