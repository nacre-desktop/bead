// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead_test::source::FakeSource;
use serde_json::Value as Json;

fn source(data: Json) -> Arc<dyn Source> {
    Arc::new(FakeSource::new(data).unwrap())
}
fn make(controls: Option<Json>, notifications: Option<Json>, updates: Option<Json>) -> Quick {
    let mut topics = Vec::new();
    if let Some(data) = controls {
        topics.push(("controls", source(data)));
    }
    if let Some(data) = notifications {
        topics.push(("notifications", source(data)));
    }
    if let Some(data) = updates {
        topics.push(("dashboard.updates", source(data)));
    }
    Quick::new(None, topics)
}
fn rows(out: &BTreeMap<String, Value>) -> Vec<Json> {
    let Value::List(rows) = &out["launcher.quick"] else {
        panic!("quick list")
    };
    rows.as_ref().clone()
}
fn stores(pairs: &[(&str, Value)]) -> Stores {
    Stores::default()
        .reconcile(
            &pairs
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone()))
                .collect(),
        )
        .unwrap()
}
fn health(available: bool, stale: bool) -> Json {
    json!({"available": available, "stale": stale})
}

#[test]
fn projects_three_live_toggles_and_keeps_external_night_light_read_only() {
    let quick = make(
        Some(
            json!({"health": health(true, false), "night_light": {"enabled": true, "external": true}, "stay_awake": {"enabled": false}}),
        ),
        Some(json!({"health": health(true, false), "do_not_disturb": true})),
        None,
    );
    let rows = rows(&quick.project(&Stores::default()));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["id"], "actions.controls.dnd");
    assert_eq!(rows[0]["on"], true);
    assert_eq!(rows[0]["enabled"], false);
    assert_eq!(rows[1]["on"], true);
    assert_eq!(rows[1]["enabled"], false);
    assert!(!rows[1]["reason"].as_str().unwrap().is_empty());
    assert_eq!(rows[2]["on"], false);
    assert_eq!(
        rows[2]["enabled"], false,
        "no daemon client advertises stay-awake.set"
    );
}

#[test]
fn toggle_rows_follow_topic_updates_without_optimistic_changes() {
    let controls = Arc::new(FakeSource::new(json!({"health": health(true, false), "night_light": {"enabled": false}, "stay_awake": {"enabled": false}})).unwrap());
    let notifications = Arc::new(
        FakeSource::new(json!({"health": health(true, false), "do_not_disturb": false})).unwrap(),
    );
    let quick = Quick::new(
        None,
        vec![
            ("controls", controls.clone() as Arc<dyn Source>),
            ("notifications", notifications.clone() as Arc<dyn Source>),
        ],
    );
    assert_eq!(rows(&quick.project(&Stores::default()))[0]["on"], false);
    notifications
        .publish(json!({"health": health(true, false), "do_not_disturb": true}))
        .unwrap();
    assert_eq!(rows(&quick.project(&Stores::default()))[0]["on"], true);
}

#[test]
fn unavailable_or_stale_data_disables_rows_and_never_invents_an_update_count() {
    let quick = make(
        Some(json!({"health": health(true, true), "night_light": {"enabled": false}})),
        None,
        Some(json!({"health": health(false, true), "count": 0})),
    );
    let out = quick.project(&Stores::default());
    assert!(
        rows(&out)
            .iter()
            .all(|row| row["enabled"] == false && row["status"] == "—")
    );
    assert_eq!(out["launcher.updates.count"], Value::Number(-1.));
}

#[test]
fn only_a_valid_count_is_projected_and_closing_does_not_rewrite_the_last_frame() {
    let quick = make(
        Some(
            json!({"health": health(true, false), "night_light": {"enabled": false}, "stay_awake": {"enabled": true}}),
        ),
        Some(json!({"health": health(true, false), "do_not_disturb": false})),
        Some(json!({"health": health(true, false), "count": 4})),
    );
    let out = quick.project(&Stores::default());
    assert_eq!(out["launcher.updates.count"], Value::Number(4.));
    let closed = quick.project(&stores(&[("panel.open", Value::Bool(false))]));
    assert!(
        closed.is_empty(),
        "runtime retirement releases the last visible rows"
    );
}

#[test]
fn zero_is_a_known_updates_count_while_missing_or_invalid_count_is_unknown() {
    let controls = Some(json!({"health": health(true, false), "night_light": {"enabled": false}}));
    let notifications = Some(json!({"health": health(true, false), "do_not_disturb": false}));
    let zero = make(
        controls.clone(),
        notifications.clone(),
        Some(json!({"health": health(true, false), "count": 0})),
    );
    assert_eq!(
        zero.project(&Stores::default())["launcher.updates.count"],
        Value::Number(0.)
    );
    let invalid = make(
        controls,
        notifications,
        Some(json!({"health": health(true, false), "count": "0"})),
    );
    assert_eq!(
        invalid.project(&Stores::default())["launcher.updates.count"],
        Value::Number(-1.)
    );
}

#[test]
fn disconnected_updates_topic_is_unknown() {
    let publisher = FakeSource::new(json!({"health": health(true, false), "count": 7})).unwrap();
    publisher.disconnect().unwrap();
    let quick = Quick::new(None, vec![("dashboard.updates", Arc::new(publisher))]);
    assert_eq!(
        quick.project(&Stores::default())["launcher.updates.count"],
        Value::Number(-1.)
    );
}
