// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic native SNI application, only on explicitly private proof buses.
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Structure, Value};

type Pixmap = (i32, i32, Vec<u8>);
type Tooltip = (&'static str, Vec<Pixmap>, &'static str, &'static str);
static CALLS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
fn record(call: String) {
    CALLS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(call);
}
pub fn take_calls() -> Vec<String> {
    std::mem::take(
        &mut *CALLS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner()),
    )
}

struct Item {
    generation: u32,
}
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }
    #[zbus(property)]
    fn id(&self) -> &str {
        "bead-fixture"
    }
    #[zbus(property)]
    fn title(&self) -> String {
        format!("Private tray fixture {}", self.generation)
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        if self.generation >= 3 {
            "NeedsAttention"
        } else {
            "Active"
        }
    }
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        if self.generation == 0 {
            "bead-missing-theme-icon"
        } else {
            "folder"
        }
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        // A transparent pearl with a dark inset: visibly different from a square.
        let mut pixels = Vec::new();
        for y in 0..24i32 {
            for x in 0..24i32 {
                let r = (x - 12).pow(2) + (y - 12).pow(2);
                pixels.extend(if r > 100 {
                    [0, 0, 0, 0]
                } else if r < 25 {
                    [255, 80, 65, 40]
                } else {
                    [255, 235, 227, 177]
                });
            }
        }
        vec![(24, 24, pixels)]
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        if self.generation == 3 {
            vec![(8, 8, vec![255; 8 * 8 * 4])]
        } else {
            vec![]
        }
    }
    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn tool_tip(&self) -> Tooltip {
        ("", vec![], "Private tray fixture", "")
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        std::env::var_os("BEAD_FIXTURE_ITEM_IS_MENU").is_some()
    }
    #[zbus(signal)]
    async fn new_title(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_icon(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_attention_icon(
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_status(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/Menu").unwrap()
    }
    fn activate(&self, x: i32, y: i32) {
        record(format!("activate {x} {y}"));
        println!("TRAY_ACTIVATE {x} {y}");
    }
    fn context_menu(&self, x: i32, y: i32) {
        record(format!("context {x} {y}"));
        println!("TRAY_CONTEXT {x} {y}");
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        record(format!("secondary {x} {y}"));
        println!("TRAY_SECONDARY {x} {y}");
    }
    fn scroll(&self, delta: i32, orientation: &str) {
        record(format!("scroll {delta} {orientation}"));
        println!("TRAY_SCROLL {delta} {orientation}");
    }
}
struct Menu(std::sync::Arc<AtomicU32>);
type Properties = HashMap<String, OwnedValue>;
type Layout = (i32, Properties, Vec<OwnedValue>);
fn properties() -> Properties {
    HashMap::from([
        (
            "label".into(),
            OwnedValue::try_from(Value::from("_Open fixture")).unwrap(),
        ),
        ("enabled".into(), OwnedValue::from(true)),
        ("visible".into(), OwnedValue::from(true)),
    ])
}
fn properties_with_label(label: &'static str) -> Properties {
    let mut properties = properties();
    properties.insert(
        "label".into(),
        OwnedValue::try_from(Value::from(label)).unwrap(),
    );
    properties
}
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> (u32, Layout) {
        let _ = (recursion_depth, property_names);
        if parent_id != 0 {
            let children = if parent_id == 2 {
                let nested_leaf = Structure::from((
                    3i32,
                    if self.0.load(Ordering::Relaxed) == 2 {
                        properties_with_label("_Prepared action")
                    } else {
                        properties_with_label("_Nested action")
                    },
                    Vec::<OwnedValue>::new(),
                ));
                vec![OwnedValue::try_from(Value::from(nested_leaf)).unwrap()]
            } else {
                Vec::new()
            };
            return (1, (parent_id, properties_with_label("_Submenu"), children));
        }
        let leaf_props = match self.0.load(Ordering::Relaxed) {
            0 => properties(),
            2 => properties_with_label("_Prepared action"),
            _ => properties_with_label("_Updated action"),
        };
        let leaf = Structure::from((1i32, leaf_props, Vec::<OwnedValue>::new()));
        let nested_leaf = Structure::from((
            3i32,
            if self.0.load(Ordering::Relaxed) == 2 {
                properties_with_label("_Prepared action")
            } else {
                properties_with_label("_Nested action")
            },
            Vec::<OwnedValue>::new(),
        ));
        let submenu = Structure::from((
            2i32,
            properties_with_label("_Submenu"),
            vec![OwnedValue::try_from(Value::from(nested_leaf)).unwrap()],
        ));
        (
            1,
            (
                0,
                HashMap::new(),
                vec![
                    OwnedValue::try_from(Value::from(leaf)).unwrap(),
                    OwnedValue::try_from(Value::from(submenu)).unwrap(),
                ],
            ),
        )
    }
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let _ = (ids, property_names);
        vec![(1, properties())]
    }
    fn about_to_show(&self, id: i32) -> bool {
        record(format!("prepare {id}"));
        println!("TRAY_PREPARE {id}");
        if id == 2 {
            self.0.store(2, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
    fn event(&self, id: i32, event_id: &str, data: Value<'_>, timestamp: u32) {
        let _ = data;
        record(format!("menu {id} {event_id}"));
        println!("TRAY_MENU {id} {event_id} {timestamp}");
    }
    #[zbus(signal)]
    async fn layout_updated(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        updated: Vec<(i32, Properties)>,
        removed: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;
}
pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref() != Ok("1") {
        return Err("requires explicit private proof bus".into());
    }
    let initial_generation = std::env::var("BEAD_FIXTURE_GENERATION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let generation = std::sync::Arc::new(AtomicU32::new(initial_generation));
    let connection = zbus::connection::Builder::session()?
        .serve_at(
            "/StatusNotifierItem",
            Item {
                generation: initial_generation,
            },
        )?
        .serve_at("/Menu", Menu(generation.clone()))?
        .build()
        .await?;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
    let watcher_name: zbus::names::BusName<'static> = "org.kde.StatusNotifierWatcher".try_into()?;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if dbus
                .name_has_owner(watcher_name.clone())
                .await
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await?;
    let mut owner_changes = dbus.receive_name_owner_changed().await?;
    let item_connection = connection.clone();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        while let Some(change) = owner_changes.next().await {
            let Ok(args) = change.args() else { continue };
            if args.name.as_str() == "org.kde.StatusNotifierWatcher" && args.new_owner().is_some() {
                let watcher = match zbus::Proxy::new(
                    &item_connection,
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    "org.kde.StatusNotifierWatcher",
                )
                .await
                {
                    Ok(watcher) => watcher,
                    Err(_) => continue,
                };
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
                while tokio::time::Instant::now() < deadline {
                    if watcher
                        .get_property::<bool>("IsStatusNotifierHostRegistered")
                        .await
                        .unwrap_or(false)
                        && watcher
                            .call::<_, _, ()>(
                                "RegisterStatusNotifierItem",
                                &("/StatusNotifierItem",),
                            )
                            .await
                            .is_ok()
                    {
                        record("reregister".into());
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        }
    });
    let watcher = zbus::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await?;
    watcher
        .call::<_, _, ()>("RegisterStatusNotifierItem", &("/StatusNotifierItem",))
        .await?;
    println!("TRAY_FIXTURE_READY");
    if std::env::var_os("BEAD_FIXTURE_UPDATE").is_some() {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let item = connection
            .object_server()
            .interface::<_, Item>("/StatusNotifierItem")
            .await?;
        item.get_mut().await.generation = 1;
        Item::new_title(item.signal_emitter()).await?;
        Item::new_icon(item.signal_emitter()).await?;
        generation.store(1, Ordering::Relaxed);
        let menu = connection
            .object_server()
            .interface::<_, Menu>("/Menu")
            .await?;
        let emitter = menu.signal_emitter();
        Menu::layout_updated(emitter, 2, 2).await?;
        Menu::items_properties_updated(
            emitter,
            vec![(1, properties_with_label("_Updated action"))],
            vec![],
        )
        .await?;
        println!("TRAY_UPDATED");
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        item.get_mut().await.generation = 3;
        generation.store(3, Ordering::Relaxed);
        Item::new_attention_icon(item.signal_emitter()).await?;
        Item::new_status(item.signal_emitter()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        item.get_mut().await.generation = 4;
        generation.store(4, Ordering::Relaxed);
        Item::new_attention_icon(item.signal_emitter()).await?;
    }
    tokio::select! {
        _ = connection.closed() => {},
        _ = std::future::pending::<()>() => {},
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run().await
}
