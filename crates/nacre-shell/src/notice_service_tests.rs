// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead::core::source::Mirror;
use nacre_protocol::{Message, framing};

fn state(extra: Json) -> NotificationsState {
    let notice = |rev: u64, default: Json, actions: Json, urgency: &str| {
        json!({"app_name":"Mail","summary":"Build finished","body":"All checks passed",
            "urgency":urgency,"resident":false,"transient":false,"created_ms":1_000,
            "actions":actions,"default_action":default,"revision":rev,"retained":true,"paused":false})
    };
    let mut data = json!({
        "health":{"available":true,"stale":false},"do_not_disturb":false,
        "active":{"items":{"n1":notice(2,json!("a2"),
            json!({"items":{"a1":{"label":"Reply"},"a2":{"label":"Open"}},"order":["a1","a2"]}),"critical")},
            "order":["n1"]},
        "history":{"items":{"h1":notice(1,Json::Null,json!({"items":{},"order":[]}),"low"),
            "h2":notice(4,Json::Null,json!({"items":{},"order":[]}),"normal")},"order":["h2","h1"]},
        "popup_order":["n1"],"inhibited":false,"history_count":9,"history_more":true,
        "history_storage":{"ready":true,"writable":true}});
    for (k, v) in extra.as_object().into_iter().flatten() {
        data[k] = v.clone();
    }
    let parsed: NotificationsState = serde_json::from_value(data).expect("fixture parses");
    assert!(parsed.producer_valid());
    parsed
}
fn text(v: &BTreeMap<String, Value>, key: &str) -> String {
    match &v[key] {
        Value::Text(t) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}
fn flag(v: &BTreeMap<String, Value>, key: &str) -> bool {
    matches!(v[key], Value::Bool(true))
}
fn rows(v: &BTreeMap<String, Value>, key: &str) -> Vec<Json> {
    match &v[key] {
        Value::List(items) => items.as_ref().clone(),
        other => panic!("{key}: {other:?}"),
    }
}
fn call(p: Result<Plan, &'static str>) -> (&'static str, Json) {
    match p.expect("planned") {
        Plan::Call(m, p) => (m, p),
        other => panic!("{other:?}"),
    }
}

#[test]
fn popup_rows_follow_popup_order_and_hide_the_default_action() {
    let s = state(json!({}));
    let v = project(Some(&s), true, &BTreeSet::new(), 61_000);
    assert!(flag(&v, "notifications.open") && flag(&v, "notifications.controls"));
    assert_eq!(text(&v, "notifications.state"), "ready");
    let popups = rows(&v, "notifications.rows");
    assert_eq!(popups.len(), 1);
    assert_eq!(popups[0]["key"], "n1|2");
    assert_eq!(popups[0]["urgency"], 2);
    assert_eq!(popups[0]["default"], "a2");
    // a2 is the default: only the explicit action stays a button.
    assert_eq!(
        popups[0]["actions"],
        json!([{"id":"a1","ref":"n1|2|a1","label":"Reply"}])
    );
    assert_eq!(popups[0]["time_str"], "1m");
    assert_eq!(popups[0]["expanded"], false);
}

#[test]
fn expansion_is_local_and_dnd_or_inhibition_empties_the_stack() {
    let s = state(json!({}));
    let v = project(Some(&s), true, &BTreeSet::from(["n1".into()]), 0);
    assert_eq!(rows(&v, "notifications.rows")[0]["expanded"], true);
    let quiet = state(json!({"do_not_disturb":true,"inhibited":true,"popup_order":[]}));
    let v = project(Some(&quiet), true, &BTreeSet::new(), 0);
    assert!(!flag(&v, "notifications.open") && flag(&v, "notifications.dnd"));
    assert!(flag(&v, "notifications.inhibited"));
    // The active notice still lives; only popup eligibility changed.
    assert_eq!(rows(&v, "notifications.history").len(), 2);
}

#[test]
fn history_is_the_bounded_newest_first_window_with_separate_total_and_more() {
    let s = state(json!({}));
    let v = project(Some(&s), true, &BTreeSet::new(), 0);
    let ids: Vec<_> = rows(&v, "notifications.history")
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["h2", "h1"]);
    assert!(rows(&v, "notifications.history")[0]["actions"] == json!([]));
    assert!(rows(&v, "notifications.history")[0]["history"] == json!(true));
    assert!(matches!(v["notifications.history_count"], Value::Number(n) if n == 9.));
    assert!(flag(&v, "notifications.history_more") && flag(&v, "notifications.history_known"));
    assert_eq!(text(&v, "notifications.history_state"), "ready");
}

#[test]
fn stale_and_absent_producers_are_distinct_and_disable_popups_and_controls() {
    let s = state(json!({}));
    let v = project(Some(&s), false, &BTreeSet::new(), 0);
    assert_eq!(text(&v, "notifications.state"), "stale");
    assert!(!flag(&v, "notifications.open") && !flag(&v, "notifications.controls"));
    assert_eq!(
        rows(&v, "notifications.history").len(),
        2,
        "last-good history stays"
    );
    let none = project(None, false, &BTreeSet::new(), 0);
    assert_eq!(text(&none, "notifications.state"), "unavailable");
    assert_eq!(text(&none, "notifications.history_state"), "unavailable");
    assert!(!flag(&none, "notifications.history_known"));
}

#[test]
fn storage_loading_error_and_readonly_stay_separate_from_bus_health() {
    let loading = state(json!({"history_storage":{"ready":false,"writable":false},
        "history_count":null,"history_more":null}));
    let v = project(Some(&loading), true, &BTreeSet::new(), 0);
    assert_eq!(text(&v, "notifications.history_state"), "loading");
    assert_eq!(text(&v, "notifications.state"), "ready");
    assert!(!flag(&v, "notifications.history_known"));
    let failed = state(json!({"history_storage":{"ready":true,"writable":false,
        "error":{"code":"io","message":"History storage unavailable","retry":true}}}));
    let v = project(Some(&failed), true, &BTreeSet::new(), 0);
    assert_eq!(text(&v, "notifications.history_state"), "error");
    assert!(!flag(&v, "notifications.writable"));
    let readonly = state(json!({"history_storage":{"ready":true,"writable":false}}));
    let v = project(Some(&readonly), true, &BTreeSet::new(), 0);
    assert_eq!(text(&v, "notifications.history_state"), "ready");
    assert!(!flag(&v, "notifications.writable"));
}

#[test]
fn dismiss_retains_and_forget_deletes_with_the_displayed_revision() {
    let s = state(json!({}));
    let arg = Value::Text("n1|2".into());
    assert_eq!(
        call(plan("notifications.dismiss", &arg, &s, false)),
        (
            "notifications.dismiss",
            json!({"id":"n1","revision":2,"forget":false})
        )
    );
    let hist = Value::Text("h2|4".into());
    assert_eq!(
        call(plan("notifications.forget", &hist, &s, false)),
        (
            "notifications.dismiss",
            json!({"id":"h2","revision":4,"forget":true})
        )
    );
    // A replaced notice or a missing one never reaches the daemon.
    for stale in ["n1|1", "gone|2", "n1", "n1|x"] {
        assert!(
            plan(
                "notifications.dismiss",
                &Value::Text(stale.into()),
                &s,
                false
            )
            .is_err()
        );
    }
}

#[test]
fn action_buttons_need_a_live_action_on_the_displayed_revision() {
    let s = state(json!({}));
    let ok = Value::Text("n1|2|a1".into());
    assert_eq!(
        call(plan("notifications.action", &ok, &s, false)),
        (
            "notifications.invoke_action",
            json!({"id":"n1","revision":2,"action":"a1"})
        )
    );
    for bad in ["n1|2|zz", "n1|3|a1", "n1|2", "h2|4|a1"] {
        assert!(plan("notifications.action", &Value::Text(bad.into()), &s, false).is_err());
    }
}

#[test]
fn card_click_invokes_the_default_only_when_the_policy_is_on() {
    let s = state(json!({}));
    let arg = Value::Text("n1|2".into());
    assert_eq!(
        plan("notifications.card", &arg, &s, false),
        Ok(Plan::Toggle("n1".into()))
    );
    assert_eq!(
        call(plan("notifications.card", &arg, &s, true)),
        (
            "notifications.invoke_action",
            json!({"id":"n1","revision":2,"action":"a2"})
        )
    );
    // History rows have no live actions: the card only expands.
    let hist = Value::Text("h2|4".into());
    assert_eq!(
        plan("notifications.card", &hist, &s, true),
        Ok(Plan::Toggle("h2".into()))
    );
    // A notice without a default action also just expands.
    let none = state(
        json!({"active":{"items":{"n1":{"app_name":"Mail","summary":"s","body":"",
        "urgency":"normal","resident":false,"transient":false,"created_ms":1,
        "actions":{"items":{},"order":[]},"default_action":null,"revision":2,"retained":true,"paused":false}},
        "order":["n1"]}}),
    );
    assert_eq!(
        plan("notifications.card", &arg, &none, true),
        Ok(Plan::Toggle("n1".into()))
    );
}

#[test]
fn dnd_and_clear_use_the_published_methods() {
    let s = state(json!({}));
    assert_eq!(
        call(plan(
            "notifications.dnd",
            &Value::Text("toggle".into()),
            &s,
            false
        )),
        ("notifications.set_dnd", json!({"enabled":true}))
    );
    assert_eq!(
        call(plan("notifications.dnd", &Value::Bool(false), &s, false)),
        ("notifications.set_dnd", json!({"enabled":false}))
    );
    assert_eq!(
        call(plan("notifications.clear", &Value::Bool(true), &s, false)),
        ("notifications.clear", json!({"scope":"history"}))
    );
    assert_eq!(
        call(plan(
            "notifications.clear_popups",
            &Value::Bool(true),
            &s,
            false
        )),
        ("notifications.clear", json!({"scope":"popups"}))
    );
    assert!(plan("notifications.bogus", &Value::Bool(true), &s, false).is_err());
}

#[test]
fn hover_keys_cover_cards_buttons_and_the_osd_only() {
    let t = |s: &str| Value::Text(s.into());
    assert_eq!(
        hover_key("notifications.card", &t("n1|2")),
        Some("n1|2".into())
    );
    assert_eq!(
        hover_key("notifications.action", &t("n1|2|a1")),
        Some("n1|2".into())
    );
    assert_eq!(
        hover_key("notifications.osd", &Value::Bool(true)),
        Some("osd".into())
    );
    assert_eq!(hover_key("notifications.dnd", &t("toggle")), None);
    assert_eq!(hover_key("bar.launcher", &t("x")), None);
}

fn snap(data: Json, connected: bool) -> Snapshot {
    Snapshot {
        epoch: "b1".into(),
        revision: 1,
        data: Arc::new(data),
        connected,
    }
}
fn shared() -> Arc<Shared> {
    let s = Arc::new(Shared {
        inner: Mutex::new(Inner::default()),
        wake: Condvar::new(),
    });
    s.lock().osd.armed = Some(Instant::now());
    s
}
fn event(data: Json) -> OsdEvent {
    let mut data = data;
    data["ts"] = json!(1_800_000_000_000_u64);
    serde_json::from_value(data).expect("event parses")
}
fn shown(s: &Shared) -> BTreeMap<String, Value> {
    osd_values(s.lock().osd.shown.as_ref())
}
fn number(v: &BTreeMap<String, Value>, key: &str) -> f64 {
    match &v[key] {
        Value::Number(n) => *n,
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn every_osd_kind_is_projected_as_its_event_fields() {
    let s = shared();
    for (data, kind, value) in [
        (
            json!({"kind":"volume","value":0.5,"muted":false,"label":"Speakers","device":"alsa_output.x"}),
            "volume",
            0.5,
        ),
        (
            json!({"kind":"output","value":0.3,"muted":false,"label":"Headphones","device":"bluez_output.1"}),
            "output",
            0.3,
        ),
        (
            json!({"kind":"microphone","value":0.0,"muted":true,"label":"Built-in microphone"}),
            "microphone",
            0.0,
        ),
        (
            json!({"kind":"brightness","value":0.75,"device":"eDP-1"}),
            "brightness",
            0.75,
        ),
        (
            json!({"kind":"keyboard","value":0.5,"max":2}),
            "keyboard",
            0.5,
        ),
        (json!({"kind":"night_light","value":1}), "night_light", 1.),
        (
            json!({"kind":"power_profile","label":"power-saver"}),
            "power_profile",
            0.,
        ),
        (json!({"kind":"capslock","value":0}), "capslock", 0.),
    ] {
        let label = data["label"].as_str().unwrap_or("").to_owned();
        deliver(&s, &event(data));
        let v = shown(&s);
        assert!(flag(&v, "osd.open"), "{kind}");
        assert_eq!(text(&v, "osd.kind"), kind);
        assert!((number(&v, "osd.value") - value).abs() < 1e-9, "{kind}");
        assert_eq!(text(&v, "osd.label"), label, "{kind}");
        assert!(
            s.lock().osd.deadline.is_some(),
            "{kind}: the hide timer starts"
        );
        s.lock().osd.shown = None;
    }
    assert!(!flag(&osd_values(None), "osd.open"));
}

#[test]
fn osd_events_clamp_the_level_and_refuse_invalid_ones() {
    let s = shared();
    deliver(
        &s,
        &event(json!({"kind":"volume","value":1.4,"muted":false})),
    );
    assert_eq!(number(&shown(&s), "osd.value"), 1.);
    deliver(&s, &event(json!({"kind":"volume","muted":true})));
    let v = shown(&s);
    assert_eq!(number(&v, "osd.value"), 0.);
    assert!(flag(&v, "osd.muted"));
    s.lock().osd.shown = None;
    // A toggle outside 0/1 and a brightness above 1 are not valid events.
    deliver(&s, &event(json!({"kind":"capslock","value":0.5})));
    deliver(&s, &event(json!({"kind":"brightness","value":1.5})));
    assert!(s.lock().osd.shown.is_none());
}

#[test]
fn osd_arming_and_hover_control_the_hide_clock() {
    let s = shared();
    let now = Instant::now();
    let any = || shown_from(&event(json!({"kind":"volume","value":0.5}))).unwrap();
    let mut g = s.lock();
    g.osd.armed = Some(now + Duration::from_secs(5));
    g.osd.show(any(), now);
    assert!(g.osd.shown.is_none(), "inside the arming window");
    g.osd.armed = Some(now);
    g.osd.hovered = true;
    g.osd.show(any(), now);
    assert!(
        g.osd.shown.is_some() && g.osd.deadline.is_none(),
        "hover holds it open"
    );
}

#[test]
fn route_event_reads_only_osd_changed_events() {
    use nacre_protocol::Message;
    let s = shared();
    SINKS.lock().unwrap().push(Arc::downgrade(&s));
    let ev = |topic: &str, name: &str, data: Json| Message::Ev {
        topic: topic.into(),
        name: name.into(),
        seq: 1,
        data,
    };
    let volume = json!({"kind":"volume","value":0.2,"ts":1});
    route_event(&ev("audio", "changed", volume.clone()));
    route_event(&ev("osd", "other", volume.clone()));
    route_event(&ev("osd", "changed", json!({"kind":"nonsense","ts":1})));
    assert!(s.lock().osd.shown.is_none());
    route_event(&ev("osd", "changed", volume));
    assert!(s.lock().osd.shown.is_some());
}

fn replay(transcript: &str) -> Vec<Json> {
    let now = Instant::now();
    let mut mirror = Mirror::new(now);
    mirror.reconnected("b1", now).unwrap();
    let mut states = vec![];
    for line in transcript.lines() {
        match framing::decode(line.as_bytes()).unwrap() {
            Message::Snap { topic, seq, data } if topic == "notifications" => {
                mirror
                    .accept(
                        Arc::new(Snapshot {
                            epoch: "b1".into(),
                            revision: seq,
                            data: Arc::new(data),
                            connected: true,
                        }),
                        now,
                    )
                    .unwrap();
            }
            Message::Patch {
                topic,
                base,
                seq,
                data,
            } if topic == "notifications" => {
                mirror.patch("b1", base, seq, &data, now).unwrap();
            }
            _ => continue,
        }
        states.push((*mirror.snapshot().unwrap().data).clone());
    }
    states
}
#[test]
fn published_lifecycle_transcript_projects_every_step_and_marks_the_default_action() {
    let steps = replay(include_str!(
        "../tests/protocol-v1/notifications-lifecycle.jsonl"
    ));
    assert!(steps.len() >= 4);
    let mut saw_replacement = false;
    for data in &steps {
        let s: NotificationsState = serde_json::from_value(data.clone()).unwrap();
        assert!(s.producer_valid());
        let v = project(Some(&s), true, &BTreeSet::new(), 1_800_000_000_000);
        for r in rows(&v, "notifications.rows") {
            let default = r["default"].as_str().unwrap();
            assert!(
                r["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|a| a["id"] != default)
            );
            saw_replacement |= r["rev"] == 2 && default == "a2";
        }
    }
    assert!(
        saw_replacement,
        "the replacement revision carries a fresh default"
    );
}
#[test]
fn published_storage_and_loss_transcripts_stay_valid_and_distinguish_loading_from_error() {
    let storage = replay(include_str!(
        "../tests/protocol-v1/notifications-storage.jsonl"
    ));
    let phases: Vec<String> = storage
        .iter()
        .map(|d| {
            let s: NotificationsState = serde_json::from_value(d.clone()).unwrap();
            assert!(s.producer_valid());
            text(
                &project(Some(&s), true, &BTreeSet::new(), 0),
                "notifications.history_state",
            )
        })
        .collect();
    assert_eq!(phases[0], "loading");
    assert!(phases.contains(&"error".to_string()) && phases.last().is_some_and(|p| p == "ready"));
    for d in replay(include_str!(
        "../tests/protocol-v1/notifications-loss-restart.jsonl"
    )) {
        let s: NotificationsState = serde_json::from_value(d).unwrap();
        assert!(s.producer_valid());
    }
}
#[test]
fn ownership_refusal_transcript_advertises_no_notifications_topic() {
    let first = include_str!("../tests/protocol-v1/notifications-ownership.jsonl")
        .lines()
        .next()
        .unwrap();
    let Message::Welcome {
        topics, methods, ..
    } = framing::decode(first.as_bytes()).unwrap()
    else {
        panic!("welcome expected");
    };
    assert!(!topics.contains_key("notifications") && methods.is_empty());
}

#[test]
fn callbacks_keep_last_good_data_stale_and_reject_legacy_payloads() {
    let s = shared();
    let good = serde_json::to_value(state(json!({}))).unwrap();
    on_notifications(&s, &snap(good.clone(), true));
    assert!(s.lock().live);
    on_notifications(&s, &snap(good, false));
    {
        let g = s.lock();
        assert!(!g.live && g.good.is_some());
    }
    // A legacy payload (no producer members) never becomes live.
    let legacy = json!({"health":{"available":true,"stale":false},"do_not_disturb":false,
        "active":{"items":{},"order":[]},"history":{"items":{},"order":[]}});
    on_notifications(&s, &snap(legacy, true));
    assert!(!s.lock().live);
    let v = models(&s.lock());
    assert_eq!(text(&v, "notifications.state"), "stale");
}
#[test]
fn seed_defines_every_key_the_views_bind() {
    let seeded = seed();
    for key in [
        "notifications.rows",
        "notifications.open",
        "notifications.history",
        "notifications.history_count",
        "notifications.history_more",
        "notifications.history_state",
        "notifications.state",
        "notifications.dnd",
        "notifications.message",
        "notifications.controls",
        "osd.open",
        "osd.kind",
        "osd.value",
        "osd.muted",
        "osd.label",
        "osd.device",
    ] {
        assert!(seeded.contains_key(key), "{key}");
    }
}

#[test]
fn views_are_offered_for_popups_and_osd_and_yield_to_other_panels() {
    let shared = Shared {
        inner: Mutex::new(Inner::default()),
        wake: Condvar::new(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let mut g = shared.lock();
        g.live = true;
        g.good = Some(state(json!({})));
        g.drivers.insert(
            1,
            Arc::new(move |event| {
                let _ = tx.send(event);
            }),
        );
    }
    let offers = || {
        rx.try_iter()
            .filter_map(|event| match event {
                UiEvent::CompanionOffer { view, open, .. } => Some((view, open)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    publish(&shared);
    assert_eq!(offers(), [(VIEW_NOTICES.to_owned(), true)]);
    // Another view is shown: the popups yield, once, and nothing is offered after.
    shared.lock().other_panel = true;
    publish(&shared);
    assert_eq!(offers(), [(VIEW_NOTICES.to_owned(), false)]);
    publish(&shared);
    assert!(offers().is_empty());
    // The OSD wins over popups and is offered even while another view is shown.
    shared.lock().osd.shown = shown_from(&event(json!({"kind":"volume","value":0.5})));
    publish(&shared);
    assert_eq!(offers(), [(VIEW_OSD.to_owned(), true)]);
    let mut g = shared.lock();
    g.osd.shown = None;
    g.other_panel = false;
    g.live = false;
    drop(g);
    publish(&shared);
    assert_eq!(offers(), [(VIEW_OSD.to_owned(), false)]);
}

#[test]
fn launcher_dashboard_and_chat_coexist_with_popups_other_views_suppress() {
    let coexist: Vec<String> = ["launcher", "dashboard", "chat"].map(String::from).into();
    for view in ["launcher", "dashboard", "dashboard-home", "chat"] {
        assert!(!suppresses(&coexist, view), "{view}");
    }
    // Popouts, status peeks, the session menu and the control drawer suppress.
    for view in ["calendar", "sound", "peek-network", "session", "control", "dashboardx"] {
        assert!(suppresses(&coexist, view), "{view}");
    }
}

#[test]
fn offers_name_the_compositor_focused_output_and_fall_back_to_the_leading_one() {
    let shared = Shared {
        inner: Mutex::new(Inner::default()),
        wake: Condvar::new(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let mut g = shared.lock();
        g.live = true;
        g.good = Some(state(json!({})));
        g.drivers.insert(
            1,
            Arc::new(move |event| {
                let _ = tx.send(event);
            }),
        );
    }
    let offered = || {
        rx.try_iter()
            .filter_map(|event| match event {
                UiEvent::CompanionOffer { output, open, .. } => Some((output, open)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    publish(&shared);
    assert_eq!(offered(), [(None, true)], "unknown focus: leading output");
    let focus = |data: Json, connected| on_compositor(&shared, &snap(data, connected));
    focus(json!({"focus": {"outputId": "DP-2"}}), true);
    publish(&shared);
    assert_eq!(offered(), [(Some("DP-2".to_owned()), true)]);
    focus(json!({"focus": {"outputId": "eDP-1"}}), true);
    publish(&shared);
    assert_eq!(offered(), [(Some("eDP-1".to_owned()), true)]);
    // A malformed payload or a lost link clears it again.
    focus(json!({"focus": {"outputId": null}}), true);
    publish(&shared);
    assert_eq!(offered(), [(None, true)]);
    focus(json!({"focus": {"outputId": "DP-2"}}), false);
    publish(&shared);
    assert_eq!(offered(), [(None, true)]);
}

mod daemon {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    struct Lab(std::path::PathBuf);
    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn lab() -> Lab {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nc-notice-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        Lab(dir)
    }
    /// A daemon that speaks the published handshake, serves one snapshot and
    /// acknowledges every call, reporting the calls it saw.
    fn fake(
        lab: &Lab,
        topics: Json,
        snapshot: Json,
    ) -> (std::path::PathBuf, mpsc::Receiver<Json>, mpsc::Sender<()>) {
        fake_events(lab, topics, snapshot, vec![])
    }
    /// Like [`fake`]; once the client subscribes `osd` it publishes `events` as `osd`
    /// `changed` events (after the consumer's arming window, 30 ms apart).
    fn fake_events(
        lab: &Lab,
        topics: Json,
        snapshot: Json,
        events: Vec<Json>,
    ) -> (std::path::PathBuf, mpsc::Receiver<Json>, mpsc::Sender<()>) {
        let socket = lab.0.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (seen, calls) = mpsc::channel();
        let (close, closed) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(20)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut welcome: Json = serde_json::from_str(
                include_str!("../tests/protocol-v1/notifications-loss-restart.jsonl")
                    .lines()
                    .next()
                    .unwrap(),
            )
            .unwrap();
            welcome["topics"] = topics;
            let events_writer = writer.try_clone().unwrap();
            let mut send = |v: &Json| writeln!(writer, "{v}").unwrap();
            let mut line = String::new();
            loop {
                if closed.try_recv().is_ok() {
                    return;
                }
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => return,
                    Ok(_) => {}
                    Err(_) => continue,
                }
                let frame: Json = serde_json::from_str(&line).unwrap();
                match frame["t"].as_str().unwrap() {
                    "hello" => send(&welcome),
                    "sub" => {
                        let has = |topic: &str| {
                            frame["topics"]
                                .as_array()
                                .is_some_and(|t| t.contains(&json!(topic)))
                        };
                        if has("notifications") {
                            send(
                                &json!({"t":"snap","topic":"notifications","seq":0,"data":snapshot}),
                            );
                        }
                        if has("osd") {
                            send(
                                &json!({"t":"snap","topic":"osd","seq":0,"data":{"health":{"available":true,"stale":false}}}),
                            );
                            let mut out = events_writer.try_clone().unwrap();
                            let events = events.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(700));
                                for (i, data) in events.iter().enumerate() {
                                    let ev = json!({"t":"ev","topic":"osd","name":"changed","seq":i + 1,"data":data});
                                    let _ = writeln!(out, "{ev}");
                                    std::thread::sleep(Duration::from_millis(30));
                                }
                            });
                        }
                    }
                    "sub"
                        if frame["topics"]
                            .as_array()
                            .is_some_and(|t| t.contains(&json!("compositor"))) =>
                    {
                        send(&json!({"t":"snap","topic":"compositor","seq":0,
                            "data":{"focus":{"outputId":"DP-9"}}}));
                    }
                    "call" => {
                        send(&json!({"t":"ret","id":frame["id"],"ok":{"changed":true}}));
                        seen.send(frame).unwrap();
                    }
                    _ => {
                        seen.send(frame).unwrap();
                    }
                }
            }
        });
        (socket, calls, close)
    }
    fn start(socket: std::path::PathBuf, provider: bool, action_on_click: bool) -> Arc<Service> {
        let client = if provider {
            crate::ui_provider::start(socket, "/tmp/bead-test-shell.sock".into(), None, &TOPICS)
                .unwrap()
        } else {
            let topics = TOPICS.iter().map(|t| t.to_string()).collect();
            Client::start(crate::nacre_source::Config::new(socket, topics)).unwrap()
        };
        Service::start(
            client,
            Options {
                action_on_click,
                ..Default::default()
            },
        ).unwrap()
    }
    fn models(service: &Service) -> mpsc::Receiver<BTreeMap<String, Value>> {
        let (tx, rx) = mpsc::channel();
        let handle = service.subscribe(
            Arc::new(move |event| {
                if let UiEvent::Model(m) = event {
                    let _ = tx.send(m);
                }
            }),
            true,
        );
        std::mem::forget(handle);
        rx
    }
    fn until(
        rx: &mpsc::Receiver<BTreeMap<String, Value>>,
        ok: impl Fn(&BTreeMap<String, Value>) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(m) = rx.recv_timeout(Duration::from_millis(50))
                && ok(&m)
            {
                return;
            }
        }
        panic!("expected model state never arrived");
    }
    fn call(calls: &mpsc::Receiver<Json>, method: &str) -> Json {
        loop {
            let frame = calls
                .recv_timeout(Duration::from_secs(5))
                .expect("call frame");
            if frame["m"] == method {
                return frame["p"].clone();
            }
        }
    }

    #[test]
    fn new_popups_are_offered_on_the_daemon_published_focused_output() {
        let lab = lab();
        let data = serde_json::to_value(state(json!({}))).unwrap();
        let (socket, _calls, _close) =
            fake(&lab, json!({"notifications":1,"compositor":1}), data);
        let service = start(socket, true, false);
        let (tx, rx) = mpsc::channel();
        std::mem::forget(service.drive(Arc::new(move |event| {
            if let UiEvent::CompanionOffer { output, open, .. } = event {
                let _ = tx.send((output, open));
            }
        })));
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = vec![];
        while Instant::now() < deadline {
            if let Ok(offer) = rx.recv_timeout(Duration::from_millis(50)) {
                seen.push(offer);
                if offer_has_focus(&seen) {
                    return;
                }
            }
            // Offers repeat with every publish: nudge one once the focus may be known.
            publish(&service.shared);
        }
        panic!("no offer named the focused output: {seen:?}");
    }
    fn offer_has_focus(seen: &[(Option<String>, bool)]) -> bool {
        seen.iter()
            .any(|(output, open)| *open && output.as_deref() == Some("DP-9"))
    }

    #[test]
    fn consumer_projects_the_snapshot_and_sends_revision_checked_calls() {
        let lab = lab();
        let data = serde_json::to_value(state(json!({}))).unwrap();
        let (socket, calls, close) = fake(&lab, json!({"notifications":1}), data);
        let service = start(socket, true, true);
        let rx = models(&service);
        until(&rx, |m| {
            flag(m, "notifications.open") && flag(m, "notifications.controls")
        });
        // The provider registers (hover and suppression are provider-only).
        assert!(
            calls
                .recv_timeout(Duration::from_secs(5))
                .is_ok_and(|f| f["t"] == "register")
        );
        service.hover("n1|2", true);
        assert_eq!(
            call(&calls, "notifications.set_hover"),
            json!({"id":"n1","revision":2,"hovered":true})
        );
        service.hover("n1|2", false);
        assert_eq!(
            call(&calls, "notifications.set_hover"),
            json!({"id":"n1","revision":2,"hovered":false})
        );
        service
            .action("notifications.card", &Value::Text("n1|2".into()))
            .unwrap();
        assert_eq!(
            call(&calls, "notifications.invoke_action"),
            json!({"id":"n1","revision":2,"action":"a2"})
        );
        service
            .action("notifications.forget", &Value::Text("h2|4".into()))
            .unwrap();
        assert_eq!(
            call(&calls, "notifications.dismiss"),
            json!({"id":"h2","revision":4,"forget":true})
        );
        // A stale reference is refused locally and never reaches the daemon.
        service
            .action("notifications.dismiss", &Value::Text("n1|1".into()))
            .unwrap();
        until(&rx, |m| {
            text(m, "notifications.message").contains("Changed elsewhere")
        });
        // Losing the daemon keeps the display but disables popups and controls.
        close.send(()).unwrap();
        until(&rx, |m| {
            text(m, "notifications.state") == "stale" && !flag(m, "notifications.open")
        });
        assert!(
            service
                .action("notifications.dismiss", &Value::Text("n1|2".into()))
                .is_err()
        );
    }

    #[test]
    fn daemon_osd_events_of_all_eight_kinds_drive_the_view_models_and_feedback_is_refused() {
        let lab = lab();
        let data = serde_json::to_value(state(json!({}))).unwrap();
        let cases = [
            (
                json!({"kind":"volume","value":0.5,"muted":false,"label":"Speakers","device":"d1"}),
                "volume",
                0.5,
            ),
            (
                json!({"kind":"output","value":0.3,"muted":false,"label":"Headphones","device":"bluez_output.1"}),
                "output",
                0.3,
            ),
            (
                json!({"kind":"microphone","value":0.0,"muted":true,"label":"Mic"}),
                "microphone",
                0.0,
            ),
            (
                json!({"kind":"brightness","value":0.75,"device":"eDP-1"}),
                "brightness",
                0.75,
            ),
            (
                json!({"kind":"keyboard","value":0.5,"max":2}),
                "keyboard",
                0.5,
            ),
            (json!({"kind":"night_light","value":1}), "night_light", 1.0),
            (
                json!({"kind":"power_profile","label":"performance"}),
                "power_profile",
                0.0,
            ),
            (json!({"kind":"capslock","value":1}), "capslock", 1.0),
        ];
        let events = cases
            .iter()
            .map(|(e, _, _)| {
                let mut e = e.clone();
                e["ts"] = json!(1_800_000_000_000_u64);
                e
            })
            .collect();
        let (socket, _calls, close) =
            fake_events(&lab, json!({"notifications":1,"osd":1}), data, events);
        let service = start(socket, true, false);
        let rx = models(&service);
        for (_, kind, value) in cases {
            until(&rx, |m| {
                flag(m, "osd.open")
                    && text(m, "osd.kind") == kind
                    && (number(m, "osd.value") - value).abs() < 1e-9
            });
        }
        assert!(
            service
                .feedback(&[json!("brightness"), json!(0.4), json!(false)])
                .is_err(),
            "the N0036 reply path is gone"
        );
        let _ = close.send(());
    }

    #[test]
    fn absent_capability_is_unavailable_and_sends_nothing() {
        let lab = lab();
        let data = serde_json::to_value(state(json!({}))).unwrap();
        let (socket, calls, close) = fake(&lab, json!({}), data);
        let service = start(socket, false, false);
        let rx = models(&service);
        std::thread::sleep(Duration::from_millis(300));
        until(&rx, |m| {
            text(m, "notifications.state") == "unavailable" && !flag(m, "notifications.controls")
        });
        assert!(
            service
                .action("notifications.dnd", &Value::Text("toggle".into()))
                .is_err()
        );
        assert!(
            calls.recv_timeout(Duration::from_millis(200)).is_err(),
            "no call, no register"
        );
        let _ = close.send(());
    }
}
