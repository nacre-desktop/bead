// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{
    Action, Battery, Bluetooth, MenuEntry, Network, Session, TrayItem,
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
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("SERVICE_BUS runtime unavailable: {error}");
                break;
            }
        };
        let result = runtime.block_on(connect(&shared, &mut receiver));
        drop(runtime);
        match result {
            Ok(()) => break,
            Err(error) => eprintln!("SERVICE_BUS disconnected: {error}"),
        }
        shared.change(|s| {
            s.battery = Battery::default();
            s.session = Session::default();
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
        let guard = shared
            .audio_wait
            .0
            .lock()
            .unwrap_or_else(crate::runtime::poisoned);
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
    refresh_lock(shared, &system).await;
    refresh_tray(shared, &tray);
    shared.change(update_clock);
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
            _=tokio::time::sleep(Duration::from_secs(delay))=>shared.change(update_clock),
            command=receiver.recv()=>match command {
                Some(Command::Stop)|None=>return Ok(()),
                Some(Command::Tasks(reply))=>{ let retained=tray.retained_watcher_tasks().await.unwrap_or(0); let _=reply.send((tokio::runtime::Handle::current().metrics().num_alive_tasks(),retained)); },
                Some(Command::Action(action,reply))=>{
                    let result=tokio::time::timeout(Duration::from_secs(2),tray_action(shared, &tray, &tray_actions, action)).await.map_err(|_|"D-Bus action timeout".to_string()).and_then(|r|r);
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
                if path.starts_with("/org/freedesktop/login1") {
                    if header.member().is_some_and(|s| s.as_str() == "PrepareForSleep")
                        && let Ok(sleeping) = message.body().deserialize::<bool>()
                    {
                        shared.change(|s| s.session.sleeping = sleeping);
                    }
                    refresh_lock(shared, &system).await;
                }
                if owner_change || path.starts_with("/org/freedesktop/UPower") || path.starts_with("/org/freedesktop/NetworkManager") || path.starts_with("/org/bluez") || interface=="org.freedesktop.DBus.ObjectManager" {refresh_sensors(shared,&system).await;}
            }
        }
    }
}
/// `LockedHint` of the caller's own session (`GetSession("auto")`); unknown reads as unlocked.
async fn refresh_lock(shared: &Shared, connection: &Connection) {
    let locked = tokio::time::timeout(Duration::from_secs(1), async {
        let manager = Proxy::new(
            connection,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        let path: OwnedObjectPath = manager.call("GetSession", &("auto",)).await?;
        let session = properties(
            connection,
            "org.freedesktop.login1",
            path.as_str(),
            "org.freedesktop.login1.Session",
        )
        .await?;
        Ok::<_, zbus::Error>(boolean(&session, "LockedHint"))
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(false);
    shared.change(|s| s.session.locked = locked);
}
fn update_clock(snapshot: &mut crate::Snapshot) {
    let (text, date) = clock();
    snapshot.clock = text;
    snapshot.date = date;
}
fn clock() -> (String, Option<crate::LocalDate>) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| libc::time_t::try_from(value.as_secs()).ok());
    let Some(seconds) = seconds else {
        return ("--:--".into(), None);
    };
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: valid distinct pointers; localtime_r initializes tm on success.
    unsafe {
        if libc::localtime_r(&seconds, tm.as_mut_ptr()).is_null() {
            return ("--:--".into(), None);
        }
        let tm = tm.assume_init();
        (
            format!("{:02}:{:02}", tm.tm_hour, tm.tm_min),
            civil_date(&tm),
        )
    }
}
fn civil_date(tm: &libc::tm) -> Option<crate::LocalDate> {
    let year = tm.tm_year.checked_add(1900)?;
    let month = tm.tm_mon.checked_add(1)?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&tm.tm_mday)
    {
        return None;
    }
    Some(crate::LocalDate {
        year: year as u16,
        month: month as u8,
        day: tm.tm_mday as u8,
    })
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
        let on_battery = properties(
            connection,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await
        .is_ok_and(|root| boolean(&root, "OnBattery"));
        Ok::<_, zbus::Error>(Battery {
            available: boolean(&p, "IsPresent") && uint(&p, "Type") == 2 && percentage.is_some(),
            percentage,
            charging: matches!(uint(&p, "State"), 1 | 4),
            on_battery,
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
const MAX_TRAY_ITEMS: usize = 64;
const MAX_MENU_DEPTH: usize = 8;
const MAX_MENU_ENTRIES: usize = 256;
const MAX_TRAY_PIXMAP_BYTES: usize = 1024 * 1024;

fn bounded_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

fn valid_pixmaps(pixmaps: Option<&Vec<system_tray::item::IconPixmap>>) -> Vec<(i32, i32, Vec<u8>)> {
    let mut total = 0usize;
    pixmaps
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let expected = usize::try_from(p.width)
                .ok()?
                .checked_mul(usize::try_from(p.height).ok()?)?
                .checked_mul(4)?;
            if p.width <= 0
                || p.height <= 0
                || p.width > 256
                || p.height > 256
                || expected != p.pixels.len()
                || total.saturating_add(expected) > MAX_TRAY_PIXMAP_BYTES
            {
                return None;
            }
            total += expected;
            Some((p.width, p.height, p.pixels.clone()))
        })
        .collect()
}

fn menu_has_clickable_leaf(entries: &[MenuItem], id: i32, ancestors_enabled: bool) -> bool {
    entries.iter().any(|entry| {
        let available = ancestors_enabled
            && entry.enabled
            && entry.visible
            && !matches!(entry.menu_type, MenuType::Separator);
        if entry.id == id {
            available && entry.submenu.is_empty()
        } else {
            menu_has_clickable_leaf(&entry.submenu, id, available)
        }
    })
}

fn menu_has_visible_enabled_submenu(
    entries: &[MenuItem],
    id: i32,
    ancestors_enabled: bool,
) -> bool {
    entries.iter().any(|entry| {
        let available = ancestors_enabled
            && entry.enabled
            && entry.visible
            && !matches!(entry.menu_type, MenuType::Separator);
        if entry.id == id {
            available && !entry.submenu.is_empty()
        } else {
            menu_has_visible_enabled_submenu(&entry.submenu, id, available)
        }
    })
}

fn menu(
    entry: &MenuItem,
    depth: usize,
    budget: &mut usize,
    parent_enabled: bool,
) -> Option<MenuEntry> {
    if depth > MAX_MENU_DEPTH || *budget == 0 {
        return None;
    }
    *budget -= 1;
    let enabled = entry.enabled && parent_enabled;
    MenuEntry {
        id: entry.id,
        label: bounded_text(entry.label.as_deref().unwrap_or_default(), 512)
            .replace("__", "\0")
            .replace('_', "")
            .replace('\0', "_"),
        enabled,
        visible: entry.visible,
        separator: matches!(entry.menu_type, MenuType::Separator),
        checked: match entry.toggle_state {
            ToggleState::On => Some(true),
            ToggleState::Off => Some(false),
            _ => None,
        },
        children: entry
            .submenu
            .iter()
            .filter_map(|child| menu(child, depth + 1, budget, enabled))
            .collect(),
    }
    .into()
}
fn refresh_tray(shared: &Shared, client: &Client) {
    let items = client.items();
    let items = items.lock().unwrap_or_else(crate::runtime::poisoned);
    let mut tray: Vec<_> = items
        .iter()
        .filter(|(_, (item, _))| item.status != system_tray::item::Status::Passive)
        .take(MAX_TRAY_ITEMS)
        .map(|(id, (item, layout))| TrayItem {
            id: bounded_text(id, 512),
            title: bounded_text(item.title.as_deref().unwrap_or_default(), 512),
            tooltip_title: bounded_text(
                item.tool_tip.as_ref().map_or("", |tip| tip.title.as_str()),
                512,
            ),
            tooltip_description: bounded_text(
                item.tool_tip
                    .as_ref()
                    .map_or("", |tip| tip.description.as_str()),
                2048,
            ),
            visible: true,
            item_is_menu: item.item_is_menu,
            has_menu: item.menu.is_some(),
            icon_name: bounded_text(
                &if matches!(item.status, system_tray::item::Status::NeedsAttention) {
                    item.attention_icon_name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .or_else(|| item.icon_name.clone())
                } else {
                    item.icon_name.clone()
                }
                .unwrap_or_default(),
                512,
            ),
            icon_theme_path: bounded_text(
                item.icon_theme_path.as_deref().unwrap_or_default(),
                1024,
            ),
            icon_pixmap: if matches!(item.status, system_tray::item::Status::NeedsAttention) {
                let attention = valid_pixmaps(item.attention_icon_pixmap.as_ref());
                if attention.is_empty() {
                    valid_pixmaps(item.icon_pixmap.as_ref())
                } else {
                    attention
                }
            } else {
                valid_pixmaps(item.icon_pixmap.as_ref())
            },
            attention: matches!(item.status, system_tray::item::Status::NeedsAttention),
            menu: layout.as_ref().map_or_else(Vec::new, |m| {
                let mut budget = MAX_MENU_ENTRIES;
                m.submenus
                    .iter()
                    .filter_map(|entry| menu(entry, 0, &mut budget, true))
                    .collect()
            }),
        })
        .collect();
    tray.sort_by(|a, b| a.id.cmp(&b.id));
    drop(items);
    shared.change(|s| s.tray = tray);
}
async fn tray_action(
    shared: &Shared,
    client: &Client,
    connection: &Connection,
    action: Action,
) -> Result<(), String> {
    let (id, operation, x, y) = match action {
        Action::TrayActivate { item, x, y } => (item, "activate", x, y),
        Action::TraySecondaryActivate { item, x, y } => (item, "secondary", x, y),
        Action::TrayContextMenu { item, x, y } => (item, "context", x, y),
        Action::TrayScroll {
            item,
            delta,
            horizontal,
        } => (
            item,
            if horizontal {
                "scroll-horizontal"
            } else {
                "scroll-vertical"
            },
            delta,
            0,
        ),
        Action::TrayMenu { item, entry } => {
            let items = client.items();
            let current = items.lock().map_err(|_| "tray lock poisoned")?;
            let menu = current
                .get(&item)
                .and_then(|(_, layout)| layout.as_ref())
                .ok_or("tray item has no current menu")?;
            if !menu_has_clickable_leaf(&menu.submenus, entry, true) {
                return Err("unknown, stale, or disabled tray menu entry".into());
            }
            drop(current);
            (item, "menu", entry, 0)
        }
        Action::TrayPrepareEntry { item, entry } => {
            let items = client.items();
            let current = items.lock().map_err(|_| "tray lock poisoned")?;
            let menu = current
                .get(&item)
                .and_then(|(_, layout)| layout.as_ref())
                .ok_or("tray item has no current menu")?;
            if !menu_has_visible_enabled_submenu(&menu.submenus, entry, true) {
                return Err("unknown, stale, or disabled tray submenu".into());
            }
            drop(current);
            (item, "prepare-entry", entry, 0)
        }
        Action::TrayPrepare { item } => (item, "prepare", 0, 0),
        _ => return Err("wrong service action".into()),
    };
    if id.len() > 512 {
        return Err("tray item identity exceeds 512 bytes".into());
    }
    if operation.starts_with("scroll") && !(-120..=120).contains(&x) {
        return Err("tray scroll delta exceeds the supported range".into());
    }
    let items = client.items();
    let item_is_menu = items
        .lock()
        .map_err(|_| "tray lock poisoned")?
        .get(&id)
        .ok_or("unknown/stale tray item")?
        .0
        .item_is_menu;
    let menu_path = client
        .items()
        .lock()
        .map_err(|_| "tray lock poisoned")?
        .get(&id)
        .ok_or("unknown/stale tray item")?
        .0
        .menu
        .clone();
    if operation == "prepare" || operation == "prepare-entry" {
        let changed = client
            .about_to_show_menuitem(
                id,
                menu_path.ok_or("tray item has no menu")?,
                if operation == "prepare-entry" { x } else { 0 },
            )
            .await
            .map_err(|e| e.to_string());
        if changed.as_ref().is_ok_and(|updated| *updated) {
            refresh_tray(shared, client);
        }
        return changed.map(|_| ());
    }
    // Client::activate suppresses some method/timeout errors; use direct zbus calls so failure is observable.
    let (destination, path) = id.split_once('/').map_or(
        (id.as_str(), "/StatusNotifierItem".to_string()),
        |(name, path)| (name, format!("/{path}")),
    );
    if operation == "menu" {
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
                &(x, "clicked", zbus::zvariant::Value::from(0i32), timestamp),
            )
            .await
            .map_err(|e| e.to_string())
    } else {
        let proxy = Proxy::new(connection, destination, path, "org.kde.StatusNotifierItem")
            .await
            .map_err(|e| e.to_string())?;
        match operation {
            "secondary" => proxy
                .call::<_, _, ()>("SecondaryActivate", &(x, y))
                .await
                .map_err(|e| e.to_string()),
            "context" => proxy
                .call::<_, _, ()>("ContextMenu", &(x, y))
                .await
                .map_err(|e| e.to_string()),
            "scroll-horizontal" => proxy
                .call::<_, _, ()>("Scroll", &(x, "horizontal"))
                .await
                .map_err(|e| e.to_string()),
            "scroll-vertical" => proxy
                .call::<_, _, ()>("Scroll", &(x, "vertical"))
                .await
                .map_err(|e| e.to_string()),
            _ if item_is_menu => proxy
                .call::<_, _, ()>("ContextMenu", &(x, y))
                .await
                .map_err(|e| e.to_string()),
            _ => proxy
                .call::<_, _, ()>("Activate", &(x, y))
                .await
                .map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    #[test]
    fn civil_date_preserves_calendar_fields_without_changing_process_timezone() {
        // SAFETY: libc::tm has integer fields and a nullable timezone pointer.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = 124;
        tm.tm_mon = 1;
        tm.tm_mday = 29;
        assert_eq!(
            civil_date(&tm),
            Some(crate::LocalDate {
                year: 2024,
                month: 2,
                day: 29
            })
        );
        tm.tm_mon = 12;
        assert!(civil_date(&tm).is_none());
        tm.tm_mon = 0;
        tm.tm_mday = 0;
        assert!(civil_date(&tm).is_none());
        tm.tm_mday = 1;
        tm.tm_year = i32::MAX;
        assert!(civil_date(&tm).is_none());
    }
    #[test]
    fn existing_snapshot_json_without_date_still_decodes() {
        let mut old = serde_json::to_value(crate::Snapshot::default()).unwrap();
        old.as_object_mut().unwrap().remove("date");
        let decoded: crate::Snapshot = serde_json::from_value(old).unwrap();
        assert!(decoded.date.is_none());
    }

    #[test]
    fn tray_projection_bounds_text_pixmaps_and_menu_depth() {
        assert_eq!(bounded_text("a\n\u{7f}b", 1), "a");
        let invalid = system_tray::item::IconPixmap {
            width: i32::MAX,
            height: i32::MAX,
            pixels: vec![],
        };
        assert!(valid_pixmaps(Some(&vec![invalid])).is_empty());
        let child = MenuItem {
            id: 11,
            ..MenuItem::default()
        };
        let parent = MenuItem {
            id: 10,
            enabled: false,
            submenu: vec![child.clone()],
            ..MenuItem::default()
        };
        assert!(!menu_has_clickable_leaf(&[parent], 11, true));
        let hidden_child = MenuItem {
            id: 11,
            visible: false,
            ..MenuItem::default()
        };
        assert!(!menu_has_clickable_leaf(&[hidden_child], 11, true));
        let mut budget = 0;
        assert!(menu(&MenuItem::default(), 0, &mut budget, true).is_none());
    }
}
