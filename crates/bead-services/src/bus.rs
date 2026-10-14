// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{
    Action, Battery, Bluetooth, MenuEntry, Network, TrayItem,
    runtime::{Command, Shared},
};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use system_tray::{
    client::Client,
    menu::{MenuItem, MenuType, ToggleState},
};
use tokio::sync::mpsc;
use zbus::{
    Connection, MessageStream, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};
type Properties = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

pub fn run(shared: Arc<Shared>, mut receiver: mpsc::Receiver<Command>) {
    super::runtime::block_worker_signals();
    let mut retry = 0;
    while !shared.stop.load(Ordering::SeqCst) {
        // A bus disconnect ends this executor, cancelling the tray client's owned
        // signal tasks before a replacement connection starts.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("service runtime");
        let result = runtime.block_on(connect(&shared, &mut receiver));
        drop(runtime);
        match result {
            Ok(()) => break,
            Err(error) => eprintln!("SERVICE_BUS disconnected: {error}"),
        }
        shared.change(|s| {
            s.battery = Battery::default();
            s.network = Network::default();
            s.bluetooth = Bluetooth::default();
            s.tray.clear();
        });
        while let Ok(command) = receiver.try_recv() {
            match command {
                Command::Stop => return,
                Command::Tasks(reply) => {
                    let _ = reply.send((0, 0));
                }
                Command::Action(_, reply) => {
                    let _ = reply.send(Err("D-Bus disconnected".into()));
                }
            }
        }
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        retry = (retry + 1).min(3);
        let guard = shared.audio_wait.0.lock().unwrap();
        let _ = shared
            .audio_wait
            .1
            .wait_timeout(guard, Duration::from_millis(200u64 << retry));
    }
}
async fn system_connection() -> zbus::Result<Connection> {
    if let Ok(address) = std::env::var("DBUS_SYSTEM_BUS_ADDRESS") {
        zbus::connection::Builder::address(address.as_str())?
            .build()
            .await
    } else {
        Connection::system().await
    }
}
async fn connect(
    shared: &Arc<Shared>,
    receiver: &mut mpsc::Receiver<Command>,
) -> Result<(), Box<dyn std::error::Error>> {
    let system = system_connection().await?;
    let rules = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();
    let mut signals = MessageStream::for_match_rule(rules, &system, Some(64)).await?;
    let tray = Client::new().await?;
    let tray_actions = Connection::session().await?;
    let mut tray_events = tray.subscribe();
    refresh_sensors(shared, &system).await;
    refresh_tray(shared, &tray);
    shared.change(|s| s.clock = clock());
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let delay = 60
            - SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                % 60;
        tokio::select! {
            _=tokio::time::sleep(Duration::from_secs(delay))=>shared.change(|s|s.clock=clock()),
            command=receiver.recv()=>match command {
                Some(Command::Stop)|None=>return Ok(()),
                Some(Command::Tasks(reply))=>{ let retained=tray.retained_watcher_tasks().await.unwrap_or(0); let _=reply.send((tokio::runtime::Handle::current().metrics().num_alive_tasks(),retained)); },
                Some(Command::Action(action,reply))=>{
                    let result=tokio::time::timeout(Duration::from_secs(2),tray_action(&tray, &tray_actions, action)).await.map_err(|_|"D-Bus action timeout".to_string()).and_then(|r|r);
                    let _=reply.send(result);
                }
            },
            _=tray_actions.closed()=>return Err("session bus EOF".into()),
            event=tray_events.recv()=>{
                match event {Ok(_)|Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>refresh_tray(shared,&tray), Err(error)=>return Err(error.into())}
            },
            message=signals.next()=>{
                let message=message.ok_or("system bus EOF")??;
                let header=message.header();
                let interface=header.interface().map(|s|s.as_str()).unwrap_or("");
                let path=header.path().map(|s|s.as_str()).unwrap_or("");
                let owner_change=interface=="org.freedesktop.DBus" && header.member().is_some_and(|s|s.as_str()=="NameOwnerChanged") && message.body().deserialize::<(String,String,String)>().is_ok_and(|(name,_,_)|["org.freedesktop.UPower","org.freedesktop.NetworkManager","org.bluez"].contains(&name.as_str()));
                if owner_change || path.starts_with("/org/freedesktop/UPower") || path.starts_with("/org/freedesktop/NetworkManager") || path.starts_with("/org/bluez") || interface=="org.freedesktop.DBus.ObjectManager" {refresh_sensors(shared,&system).await;}
            }
        }
    }
}
fn clock() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: valid distinct pointers; localtime_r initializes tm on success.
    unsafe {
        if libc::localtime_r(&seconds, tm.as_mut_ptr()).is_null() {
            return "--:--".into();
        }
        let tm = tm.assume_init();
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}
async fn properties(
    connection: &Connection,
    name: &str,
    path: &str,
    interface: &str,
) -> zbus::Result<Properties> {
    Proxy::new(connection, name, path, "org.freedesktop.DBus.Properties")
        .await?
        .call("GetAll", &(interface,))
        .await
}
fn boolean(p: &Properties, name: &str) -> bool {
    p.get(name)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn number(p: &Properties, name: &str) -> Option<f64> {
    p.get(name).and_then(|v| f64::try_from(v).ok())
}
fn uint(p: &Properties, name: &str) -> u32 {
    p.get(name).and_then(|v| u32::try_from(v).ok()).unwrap_or(0)
}
fn text(p: &Properties, name: &str) -> String {
    p.get(name)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("")
        .into()
}
async fn refresh_sensors(shared: &Shared, connection: &Connection) {
    let battery = tokio::time::timeout(Duration::from_secs(1), async {
        let proxy = Proxy::new(
            connection,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await?;
        let path: OwnedObjectPath = proxy.call("GetDisplayDevice", &()).await?;
        let p = properties(
            connection,
            "org.freedesktop.UPower",
            path.as_str(),
            "org.freedesktop.UPower.Device",
        )
        .await?;
        let percentage =
            number(&p, "Percentage").filter(|v| v.is_finite() && (0.0..=100.0).contains(v));
        Ok::<_, zbus::Error>(Battery {
            available: boolean(&p, "IsPresent") && uint(&p, "Type") == 2 && percentage.is_some(),
            percentage,
            charging: matches!(uint(&p, "State"), 1 | 4),
        })
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    let network = tokio::time::timeout(Duration::from_secs(1), async {
        let p = properties(
            connection,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;
        let mut network = Network {
            available: true,
            connected: uint(&p, "State") >= 50,
            wireless_enabled: boolean(&p, "WirelessEnabled"),
            ..Default::default()
        };
        let primary = p
            .get("PrimaryConnection")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| OwnedObjectPath::try_from(v).ok())
            .map_or_else(String::new, |p| p.to_string());
        if primary != "/"
            && !primary.is_empty()
            && let Ok(active) = properties(
                connection,
                "org.freedesktop.NetworkManager",
                &primary,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await
        {
            network.name = text(&active, "Id");
        }
        Ok::<_, zbus::Error>(network)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    let bluetooth = tokio::time::timeout(Duration::from_secs(1), async {
        let proxy = Proxy::new(
            connection,
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
        )
        .await?;
        let objects: Objects = proxy.call("GetManagedObjects", &()).await?;
        let mut bluetooth = Bluetooth::default();
        for interfaces in objects.values() {
            if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
                bluetooth.available = true;
                bluetooth.powered |= boolean(adapter, "Powered");
            }
            if let Some(device) = interfaces.get("org.bluez.Device1")
                && boolean(device, "Connected")
            {
                bluetooth.connected_devices.push(text(device, "Alias"));
            }
        }
        bluetooth.connected_devices.sort();
        Ok::<_, zbus::Error>(bluetooth)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    shared.change(|s| {
        s.battery = battery;
        s.network = network;
        s.bluetooth = bluetooth;
    });
}
fn menu(entry: &MenuItem) -> MenuEntry {
    MenuEntry {
        id: entry.id,
        label: entry
            .label
            .clone()
            .unwrap_or_default()
            .replace("__", "\0")
            .replace('_', "")
            .replace('\0', "_"),
        enabled: entry.enabled,
        visible: entry.visible,
        separator: matches!(entry.menu_type, MenuType::Separator),
        checked: match entry.toggle_state {
            ToggleState::On => Some(true),
            ToggleState::Off => Some(false),
            _ => None,
        },
        children: entry.submenu.iter().map(menu).collect(),
    }
}
fn refresh_tray(shared: &Shared, client: &Client) {
    let items = client.items();
    let items = items.lock().unwrap();
    let mut tray: Vec<_> = items
        .iter()
        .map(|(id, (item, layout))| TrayItem {
            id: id.clone(),
            title: item.title.clone().unwrap_or_default(),
            visible: item.status != system_tray::item::Status::Passive,
            item_is_menu: item.item_is_menu,
            icon_name: if matches!(item.status, system_tray::item::Status::NeedsAttention) {
                item.attention_icon_name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .or_else(|| item.icon_name.clone())
            } else {
                item.icon_name.clone()
            }
            .unwrap_or_default(),
            icon_theme_path: item.icon_theme_path.clone().unwrap_or_default(),
            icon_pixmap: (if matches!(item.status, system_tray::item::Status::NeedsAttention) {
                item.attention_icon_pixmap
                    .as_ref()
                    .filter(|p| !p.is_empty())
                    .or(item.icon_pixmap.as_ref())
            } else {
                item.icon_pixmap.as_ref()
            })
            .into_iter()
            .flatten()
            .filter(|p| {
                p.width > 0
                    && p.height > 0
                    && p.width <= 512
                    && p.height <= 512
                    && p.pixels.len() == (p.width * p.height * 4) as usize
            })
            .map(|p| (p.width, p.height, p.pixels.clone()))
            .collect(),
            attention: matches!(item.status, system_tray::item::Status::NeedsAttention),
            menu: layout
                .as_ref()
                .map_or_else(Vec::new, |m| m.submenus.iter().map(menu).collect()),
        })
        .collect();
    tray.sort_by(|a, b| a.id.cmp(&b.id));
    drop(items);
    shared.change(|s| s.tray = tray);
}
async fn tray_action(
    client: &Client,
    connection: &Connection,
    action: Action,
) -> Result<(), String> {
    let (id, entry, prepare, x, y) = match action {
        Action::TrayActivate { item, x, y } => (item, None, false, x, y),
        Action::TrayMenu { item, entry } => (item, Some(entry), false, 0, 0),
        Action::TrayPrepare { item } => (item, None, true, 0, 0),
        _ => return Err("wrong service action".into()),
    };
    let menu_path = client
        .items()
        .lock()
        .unwrap()
        .get(&id)
        .ok_or("unknown/stale tray item")?
        .0
        .menu
        .clone();
    if prepare {
        return client
            .about_to_show_menuitem(id, menu_path.ok_or("tray item has no menu")?, 0)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    // Client::activate suppresses some method/timeout errors; use direct zbus calls so failure is observable.
    let (destination, path) = id.split_once('/').map_or(
        (id.as_str(), "/StatusNotifierItem".to_string()),
        |(name, path)| (name, format!("/{path}")),
    );
    if let Some(entry) = entry {
        let proxy = Proxy::new(
            connection,
            destination,
            menu_path.ok_or("tray item has no menu")?,
            "com.canonical.dbusmenu",
        )
        .await
        .map_err(|e| e.to_string())?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        proxy
            .call::<_, _, ()>(
                "Event",
                &(
                    entry,
                    "clicked",
                    zbus::zvariant::Value::from(0i32),
                    timestamp,
                ),
            )
            .await
            .map_err(|e| e.to_string())
    } else {
        Proxy::new(connection, destination, path, "org.kde.StatusNotifierItem")
            .await
            .map_err(|e| e.to_string())?
            .call::<_, _, ()>("Activate", &(x, y))
            .await
            .map_err(|e| e.to_string())
    }
}
