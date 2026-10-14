// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic native SNI application, only on explicitly private proof buses.
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Structure, Value};

type Pixmap = (i32, i32, Vec<u8>);
type Tooltip = (&'static str, Vec<Pixmap>, &'static str, &'static str);

struct Item;
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
    fn title(&self) -> &str {
        "Private tray fixture"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![(16, 16, [255, 235, 227, 177].repeat(256))]
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
        vec![]
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
        true
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/Menu").unwrap()
    }
    fn activate(&self, x: i32, y: i32) {
        println!("TRAY_ACTIVATE {x} {y}");
    }
    fn context_menu(&self, x: i32, y: i32) {
        println!("TRAY_CONTEXT {x} {y}");
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        println!("TRAY_SECONDARY {x} {y}");
    }
    fn scroll(&self, delta: i32, orientation: &str) {
        println!("TRAY_SCROLL {delta} {orientation}");
    }
}
struct Menu;
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
        let _ = (parent_id, recursion_depth, property_names);
        let child = Structure::from((1i32, properties(), Vec::<OwnedValue>::new()));
        (
            1,
            (
                0,
                HashMap::new(),
                vec![OwnedValue::try_from(Value::from(child)).unwrap()],
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
        println!("TRAY_PREPARE {id}");
        false
    }
    fn event(&self, id: i32, event_id: &str, data: Value<'_>, timestamp: u32) {
        let _ = data;
        println!("TRAY_MENU {id} {event_id} {timestamp}");
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref() != Ok("1") {
        return Err("requires explicit private proof bus".into());
    }
    let connection = zbus::connection::Builder::session()?
        .serve_at("/StatusNotifierItem", Item)?
        .serve_at("/Menu", Menu)?
        .build()
        .await?;
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
    std::future::pending::<()>().await;
    Ok(())
}
