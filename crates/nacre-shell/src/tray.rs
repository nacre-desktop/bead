// SPDX-License-Identifier: GPL-3.0-only
//! Nacre tray presentation. The headless service owns D-Bus state.
use bead::{
    core::Value,
    services::{MenuEntry, Snapshot},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub type Selection = Arc<Mutex<String>>;

#[derive(Default)]
pub struct Menu {
    pub selection: Selection,
    item: String,
    path: Vec<i32>,
    reset_scroll: bool,
}
impl Menu {
    pub fn new(selection: Selection) -> Self {
        Self {
            selection,
            reset_scroll: true,
            ..Self::default()
        }
    }
    fn rows<'a>(&self, snapshot: &'a Snapshot) -> Option<&'a [MenuEntry]> {
        let item = snapshot
            .tray
            .iter()
            .find(|item| item.id == self.item && item.visible)?;
        let mut rows = item.menu.as_slice();
        for id in &self.path {
            let parent = rows
                .iter()
                .find(|row| row.id == *id && row.visible && row.enabled && !row.separator)?;
            rows = &parent.children;
        }
        Some(rows)
    }
    pub fn navigate(&mut self, snapshot: &Snapshot, id: i32) -> bool {
        if self.rows(snapshot).is_some_and(|rows| {
            rows.iter().any(|row| {
                row.id == id
                    && row.visible
                    && row.enabled
                    && !row.separator
                    && !row.children.is_empty()
            })
        }) {
            self.path.push(id);
            self.reset_scroll = true;
            true
        } else {
            false
        }
    }
    pub fn back(&mut self) {
        self.path.pop();
        self.reset_scroll = true;
    }
    pub fn values(&mut self, snapshot: &Snapshot) -> BTreeMap<String, Value> {
        let selected = self
            .selection
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if self.item != selected {
            self.item = selected;
            self.path.clear();
            self.reset_scroll = true;
        }
        if self.rows(snapshot).is_none() {
            self.path.clear();
            self.reset_scroll = true;
        }
        let item = snapshot
            .tray
            .iter()
            .find(|item| item.id == self.item && item.visible);
        let rows = self.rows(snapshot).unwrap_or_default();
        let values = rows.iter().filter(|row| row.visible).map(|row| {
            serde_json::json!({"id":format!("{}|{}",self.item,row.id),"entry":row.id,"label":row.label,
                "enabled":row.enabled,"separator":row.separator,"submenu":!row.children.is_empty(),
                "toggle":row.checked.map(|checked| if checked {"✓ "} else {"○ "}).unwrap_or("")})
        }).collect();
        let mut values = BTreeMap::from([
            (
                "tray.title".into(),
                Value::Text(item.map(|item| item.title.clone()).unwrap_or_default()),
            ),
            ("tray.rows".into(), Value::list(values)),
            (
                "tray.height".into(),
                Value::Number(
                    (56 + rows
                        .iter()
                        .filter(|row| row.visible)
                        .map(|row| if row.separator { 3 } else { 32 })
                        .sum::<usize>()
                        .min(400))
                    .max(96) as f64,
                ),
            ),
            ("tray.back".into(), Value::Bool(!self.path.is_empty())),
            (
                "tray.empty".into(),
                Value::Bool(rows.iter().all(|row| !row.visible)),
            ),
        ]);
        if std::mem::take(&mut self.reset_scroll) {
            values.insert("tray.scroll".into(), Value::Number(0.));
        }
        if item.is_none() {
            values.insert("panel.open".into(), Value::Bool(false));
        }
        values
    }
}

/// A lazy private/legacy menu shares the existing headless owner; no bus is
/// created by this factory. Core daemon integration awaits published contracts.
pub fn spec(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    owner: Arc<std::sync::OnceLock<Arc<bead::services::Desktop>>>,
    selection: Selection,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.width = 300.;
    fallback.bounds = bead::render::Rect::new(-32., -32., 364., 160.);
    fallback.primitives = vec![bead::render::Primitive::Rounded {
        rect: bead::render::Rect::new(0., 0., 300., 96.),
        radius: 12.,
        fill: colors.get("surface")?,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "tray-menu".into();
        motion.edge = bead::core::Edge::Top;
    }
    let cold = fallback.clone();
    Ok(bead::wayland::PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "tray/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let desktop = owner.get().cloned().ok_or("bar services not ready")?;
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_shared_desktop(desktop, false)
                .and_then(|runtime| runtime.with_tray_menu(selection.clone()))
                .and_then(|runtime| {
                    runtime.with_frame(crate::placement::Attachment::Popout, &colors)
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_preserves_disabled_separators_checks_and_submenu_boundaries() {
        let leaf = MenuEntry {
            id: 2,
            label: "Child".into(),
            visible: true,
            enabled: true,
            ..Default::default()
        };
        let mut snapshot = Snapshot {
            tray: vec![bead::services::TrayItem {
                id: "item".into(),
                visible: true,
                menu: vec![
                    MenuEntry {
                        id: 1,
                        label: "Parent".into(),
                        visible: true,
                        enabled: false,
                        children: vec![leaf],
                        ..Default::default()
                    },
                    MenuEntry {
                        id: 3,
                        visible: true,
                        separator: true,
                        ..Default::default()
                    },
                    MenuEntry {
                        id: 4,
                        visible: true,
                        enabled: true,
                        checked: Some(true),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut menu = Menu::new(Arc::new(Mutex::new("item".into())));
        let values = menu.values(&snapshot);
        let Value::List(rows) = &values["tray.rows"] else {
            panic!()
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["enabled"], false);
        assert_eq!(rows[1]["separator"], true);
        assert_eq!(rows[2]["toggle"], "✓ ");
        assert!(!menu.navigate(&snapshot, 1));
        snapshot.tray[0].menu[0].enabled = true;
        assert!(menu.navigate(&snapshot, 1));
        let values = menu.values(&snapshot);
        let Value::List(rows) = &values["tray.rows"] else {
            panic!()
        };
        assert_eq!(rows[0]["label"], "Child");
        assert_eq!(values["tray.back"], Value::Bool(true));
        snapshot.tray.clear();
        assert_eq!(menu.values(&snapshot)["panel.open"], Value::Bool(false));
        assert!(menu.path.is_empty());
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    #[test]
    fn menu_close_reload_and_drop_release_view_subscription() {
        use bead::wayland::{SceneSource, UiEvent};
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette = root.join("palette.json");
        let colors = bead::render::Palette::read(&palette).unwrap();
        let fallback = crate::sound::fallback(colors.get("surface").unwrap());
        let session =
            bead::ui::Session::new(root, "tray/main.luau".into(), palette, colors, fallback);
        let desktop = bead::services::Desktop::start_config(bead::services::Config {
            audio: false,
            bus: false,
        });
        let mut runtime = crate::reload::Runtime::panel_session(session, true)
            .with_shared_desktop(desktop.clone(), false)
            .unwrap()
            .with_tray_menu(Arc::new(Mutex::new("item".into())))
            .unwrap();
        runtime.services(Box::new(|_| {}));
        runtime.stage(UiEvent::Reload).unwrap();
        runtime.commit();
        assert_eq!(desktop.subscription_count(), 1);
        let snapshot = Arc::new(Snapshot {
            tray: vec![bead::services::TrayItem {
                id: "item".into(),
                visible: true,
                title: "Fixture".into(),
                menu: (1..=40)
                    .map(|id| MenuEntry {
                        id,
                        label: format!("Entry {id}"),
                        enabled: true,
                        visible: true,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        });
        runtime.stage(UiEvent::PanelVisibility(true)).unwrap();
        runtime.commit();
        runtime.stage(UiEvent::Desktop(snapshot.clone())).unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(true))
        );
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|node| node.action.as_deref() == Some("services.menu"))
            .unwrap();
        let rect = bead::ui::rect(&node.draw);
        runtime
            .stage(UiEvent::Scroll {
                x: rect.x + rect.w / 2.,
                y: rect.y + rect.h / 2.,
                delta: 30.,
            })
            .unwrap();
        runtime.commit();
        assert!(
            matches!(runtime.session.stores.get("tray.scroll"),Some(Value::Number(value)) if *value > 0.)
        );
        runtime
            .stage(UiEvent::Command {
                command: "escape".into(),
                shift: false,
            })
            .unwrap();
        runtime.commit();
        runtime.stage(UiEvent::Desktop(snapshot)).unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(false))
        );
        runtime.stage(UiEvent::Reload).unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(false))
        );
        runtime.stage(UiEvent::PanelVisibility(true)).unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(true))
        );
        runtime
            .stage(UiEvent::Desktop(Arc::new(Snapshot {
                revision: 3,
                ..Default::default()
            })))
            .unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(false))
        );
        runtime
            .stage(UiEvent::Desktop(Arc::new(Snapshot {
                revision: 2,
                tray: vec![bead::services::TrayItem {
                    id: "item".into(),
                    title: "Late stale callback".into(),
                    visible: true,
                    ..Default::default()
                }],
                ..Default::default()
            })))
            .unwrap();
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("tray.title"),
            Some(&Value::Text(String::new()))
        );
        drop(runtime);
        assert_eq!(desktop.subscription_count(), 0);
        desktop.stop();
    }
    #[test]
    fn actual_popover_bundle_preserves_disabled_rows_and_submenu_actions() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette = root.join("palette.json");
        let colors = bead::render::Palette::read(&palette).unwrap();
        let fallback = crate::sound::fallback(colors.get("surface").unwrap());
        let mut session =
            bead::ui::Session::new(root, "tray/main.luau".into(), palette, colors, fallback);
        let snapshot = Snapshot {
            tray: vec![bead::services::TrayItem {
                id: "item".into(),
                title: "Fixture tray".into(),
                visible: true,
                menu: vec![
                    MenuEntry {
                        id: 1,
                        label: "Enabled".into(),
                        visible: true,
                        enabled: true,
                        ..Default::default()
                    },
                    MenuEntry {
                        id: 2,
                        label: "Disabled".into(),
                        visible: true,
                        enabled: false,
                        ..Default::default()
                    },
                    MenuEntry {
                        id: 3,
                        label: "Submenu".into(),
                        visible: true,
                        enabled: true,
                        children: vec![MenuEntry::default()],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut menu = Menu::new(Arc::new(Mutex::new("item".into())));
        session.stores = session.stores.reconcile(&menu.values(&snapshot)).unwrap();
        session.set_viewport(300, 464);
        let scene = session.reload().unwrap();
        session.commit(scene);
        let actions: Vec<_> = session
            .view_nodes()
            .iter()
            .filter_map(|node| {
                node.action
                    .as_deref()
                    .map(|action| (action, node.action_arg.clone()))
            })
            .collect();
        assert!(actions.contains(&("services.menu", Some(Value::Text("item|1".into())))));
        assert!(!actions.contains(&("services.menu", Some(Value::Text("item|2".into())))));
        assert!(actions.contains(&("services.menu.open", Some(Value::Text("item|3".into())))));
    }
}
