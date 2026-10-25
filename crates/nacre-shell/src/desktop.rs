// SPDX-License-Identifier: GPL-3.0-only
use bead::core::Value;
use bead::services::Snapshot;
use std::collections::BTreeMap;

pub fn values(s: &Snapshot, menu_item: &str) -> BTreeMap<String, Value> {
    let audio = match (s.audio.volume, s.audio.muted) {
        (_, Some(true)) => "volume_off",
        (Some(_), Some(false)) => "volume_up",
        _ => "volume_mute",
    };
    let mut menu = Vec::new();
    fn entries(item: &str, rows: &[bead::services::MenuEntry], menu: &mut Vec<serde_json::Value>) {
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
        ("bead.desktop.audio.percent".into(),Value::Number(s.audio.volume.unwrap_or(0.)*100.)),
        ("bead.desktop.audio.device".into(),Value::Text(s.audio.default_sink.clone().unwrap_or_else(||"Output unavailable".into()))),
        ("bead.desktop.audio.label".into(),Value::Text(s.audio.volume.map(|v|format!("{:.0}%",v*100.)).unwrap_or_else(||"Unavailable".into()))),
        ("bead.desktop.titlevisible".into(),Value::Bool(menu.is_empty())),
        ("bead.desktop.clock".into(), Value::Text(s.clock.clone())),
        ("bead.desktop.audio".into(), Value::Text(audio.into())),
        ("bead.desktop.audio.available".into(), Value::Bool(s.audio.volume.is_some())),
        ("bead.desktop.battery".into(), Value::Text(if !s.battery.available { "battery_unknown" } else if s.battery.charging { "battery_charging_full" } else { "battery_full" }.into())),
        ("bead.desktop.battery.value".into(), Value::Text(s.battery.percentage.map_or(String::new(), |n| format!("{n:.0}")))),
        // QML detail text sits 4 px after the glyph; the cell's 3 px padding closes it (none without a value).
        ("bead.desktop.battery.value.width".into(), Value::Number(s.battery.percentage.map_or(3., |n| 7. + 7.5 * format!("{n:.0}").len() as f32 as f64))),
        ("bead.desktop.network".into(), Value::Text(if s.network.connected { "wifi" } else { "wifi_off" }.into())),
        ("bead.desktop.bluetooth".into(), Value::Text(if s.bluetooth.powered { "bluetooth" } else { "bluetooth_disabled" }.into())),
        ("bead.desktop.tray".into(), Value::list(s.tray.iter().filter(|t| t.visible).map(|t|serde_json::json!({"id":t.id,"title":t.title,"icon":format!("tray:{}",t.id),"attention":t.attention})).collect())),
        ("bead.desktop.menu".into(), Value::list(menu)),
        ("bead.desktop.tray.tooltip".into(),Value::Text(String::new())),
        ("bead.desktop.tray.tooltip.visible".into(),Value::Bool(false)),
    ])
}

pub fn tooltip_values(snapshot: &Snapshot, id: &str) -> BTreeMap<String, Value> {
    let text = snapshot
        .tray
        .iter()
        .find(|item| item.id == id && item.visible)
        .map(|item| {
            let title = if item.tooltip_title.is_empty() {
                &item.title
            } else {
                &item.tooltip_title
            };
            if item.tooltip_description.is_empty() {
                title.clone()
            } else {
                format!("{title} — {}", item.tooltip_description)
            }
        })
        .unwrap_or_default();
    BTreeMap::from([
        (
            "bead.desktop.tray.tooltip.visible".into(),
            Value::Bool(!text.is_empty()),
        ),
        (
            "bead.desktop.titlevisible".into(),
            Value::Bool(text.is_empty()),
        ),
        ("bead.desktop.tray.tooltip".into(), Value::Text(text)),
    ])
}
fn read_icon_file(path: &std::path::Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("icon is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::other("icon exceeds byte limit"));
    }
    Ok(bytes)
}

fn valid_pixmap(p: &(i32, i32, Vec<u8>)) -> bool {
    let (w, h, bytes) = p;
    *w > 0 && *h > 0 && *w <= 512 && *h <= 512 && bytes.len() == (*w as usize) * (*h as usize) * 4
}

fn named_icon(item: &bead::services::TrayItem) -> Option<std::path::PathBuf> {
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
        if let Ok(index) = read_icon_file(&root.join("index.theme"), 64 * 1024)
            .and_then(|bytes| String::from_utf8(bytes).map_err(std::io::Error::other))
        {
            for line in index.lines() {
                if let Some(list) = line
                    .strip_prefix("Directories=")
                    .or_else(|| line.strip_prefix("ScaledDirectories="))
                {
                    directories.extend(
                        list.split(',')
                            .filter(|s| !s.is_empty() && !s.split('/').any(|part| part == ".."))
                            .take(128)
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
    scene: &mut bead::render::Scene,
    snapshot: &Snapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    use image::ImageEncoder;
    for draw in &mut scene.primitives {
        let bead::render::Primitive::Image { path, .. } = draw else {
            continue;
        };
        let Some(id) = path.to_str().and_then(|p| p.strip_prefix("@icon:tray:")) else {
            continue;
        };
        let missing = bead::services::TrayItem::default();
        let item = snapshot
            .tray
            .iter()
            .find(|item| item.id == id)
            .unwrap_or(&missing);
        if let Some(named) = named_icon(item)
            && let Ok(bytes) = read_icon_file(&named, 4 * 1024 * 1024)
            && bead::render::validate_asset(&named, &bytes).is_ok()
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
                std::sync::Arc::from(read_icon_file(&fallback, 4 * 1024 * 1024)?),
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
    /// The real bar bundle at 1920 px with these workspace rows, reloaded but not yet committed.
    fn bar_session(rows: Vec<serde_json::Value>) -> (bead::ui::Session, bead::render::Scene) {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette_path = root.join("palette.json");
        let fallback = bead::render::Scene {
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
            bounds: bead::render::Rect::new(0., 0., 1920., 50.),
        };
        let mut session = bead::ui::Session::new(
            root,
            "bar/main.luau".into(),
            palette_path.clone(),
            bead::render::Palette::read(&palette_path).unwrap(),
            fallback,
        );
        let mut state = values(&Snapshot::default(), "");
        state.insert("bead.workspaces".into(), Value::list(rows));
        state.insert(
            "bead.focused_title".into(),
            Value::Text("Fixture title".into()),
        );
        session.stores = session.stores.reconcile(&state).unwrap();
        session.set_viewport(1920, 1200);
        let scene = session.reload().unwrap();
        (session, scene)
    }
    #[test]
    fn actual_bar_bundle_loads_native_actions_assets_and_full_width_layout() {
        let rows =
            vec![serde_json::json!({"id":"fixture-stable-id","name":"1","index":1,"active":true})];
        let (mut session, scene) = bar_session(rows);
        assert_eq!(scene.width, 1920.);
        assert!(scene.primitives.iter().any(|draw|matches!(draw,bead::render::Primitive::Rounded{rect,..} if rect.x==0. && rect.y==0. && rect.w==1920. && rect.h==50.)));
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
    /// Widths of the workspace cells and every text drawn, for a set of workspace rows.
    fn capsule(rows: Vec<serde_json::Value>) -> (Vec<f32>, Vec<String>) {
        let (mut session, scene) = bar_session(rows);
        let texts = scene
            .primitives
            .iter()
            .filter_map(|draw| match draw {
                bead::render::Primitive::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        session.commit(scene);
        let widths = session
            .view_nodes()
            .iter()
            .filter(|n| n.action.as_deref() == Some("backend.workspace.switch"))
            .filter_map(|n| match &n.draw {
                bead::render::Primitive::Rounded { rect, .. } => Some(rect.w),
                _ => None,
            })
            .collect();
        (widths, texts)
    }
    /// B0101: the blended pearl colours are tint expressions under `ui.alpha`/`ui.tint` inside `ui.Repeat`; the bar must keep
    /// rendering while workspaces are added, removed, occupied and activated live.
    #[test]
    fn pearl_blends_survive_live_workspace_model_updates() {
        let rows = |count: u32, active: u32, occupied: &[u32]| -> Vec<serde_json::Value> {
            (1..=count)
                .map(|i| {
                    serde_json::json!({"id":format!("w{i}"),"name":i.to_string(),"label":i.to_string(),
                        "index":i,"active":i==active,"icon":"","icon_mode":false,"occupied":occupied.contains(&i),
                        "shown":0,"overflow":"","hinted":false,"app1":"","app2":"","app3":""})
                })
                .collect()
        };
        let (mut session, scene) = bar_session(rows(6, 2, &[1, 2]));
        session.commit(scene);
        for (count, active, occupied) in [
            (6, 3, vec![1, 3, 5]),
            (7, 7, vec![7]),
            (3, 1, vec![]),
            (1, 1, vec![1]),
            (9, 4, vec![2, 4, 9]),
            (6, 6, vec![1, 2, 3, 4, 5, 6]),
        ] {
            let update = BTreeMap::from([(
                "bead.workspaces".to_owned(),
                Value::list(rows(count, active, &occupied)),
            )]);
            if let Some(scene) = session
                .update(update)
                .expect("bar renders after a workspace change")
            {
                session.commit(scene);
            }
        }
    }
    #[test]
    fn workspace_capsule_draws_icon_mode_cells_and_window_hints_from_row_fields() {
        let row = |i: u32, extra: serde_json::Value| {
            let mut row = serde_json::json!({"id":format!("w{i}"),"name":i.to_string(),"label":i.to_string(),
                "index":i,"active":i==1,"icon":"","icon_mode":false,"occupied":false,"shown":0,"overflow":"",
                "hinted":false,"app1":"","app2":"","app3":""});
            row.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            row
        };
        // Default: 32 px cells, no hint glyphs.
        let (plain, texts) = capsule(vec![
            row(1, serde_json::json!({})),
            row(2, serde_json::json!({})),
        ]);
        assert_eq!(plain, [32., 32.]);
        assert!(!texts.iter().any(|t| t == "terminal"));
        // workspaceWindowHints: the active cell grows by 18 per glyph, 20 for "+N" and 4 padding.
        let hinted = serde_json::json!({"hinted":true,"shown":2,"overflow":"+1","app1":"terminal","app2":"language"});
        let (wide, texts) = capsule(vec![row(1, hinted), row(2, serde_json::json!({}))]);
        assert_eq!(wide, [32. + 36. + 20. + 4., 32.]);
        for glyph in ["terminal", "language", "+1"] {
            assert!(texts.iter().any(|t| t == glyph), "{glyph} drawn");
        }
        // workspaceIcons: every cell is 46 px and shows its symbol and number.
        let icons = serde_json::json!({"icon_mode":true,"occupied":true});
        let (cells, texts) = capsule(vec![
            row(
                1,
                serde_json::json!({"icon":"home","icon_mode":true,"occupied":true}),
            ),
            row(2, icons),
        ]);
        assert_eq!(cells, [46., 46.]);
        assert!(texts.iter().any(|t| t == "home") && texts.iter().any(|t| t == "2"));
        // N0040: a visiting group starts with its home output's name and keeps the cell width.
        let visitor =
            serde_json::json!({"visiting":true,"group_start":true,"origin":"DP-2","label":"3"});
        let (cells, texts) = capsule(vec![row(1, serde_json::json!({})), row(2, visitor)]);
        assert_eq!(cells, [32., 32.]);
        assert!(texts.iter().any(|t| t == "▣ DP-2") && texts.iter().any(|t| t == "3"));
    }
    #[test]
    fn unavailable_audio_and_bad_tray_pixels_do_not_become_valid_ui_state() {
        let s = Snapshot {
            tray: vec![bead::services::TrayItem {
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
        assert_eq!(
            state["bead.desktop.battery.value"],
            Value::Text(String::new())
        );
        assert_eq!(state["bead.desktop.menu"], Value::list(vec![]));
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
        let item = bead::services::TrayItem {
            id: "fixture".into(),
            icon_name: "fixture".into(),
            icon_theme_path: dir.to_string_lossy().into(),
            icon_pixmap: vec![(1, 1, vec![128, 20, 40, 60])],
            ..Default::default()
        };
        let mut scene = bead::render::Scene {
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
            bounds: bead::render::Rect::new(0., 0., 24., 24.),
        };
        scene.primitives.push(bead::render::Primitive::Image {
            blur: 0.,
            path: "@icon:tray:fixture".into(),
            rect: bead::render::Rect::new(0., 0., 24., 24.),
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
        scene.primitives[0] = bead::render::Primitive::Image {
            blur: 0.,
            path: "@icon:tray:fixture".into(),
            rect: bead::render::Rect::new(0., 0., 24., 24.),
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
        scene.primitives[0] = bead::render::Primitive::Image {
            blur: 0.,
            path: "@icon:tray:fixture".into(),
            rect: bead::render::Rect::new(0., 0., 24., 24.),
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
            matches!(&scene.primitives[0],bead::render::Primitive::Image {path,..} if path.to_string_lossy().starts_with("@tray-pixmap/"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tray_menu_excludes_disabled_hidden_and_separator_entries() {
        let s = Snapshot {
            tray: vec![bead::services::TrayItem {
                id: "item".into(),
                menu: vec![
                    bead::services::MenuEntry {
                        id: 1,
                        label: "_Open".into(),
                        enabled: true,
                        visible: true,
                        ..Default::default()
                    },
                    bead::services::MenuEntry {
                        id: 2,
                        label: "Hidden".into(),
                        enabled: true,
                        ..Default::default()
                    },
                    bead::services::MenuEntry {
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
            menu.as_ref(),
            &vec![serde_json::json!({"id":"item|1","label":"Open"})]
        );
    }
}
