// SPDX-License-Identifier: GPL-3.0-only
use bead_core::Value;
use bead_services::Snapshot;
use std::collections::BTreeMap;

pub fn values(s: &Snapshot, menu_item: &str) -> BTreeMap<String, Value> {
    let audio = match (s.audio.volume, s.audio.muted) {
        (_, Some(true)) => "volume_off",
        (Some(_), Some(false)) => "volume_up",
        _ => "volume_mute",
    };
    let mut menu = Vec::new();
    fn entries(item: &str, rows: &[bead_services::MenuEntry], menu: &mut Vec<serde_json::Value>) {
        for row in rows
            .iter()
            .filter(|row| row.visible && row.enabled && !row.separator)
        {
            if row.children.is_empty() {
                menu.push(serde_json::json!({"id":format!("{item}|{}",row.id),"label":row.label.replace('_',"")}));
            } else {
                entries(item, &row.children, menu);
            }
        }
    }
    if let Some(item) = s.tray.iter().find(|item| item.id == menu_item) {
        entries(&item.id, &item.menu, &mut menu);
    }
    BTreeMap::from([
        ("bead.desktop.titlevisible".into(),Value::Bool(menu.is_empty())),
        ("bead.desktop.clock".into(), Value::Text(s.clock.clone())),
        ("bead.desktop.audio".into(), Value::Text(audio.into())),
        ("bead.desktop.audio.available".into(), Value::Bool(s.audio.volume.is_some())),
        ("bead.desktop.battery".into(), Value::Text(if !s.battery.available { "battery_unknown" } else if s.battery.charging { "battery_charging_full" } else { "battery_full" }.into())),
        ("bead.desktop.battery.value".into(), Value::Text(s.battery.percentage.map_or("—".into(), |n| format!("{n:.0}")))),
        ("bead.desktop.network".into(), Value::Text(if s.network.connected { "wifi" } else { "wifi_off" }.into())),
        ("bead.desktop.bluetooth".into(), Value::Text(if s.bluetooth.powered { "bluetooth" } else { "bluetooth_disabled" }.into())),
        ("bead.desktop.tray".into(), Value::List(s.tray.iter().filter(|t| t.visible).map(|t|serde_json::json!({"id":t.id,"title":t.title,"icon":format!("tray:{}",t.id)})).collect())),
        ("bead.desktop.menu".into(), Value::List(menu)),
    ])
}

fn valid_pixmap(p: &(i32, i32, Vec<u8>)) -> bool {
    let (w, h, bytes) = p;
    *w > 0 && *h > 0 && *w <= 512 && *h <= 512 && bytes.len() == (*w as usize) * (*h as usize) * 4
}

fn named_icon(item: &bead_services::TrayItem) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if item.icon_name.is_empty() {
        return None;
    }
    let direct = PathBuf::from(&item.icon_name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    // Some SNI applications ship icons outside the installed desktop theme.
    if !item.icon_theme_path.is_empty() && !item.icon_name.contains('/') {
        let root = PathBuf::from(&item.icon_theme_path);
        let mut directories = vec![root.clone()];
        // IconThemePath may point to a flat app directory or a normal indexed theme.
        if let Ok(index) = std::fs::read_to_string(root.join("index.theme")) {
            for line in index.lines() {
                if let Some(list) = line
                    .strip_prefix("Directories=")
                    .or_else(|| line.strip_prefix("ScaledDirectories="))
                {
                    directories.extend(
                        list.split(',')
                            .filter(|s| !s.is_empty())
                            .map(|s| root.join(s.trim())),
                    );
                }
            }
        }
        for directory in directories {
            for suffix in ["", ".png", ".svg", ".xpm"] {
                let path = directory.join(format!("{}{suffix}", item.icon_name));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    freedesktop_icons::lookup(&item.icon_name)
        .with_size(24)
        .find()
        .or_else(|| {
            freedesktop_icons::lookup(&item.icon_name)
                .with_theme("Adwaita")
                .with_size(24)
                .find()
        })
}

pub fn icons(
    scene: &mut bead_render::Scene,
    snapshot: &Snapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    use image::ImageEncoder;
    for draw in &mut scene.primitives {
        let bead_render::Primitive::Image { path, .. } = draw else {
            continue;
        };
        let Some(id) = path.to_str().and_then(|p| p.strip_prefix("@icon:tray:")) else {
            continue;
        };
        let item = snapshot
            .tray
            .iter()
            .find(|item| item.id == id)
            .ok_or("tray icon disappeared")?;
        if let Some(named) = named_icon(item)
            && let Ok(bytes) = std::fs::read(&named)
            && bead_render::validate_asset(&named, &bytes).is_ok()
        {
            scene
                .asset_data
                .insert(named.clone(), std::sync::Arc::from(bytes));
            *path = named;
            continue;
        }
        let Some((w, h, bytes)) = item
            .icon_pixmap
            .iter()
            .filter(|p| valid_pixmap(p))
            .min_by_key(|(w, h, _)| {
                (
                    if *w >= 24 && *h >= 24 { 0 } else { 1 },
                    (*w - 24).abs() + (*h - 24).abs(),
                )
            })
        else {
            let fallback = freedesktop_icons::lookup("application-x-executable")
                .with_size(24)
                .find()
                .unwrap_or_else(|| "shell/sample.svg".into());
            scene.asset_data.insert(
                fallback.clone(),
                std::sync::Arc::from(std::fs::read(&fallback)?),
            );
            *path = fallback;
            continue;
        };
        // SNI serializes network-order ARGB; PNG uses RGBA.
        let rgba: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[1], p[2], p[3], p[0]])
            .collect();
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            &rgba,
            *w as u32,
            *h as u32,
            image::ExtendedColorType::Rgba8,
        )?;
        let key = std::path::PathBuf::from(format!("@tray-pixmap/{id}.png"));
        scene
            .asset_data
            .insert(key.clone(), std::sync::Arc::from(png));
        *path = key;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_bar_bundle_loads_native_actions_assets_and_full_width_layout() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette_path = root.join("palette.json");
        let fallback = bead_render::Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 1920.,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 1920., 50.),
        };
        let mut session = bead_ui::Session::new(
            root,
            "bar/main.luau".into(),
            palette_path.clone(),
            bead_render::Palette::read(&palette_path).unwrap(),
            fallback,
        );
        let mut state = values(&Snapshot::default(), "");
        state.insert(
            "bead.workspaces".into(),
            Value::List(vec![
                serde_json::json!({"id":"fixture-stable-id","name":"1","index":1,"active":true}),
            ]),
        );
        state.insert(
            "bead.focused_title".into(),
            Value::Text("Fixture title".into()),
        );
        session.stores = session.stores.reconcile(&state).unwrap();
        session.set_viewport(1920, 1200);
        let scene = session.reload().unwrap();
        assert_eq!(scene.width, 1920.);
        assert!(scene.primitives.iter().any(|draw|matches!(draw,bead_render::Primitive::Rounded{rect,..} if rect.x==0. && rect.y==0. && rect.w==1920. && rect.h==50.)));
        session.commit(scene);
        let workspace = session
            .view_nodes()
            .iter()
            .find(|node| node.action.as_deref() == Some("backend.workspace.switch"))
            .unwrap();
        assert_eq!(
            workspace.action_arg,
            Some(Value::Text("fixture-stable-id".into()))
        );
        assert!(
            session
                .view_nodes()
                .iter()
                .any(|node| node.action.as_deref() == Some("services.audio"))
        );
    }
    #[test]
    fn unavailable_audio_and_bad_tray_pixels_do_not_become_valid_ui_state() {
        let s = Snapshot {
            tray: vec![bead_services::TrayItem {
                id: "bad".into(),
                visible: true,
                icon_name: "application-x-executable".into(),
                icon_pixmap: vec![(512, 512, vec![0; 4])],
                ..Default::default()
            }],
            ..Default::default()
        };
        let state = values(&s, "stale-id");
        assert_eq!(state["bead.desktop.audio.available"], Value::Bool(false));
        assert_eq!(state["bead.desktop.battery.value"], Value::Text("—".into()));
        assert_eq!(state["bead.desktop.menu"], Value::List(vec![]));
        let Value::List(tray) = &state["bead.desktop.tray"] else {
            panic!()
        };
        assert_eq!(tray[0]["icon"], "tray:bad");
    }
    #[test]
    fn named_tray_icon_precedes_pixmap_and_missing_name_falls_back_to_argb() {
        let dir = std::env::temp_dir().join(format!("bead-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let named = dir.join("fixture.png");
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[20, 40, 60, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&named, &png).unwrap();
        let item = bead_services::TrayItem {
            id: "fixture".into(),
            icon_name: "fixture".into(),
            icon_theme_path: dir.to_string_lossy().into(),
            icon_pixmap: vec![(1, 1, vec![128, 20, 40, 60])],
            ..Default::default()
        };
        let mut scene = bead_render::Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 24.,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 24., 24.),
        };
        scene.primitives.push(bead_render::Primitive::Image {
            path: "@icon:tray:fixture".into(),
            rect: bead_render::Rect::new(0., 0., 24., 24.),
        });
        let snapshot = Snapshot {
            tray: vec![item.clone()],
            ..Default::default()
        };
        icons(&mut scene, &snapshot).unwrap();
        assert!(scene.asset_data.contains_key(&named));
        let nested = dir.join("24x24/status");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            dir.join("index.theme"),
            "[Icon Theme]\nName=Fixture\nDirectories=24x24/status\n",
        )
        .unwrap();
        std::fs::rename(&named, nested.join("fixture.png")).unwrap();
        assert_eq!(named_icon(&item), Some(nested.join("fixture.png")));
        // A found but undecodable named icon also falls back to the real pixmap.
        std::fs::write(nested.join("fixture.png"), b"broken PNG").unwrap();
        scene.primitives[0] = bead_render::Primitive::Image {
            path: "@icon:tray:fixture".into(),
            rect: bead_render::Rect::new(0., 0., 24., 24.),
        };
        icons(
            &mut scene,
            &Snapshot {
                tray: vec![item],
                ..Default::default()
            },
        )
        .unwrap();
        let png = &scene.asset_data[&std::path::PathBuf::from("@tray-pixmap/fixture.png")];
        let decoded = image::load_from_memory(png).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [20, 40, 60, 128]);
        let mut missing = snapshot.tray[0].clone();
        missing.icon_name = "bead-nonexistent-icon-7832".into();
        scene.primitives[0] = bead_render::Primitive::Image {
            path: "@icon:tray:fixture".into(),
            rect: bead_render::Rect::new(0., 0., 24., 24.),
        };
        icons(
            &mut scene,
            &Snapshot {
                tray: vec![missing],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            matches!(&scene.primitives[0],bead_render::Primitive::Image{path,..} if path.to_string_lossy().starts_with("@tray-pixmap/"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tray_menu_excludes_disabled_hidden_and_separator_entries() {
        let s = Snapshot {
            tray: vec![bead_services::TrayItem {
                id: "item".into(),
                menu: vec![
                    bead_services::MenuEntry {
                        id: 1,
                        label: "_Open".into(),
                        enabled: true,
                        visible: true,
                        ..Default::default()
                    },
                    bead_services::MenuEntry {
                        id: 2,
                        label: "Hidden".into(),
                        enabled: true,
                        ..Default::default()
                    },
                    bead_services::MenuEntry {
                        id: 3,
                        label: "Disabled".into(),
                        visible: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let state = values(&s, "item");
        let Value::List(menu) = &state["bead.desktop.menu"] else {
            panic!()
        };
        assert_eq!(
            menu,
            &vec![serde_json::json!({"id":"item|1","label":"Open"})]
        );
    }
}
