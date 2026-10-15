// SPDX-License-Identifier: GPL-3.0-only
use std::path::Path;
pub fn check(file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = file.canonicalize()?;
    let root = file
        .ancestors()
        .skip(1)
        .find(|p| p.join("palette.json").is_file())
        .ok_or("UI bundle needs ancestor palette.json")?;
    let palette_path = root.join("palette.json");
    let fixture = initial_models()?;
    let mut harness = bead_test::Harness::load(
        root,
        file.strip_prefix(root)?,
        &palette_path,
        fixture,
        bead_test::Config {
            width: 1280,
            height: 900,
            scale: 1.,
        },
    )?;
    harness.render()?;
    println!("CHECK_OK {}", harness.session.resource_counts());
    Ok(())
}
fn initial_models()
-> Result<std::collections::BTreeMap<String, bead::core::Value>, Box<dyn std::error::Error>> {
    let mut session = bead::core::Stores::default();
    session = session.reconcile(&crate::notifications::values(&serde_json::Value::Null))?;
    session = session.reconcile(&crate::desktop::values(&Default::default(), ""))?;
    session = session.reconcile(&std::collections::BTreeMap::from([
        ("launcher.catalog".into(), bead::core::Value::List(vec![])),
        ("bead.workspaces".into(), bead::core::Value::List(vec![])),
        (
            "bead.focused_title".into(),
            bead::core::Value::Text(String::new()),
        ),
    ]))?;
    Ok(session.values().clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    use bead::core::Value;
    use std::path::PathBuf;
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell")
    }
    #[test]
    fn shell_views_use_headless_harness() -> Result<(), Box<dyn std::error::Error>> {
        for entry in [
            "ui/main.luau",
            "bar/main.luau",
            "notifications/main.luau",
            "osd/main.luau",
        ] {
            check(&root().join(entry))?;
        }
        Ok(())
    }
    #[test]
    fn notification_osd_populated_goldens_and_pose_counter()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = serde_json::json!({"rows":[{"id":"1","app":"Example","summary":"Foundation ready","body":"Private fixture","time_str":"now","expanded":true,"urgency":1,"actions":[{"id":"1|open","label":"Open"}]}],"popup_count":1,"history_count":1,"history_ready":true,"dnd":false,"inhibited":false,"error":"","timers":[],"osd":{"channel":"volume","value":0.75,"muted":false}});
        for (name, entry, config) in [
            (
                "notification",
                "notifications/main.luau",
                bead_test::Config {
                    width: 464,
                    height: 400,
                    scale: 1.,
                },
            ),
            (
                "osd",
                "osd/main.luau",
                bead_test::Config {
                    width: 404,
                    height: 144,
                    scale: 1.,
                },
            ),
        ] {
            let mut fixture = initial_models()?;
            fixture.extend(crate::notifications::values(&state));
            fixture.insert("panel.open".into(), Value::Bool(true));
            let mut harness = bead_test::Harness::load_with_icons(
                &root(),
                Path::new(entry),
                &root().join("palette.json"),
                fixture,
                config,
                std::collections::BTreeMap::from([(
                    "dialog-information".into(),
                    root().join("tests/icon.svg"),
                )]),
            )?;
            let frame = harness.render()?;
            if name == "osd" {
                let scene = harness.scene();
                let mut label_right = None;
                let mut status_left = None;
                for primitive in &scene.primitives {
                    if let bead::render::Primitive::Text { rect, text, .. } = primitive {
                        assert!(
                            rect.x >= scene.bounds.x
                                && rect.x + rect.w <= scene.bounds.x + scene.bounds.w,
                            "OSD text must fit within its panel"
                        );
                        if text == "Volume" {
                            label_right = Some(rect.x + rect.w);
                        }
                        if text == "75%" {
                            status_left = Some(rect.x);
                        }
                    }
                }
                assert!(
                    label_right
                        .zip(status_left)
                        .is_some_and(|(right, left)| right <= left),
                    "OSD label and status must occupy separate space"
                );
            }
            assert!(
                frame
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[3] > 0)
                    .count()
                    > 4000,
                "populated golden must not be blank"
            );
            let golden = root().join("tests/goldens").join(format!("{name}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                std::fs::create_dir_all(golden.parent().ok_or("golden parent")?)?;
                frame.save(&golden)?;
            }
            let artifacts = std::env::var_os("BEAD_TEST_ARTIFACTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("bead-shell-goldens"))
                .join(name);
            frame.compare(&golden, bead_test::Tolerance::default(), &artifacts)?;
            let offset = harness.scene().offset;
            harness.pose(offset, 0.99)?;
            let before = harness.counters();
            for i in 0..30 {
                harness.pose((offset.0, offset.1 + i as f32 * 0.2), 0.5)?;
            }
            assert_eq!(harness.counters(), before);
        }
        Ok(())
    }
}
