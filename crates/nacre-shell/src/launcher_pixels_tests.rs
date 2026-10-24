// SPDX-License-Identifier: GPL-3.0-only
//! Real launcher widgets against accepted-source goldens, not a DSL imitation.
use bead::core::Value;
use bead_test::{Config, Harness, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn harness(query: &str, scroll: f64) -> Result<Harness> {
    crate::i18n::init();
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let selected = std::env::var_os("BEAD_ROW_REFERENCE_BUNDLE");
    if selected.is_some() && !update {
        return Err("reference bundle requires explicit golden capture".into());
    }
    let root = selected.map(PathBuf::from).unwrap_or_else(root);
    let rows=(0..320).map(|i|serde_json::json!({"id":format!("app-{i}"),"name":format!("Example application {i}"),"description":format!("Description {i}"),"icon":"fixture","selected":i==0,"disabled":i%3==2,"favorite":i%7==0,"favoriteId":format!("app-{i}"),"emoji":if i%4==0 {"A"}else{""}})).collect();
    Harness::load_with_icons(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("launcher.apps".into(), Value::list(rows)),
            ("launcher.groups".into(), crate::launcher::fixture_groups()),
            ("launcher.result_count".into(), Value::Number(320.)),
            ("launcher.search.text".into(), Value::Text(query.into())),
            ("launcher.scroll.y".into(), Value::Number(scroll)),
            (
                "launcher.quick".into(),
                Value::list(vec![
                    serde_json::json!({"id":"actions.controls.dnd","label":"Do not disturb","reason":"Silence notification popups","status":"Off","icon":"notifications","on":false,"enabled":true}),
                    serde_json::json!({"id":"actions.controls.night-light","label":"Night light","reason":"Make the screen warmer","status":"On","icon":"nightlight","on":true,"enabled":true}),
                    serde_json::json!({"id":"actions.controls.stay-awake","label":"Stay awake","reason":"Prevent automatic sleep","status":"Off","icon":"coffee","on":false,"enabled":true}),
                ]),
            ),
            ("launcher.updates.count".into(), Value::Number(7.)),
        ]),
        Config {
            width: 1920,
            height: 1200,
            scale: 1.5,
        },
        BTreeMap::from([("fixture".into(), root.join("tests/icon.svg"))]),
    )
}

#[test]
fn quick_buttons_keep_their_control_ids_and_unknown_updates_stay_hidden() -> Result<()> {
    let mut h = harness("", 0.)?;
    let ids = h
        .session
        .view_nodes()
        .iter()
        .filter(|node| node.action.as_deref() == Some("launcher.quick"))
        .map(|node| node.action_arg.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "actions.controls.dnd",
            "actions.controls.night-light",
            "actions.controls.stay-awake"
        ]
        .map(|id| Value::Text(id.into()))
    );
    assert!(
        h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .all(|node| bead::ui::rect(&node.draw).h == 50.),
        "the QML default descriptions fit single-line, 50-pixel rows"
    );
    h.update(BTreeMap::from([(
        "launcher.updates.count".into(),
        Value::Number(-1.),
    )]))?;
    assert!(!h.session.view_nodes().iter().any(|node| {
        matches!(&node.draw, bead::render::Primitive::Text { text, .. } if text == "7 updates" || text == "Up to date")
    }));
    Ok(())
}

#[test]
fn real_launcher_grid_results_and_scrolled_pixels_match_accepted_markup() -> Result<()> {
    for (name, query, scroll) in [
        ("grid", "", 0.),
        ("results", "a", 0.),
        ("scrolled", "a", 500.),
    ] {
        let mut h = harness(query, scroll)?;
        assert_eq!(
            h.scene().motion.as_ref().map(|motion| motion.id.as_str()),
            Some("launcher")
        );
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|n| n.action.as_deref() == Some("launcher.quick"))
                .count(),
            3,
            "Quick remains beside ordinary search results: {query:?}; viewport={:?}",
            h.session.stores.get("bead.viewport.width")
        );
        let source = h.render()?;
        let width = (h.scene().bounds.w * 1.5).ceil() as u32;
        let height = (h.scene().bounds.h * 1.5).ceil() as u32;
        assert!(width <= source.width && height <= source.height);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in source
            .pixels
            .chunks_exact(source.width as usize * 4)
            .take(height as usize)
        {
            pixels.extend_from_slice(&row[..width as usize * 4]);
        }
        let frame = bead_test::Frame {
            width,
            height,
            pixels,
        };
        let golden = root()
            .join("tests/goldens")
            .join(format!("launcher-rows-{name}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance {
                channel: 0,
                changed_fraction: 0.,
            },
            &std::env::temp_dir().join("bead-launcher-rows"),
        )?;
        let actions = h
            .session
            .view_nodes()
            .iter()
            .filter(|n| n.action.as_deref() == Some("select_app"))
            .collect::<Vec<_>>();
        assert!(
            actions.len() >= 7,
            "populated fixture must exercise actual rows"
        );
        let ids = actions
            .iter()
            .map(|n| match &n.action_arg {
                Some(Value::Text(id)) => id.clone(),
                _ => panic!("row action lost its item ID"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actions.len(),
            ids.len(),
            "each visible row must retain its own action ID"
        );
        let favorites = h
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("toggle_favorite"))
            .collect::<Vec<_>>();
        let row_ids = actions
            .iter()
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let favorite_ids = favorites
            .iter()
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert!(
            favorite_ids.is_subset(&row_ids),
            "favorite actions must point at displayed rows"
        );
        let fully_visible_rows = actions
            .iter()
            .filter(|node| {
                let rect = bead::ui::rect(&node.draw);
                node.clip.is_none_or(|clip| {
                    rect.x >= clip.x
                        && rect.y >= clip.y
                        && rect.x + rect.w <= clip.x + clip.w
                        && rect.y + rect.h <= clip.y + clip.h
                })
            })
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert!(
            fully_visible_rows.is_subset(&favorite_ids),
            "every fully visible row needs its favorite action"
        );
    }
    Ok(())
}

#[test]
fn restored_launcher_route_reopens_with_matching_mode_and_results_layout() -> Result<()> {
    for route in ["search", "actions"] {
        let mut h = harness("", 0.)?;
        // Only page/route strings are durable; mode and query are transient.
        h.update(BTreeMap::from([(
            "launcher.route".into(),
            Value::Text(route.into()),
        )]))?;
        assert_eq!(
            h.session.stores.get("launcher.mode"),
            Some(&Value::Text("apps".into()))
        );
        let mut reply = crate::launcher::fixture_reply(
            "",
            vec![serde_json::json!({"id":"restored","name":"Restored result","icon":"fixture"})],
        );
        reply["mode"] = serde_json::json!(route);
        reply["route"] = serde_json::json!(route);
        let source = std::sync::Arc::new(crate::launcher::Fixture::new(
            serde_json::json!({"queries":{"":reply}}),
        )?);
        let menu = crate::launcher::Menu::new(Some(source))?;
        h.session
            .set_projector(Box::new(move |stores| menu.project(stores)));
        h.update(BTreeMap::new())?;
        assert_eq!(
            h.session.stores.get("launcher.mode"),
            Some(&Value::Text(route.into()))
        );
        assert_eq!(
            h.session.stores.get("launcher.search.text"),
            Some(&Value::Text(String::new()))
        );
        assert!(
            h.scene()
                .layers
                .iter()
                .any(|layer| layer.id.contains("results"))
        );
        assert!(
            !h.scene()
                .layers
                .iter()
                .any(|layer| layer.id.contains("app-grid"))
        );
    }
    Ok(())
}

#[test]
fn ranked_prefix_keeps_actual_visible_pixels_and_actions_exact() -> Result<()> {
    for scroll in [0., 500.] {
        let mut h = harness("a", scroll)?;
        let full = h.render()?;
        let action_ids = |h: &Harness| {
            h.session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("select_app"))
                .map(|node| format!("{:?}", node.action_arg))
                .collect::<Vec<_>>()
        };
        let expected = action_ids(&h);
        let Some(Value::List(rows)) = h.session.stores.get("launcher.apps") else {
            panic!("fixture rows")
        };
        let prefix = rows[..64].to_vec();
        h.update(BTreeMap::from([
            ("launcher.apps".into(), Value::list(prefix)),
            ("launcher.result_count".into(), Value::Number(64.)),
            ("launcher.total_count".into(), Value::Number(320.)),
        ]))?;
        assert_eq!(h.render()?.pixels, full.pixels);
        assert_eq!(action_ids(&h), expected);
    }
    Ok(())
}

/// B0037: `launcher-layout` order and hidden ids drive the category rail live; Home is
/// never hidden and rows outside the order keep their place at the end.
#[test]
fn launcher_rail_follows_layout_order_and_hidden_ids() -> Result<()> {
    let rail = |order: &[&str], hidden: &[&str]| -> Result<Vec<String>> {
        let list = |ids: &[&str]| {
            Value::list(ids.iter().map(|i| serde_json::json!({ "id": i })).collect())
        };
        let root = root();
        let mut h = Harness::load(
            &root,
            Path::new("ui/main.luau"),
            &root.join("palette.json"),
            BTreeMap::from([
                ("launcher.groups".into(), crate::launcher::fixture_groups()),
                ("launcher.rail.order".into(), list(order)),
                ("launcher.rail.hidden".into(), list(hidden)),
            ]),
            Config {
                width: 1920,
                height: 1200,
                scale: 1.,
            },
        )?;
        h.render()?;
        Ok(h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("navigate"))
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect())
    };
    let all = [
        "home", "apps", "actions", "style", "setup", "software", "health", "learn", "power",
    ];
    assert_eq!(rail(&[], &[])?, all);
    assert_eq!(
        rail(&[], &["actions", "home", "power"])?,
        [
            "home", "apps", "style", "setup", "software", "health", "learn"
        ]
    );
    assert_eq!(
        rail(&["power", "style"], &[])?,
        [
            "home", "power", "style", "apps", "actions", "setup", "software", "health", "learn"
        ]
    );
    assert_eq!(
        rail(&["apps", "style"], &["actions"])?,
        [
            "home", "apps", "style", "setup", "software", "health", "learn", "power"
        ]
    );
    // Without `menu.groups` (older daemon) the rail keeps the groups every engine has.
    let bare = {
        let root = root();
        let mut h = Harness::load(
            &root,
            Path::new("ui/main.luau"),
            &root.join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 1920,
                height: 1200,
                scale: 1.,
            },
        )?;
        h.render()?;
        h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("navigate"))
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(bare, ["home", "apps", "actions"]);
    Ok(())
}

#[test]
fn pending_query_keeps_the_accepted_model_geometry_until_replacement() -> Result<()> {
    let mut h = harness("", 0.)?;
    let grid = |h: &Harness| {
        h.session
            .view_nodes()
            .iter()
            .any(|n| n.id.ends_with("app-grid"))
    };
    assert!(grid(&h));
    h.update(BTreeMap::from([
        ("launcher.search.text".into(), Value::Text("a".into())),
        ("launcher.pending".into(), Value::Bool(true)),
    ]))?;
    assert!(
        grid(&h),
        "old grid rows must not be reshaped as pending search rows"
    );
    assert_eq!(
        h.session.stores.get("launcher.search.text"),
        Some(&Value::Text("a".into()))
    );
    h.update(BTreeMap::from([
        ("launcher.result_query".into(), Value::Text("a".into())),
        ("launcher.pending".into(), Value::Bool(false)),
    ]))?;
    assert!(!grid(&h), "accepted search results must use row geometry");
    Ok(())
}
