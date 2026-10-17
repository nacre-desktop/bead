// SPDX-License-Identifier: GPL-3.0-only
//! Existing hosted types, deterministic fonts, no hardware or compositor.
use crate::status_views::{Kind, Status};
use bead::core::{Value, source::Source};
use bead_test::{Config, Harness, Result, source::FakeSource};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn data() -> serde_json::Value {
    let snapshot = bead::services::Snapshot {
        network: bead::services::Network {
            available: true,
            connected: true,
            wireless_enabled: true,
            name: "Fixture network".into(),
            strength: Some(86),
        },
        bluetooth: bead::services::Bluetooth {
            available: true,
            powered: true,
            connected_devices: vec!["Fixture headphones".into(), "Fixture keyboard".into()],
        },
        battery: bead::services::Battery {
            available: true,
            percentage: Some(83.),
            charging: true,
        },
        ..Default::default()
    };
    serde_json::to_value(snapshot).unwrap()
}
fn harness(scale: f32) -> Result<Harness> {
    Harness::load(
        &root(),
        Path::new("status/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 380,
            height: 300,
            scale,
        },
    )
}
#[test]
fn read_only_status_popouts_have_populated_goldens_at_both_scales() -> Result<()> {
    for kind in Kind::ALL {
        for scale in [1., 1.5] {
            let source = Arc::new(FakeSource::new(data())?);
            let view = Status::new(kind, source.clone());
            let mut h = harness(scale)?;
            h.session
                .set_projector(Box::new(move |_| Ok(view.project())));
            h.update(BTreeMap::new())?;
            assert_eq!(
                h.session.stores.get("status.ready"),
                Some(&Value::Bool(true))
            );
            assert!(
                h.session.view_nodes().is_empty(),
                "status-only views cannot execute service controls"
            );
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("status-{}-{scale}.png", kind.name()));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-status-{}-{scale}-diff", kind.name())),
            )?;
            assert_eq!(source.calls(), 0);
        }
    }
    Ok(())
}
#[test]
fn status_source_wakes_are_noop_when_unchanged_keep_last_data_and_release_on_drop() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::{sync::mpsc, time::Duration};
    for kind in Kind::ALL {
        let source = Arc::new(FakeSource::new(data())?);
        let weak = Arc::downgrade(&source);
        let h = harness(1.)?;
        let mut runtime =
            crate::reload::Runtime::new(h.session).with_status(kind, source.clone())?;
        let (send, receive) = mpsc::channel();
        runtime.services(Box::new(move |event| {
            let _ = send.send(event);
        }));
        let scene = runtime
            .stage(receive.recv_timeout(Duration::from_millis(200))?)?
            .ok_or("Source wake did not project status")?;
        runtime.session.commit(scene);
        assert!(
            runtime.stage(UiEvent::Model(BTreeMap::new()))?.is_none(),
            "unchanged Source must not layout or raster"
        );
        let primary = runtime.session.stores.get("status.primary").cloned();
        let previous_data = Arc::downgrade(&source.snapshot().data);
        source.disconnect()?;
        assert!(
            runtime
                .stage(receive.recv_timeout(Duration::from_millis(200))?)?
                .is_none(),
            "disconnect keeps the last snapshot"
        );
        assert_eq!(
            runtime.session.stores.get("status.primary"),
            primary.as_ref()
        );
        source.publish(serde_json::json!({}))?;
        let scene = runtime
            .stage(receive.recv_timeout(Duration::from_millis(200))?)?
            .ok_or("missing data must invalidate availability")?;
        runtime.session.commit(scene);
        assert_eq!(
            runtime.session.stores.get("status.ready"),
            Some(&Value::Bool(false))
        );
        assert!(
            previous_data.upgrade().is_none(),
            "view caches must not retain replaced Source payloads"
        );
        let current_data = Arc::downgrade(&source.snapshot().data);
        assert_eq!(source.calls(), 0);
        drop(source);
        assert!(weak.upgrade().is_some());
        drop(runtime);
        assert!(
            weak.upgrade().is_none(),
            "closed worker must release its Source and subscription"
        );
        assert!(
            current_data.upgrade().is_none(),
            "closed worker must release the last payload too"
        );
    }
    Ok(())
}
#[test]
fn coarse_status_rejects_invalid_numbers_and_bounds_external_labels() -> Result<()> {
    let mut snapshot = data();
    snapshot["battery"]["percentage"] = serde_json::json!(101);
    snapshot["bluetooth"]["connected_devices"] = serde_json::json!([null]);
    snapshot["network"]["wireless_enabled"] = serde_json::Value::Null;
    for kind in Kind::ALL {
        let source = Arc::new(FakeSource::new(snapshot.clone())?);
        assert_eq!(
            Status::new(kind, source).project()["status.ready"],
            Value::Bool(false)
        );
    }
    let mut off = data();
    off["network"]["wireless_enabled"] = serde_json::json!(false);
    off["network"]["connected"] = serde_json::json!(false);
    off["bluetooth"]["powered"] = serde_json::json!(false);
    for kind in [Kind::Network, Kind::Bluetooth] {
        let source = Arc::new(FakeSource::new(off.clone())?);
        let values = Status::new(kind, source).project();
        assert_eq!(values["status.ready"], Value::Bool(true));
        assert_eq!(values["status.active"], Value::Bool(false));
    }
    let mut snapshot = data();
    snapshot["network"]["name"] = serde_json::json!("界\n".repeat(200));
    let source = Arc::new(FakeSource::new(snapshot)?);
    let projected = Status::new(Kind::Network, source).project();
    let Value::Text(label) = &projected["status.primary"] else {
        panic!("text label")
    };
    assert!(label.chars().count() <= 33 && !label.contains('\n') && label.ends_with('…'));
    Ok(())
}

#[test]
fn status_registry_keeps_only_cold_metadata_and_never_starts_another_service() -> Result<()> {
    let palette = Arc::new(root().join("palette.json"));
    let colors = Arc::new(bead::render::Palette::read(&palette)?);
    let scale: serde_json::Value =
        serde_json::from_str(include_str!("../../bead-ui/assets/size-scale.json"))?;
    let radius = scale["radii"]["medium"]
        .as_f64()
        .ok_or("shared radius missing")? as f32;
    let owner = Arc::new(std::sync::OnceLock::new());
    let specs = Kind::ALL
        .into_iter()
        .map(|kind| {
            crate::popouts::status(
                kind,
                palette.clone(),
                colors.clone(),
                owner.clone(),
                radius,
                false,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert!(owner.get().is_none());
    for spec in specs {
        assert!(!spec.initial_open && !spec.passive);
        assert!(matches!(spec.renderer, bead::render::RendererKind::Cpu));
        assert!(spec.fallback.asset_data.is_empty() && spec.fallback.font_files.is_empty());
        assert!(
            (spec.factory)(true).is_err(),
            "an absent shared owner must not start a second backend"
        );
        assert!(owner.get().is_none());
    }
    Ok(())
}

#[test]
fn each_status_source_selects_its_existing_service_before_copying_data() -> Result<()> {
    let mut snapshot: bead::services::Snapshot = serde_json::from_value(data())?;
    snapshot.tray = vec![bead::services::TrayItem {
        icon_pixmap: vec![(1024, 1024, vec![255; 4 * 1024 * 1024])],
        ..Default::default()
    }];
    for kind in Kind::ALL {
        let selected = kind.projection()(&snapshot);
        let object = selected.as_object().ok_or("object projection")?;
        assert_eq!(object.len(), 1);
        assert_eq!(selected[kind.name()], data()[kind.name()]);
        let full = Status::new(kind, Arc::new(FakeSource::new(data())?)).project();
        let subset = Status::new(kind, Arc::new(FakeSource::new(selected.clone())?)).project();
        assert_eq!(
            full, subset,
            "view bindings must not depend on unrelated services"
        );
        assert!(selected.get("tray").is_none() && selected.get("clock").is_none());
        assert!(
            serde_json::to_vec(&selected)?.len() < 1024,
            "bulk tray data must never be copied into the view Source"
        );
    }
    Ok(())
}
