// SPDX-License-Identifier: GPL-3.0-only
//! B0027: the N0034 pages, tab customization and their gated calls.
use super::tests::{extra_harness, fixture, root};
use super::*;
use crate::nacre_source::{CallWake, Client, Config as ClientConfig};
use bead_test::{Harness, Result, source::FakeSource};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::{os::unix::fs::PermissionsExt, time::Duration};
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

const PAGES: [Page; 6] = [
    Page::Focus,
    Page::Clipboard,
    Page::AiUsage,
    Page::Screenshots,
    Page::Brain,
    Page::Customize,
];
// 750 s before the focus fixture's deadline (12:30 left of 25:00).
fn fixed_now() -> u64 {
    1709193600000 - 750_000
}
fn page_view(page: Page, data: Json) -> Dashboard {
    Dashboard::new(page, Arc::new(FakeSource::new(data).unwrap()), None, true).with_now(fixed_now)
}
fn rows(model: &BTreeMap<String, Value>, key: &str) -> Vec<Json> {
    match &model[key] {
        Value::List(list) => list.to_vec(),
        other => panic!("{key}: {other:?}"),
    }
}
fn text(model: &BTreeMap<String, Value>, key: &str) -> String {
    match &model[key] {
        Value::Text(text) => text.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn registry_slots_map_to_pages_and_customize_is_reached_by_view_only() -> Result<()> {
    for (index, name) in [
        "focus",
        "notifications",
        "clipboard",
        "devices",
        "network",
        "ai-usage",
        "screenshots",
        "brain",
    ]
    .into_iter()
    .enumerate()
    {
        let page = Page::from_tab(index + 4).expect(name);
        assert_eq!(page.name(), name);
        assert_eq!(page.tab(), index + 4);
        assert_eq!(Page::REGISTRY[index], page);
        let call = crate::commands::from_strings(
            "nacre".into(),
            "tab".into(),
            &[(index + 4).to_string()],
        )?;
        assert!(crate::commands::map_views(call).is_ok());
    }
    assert_eq!(
        Page::from_view("dashboard-customize"),
        Some(Page::Customize)
    );
    assert_eq!(
        Page::from_tab(0),
        Some(Page::Today),
        "customize has no slot"
    );
    Ok(())
}

#[test]
fn pages_have_complete_state_defaults_and_project_published_rows() -> Result<()> {
    for page in PAGES {
        let harness = extra_harness(page, BTreeMap::new(), 1.)?;
        let defaults = harness.session.stores.values();
        let unavailable =
            Dashboard::new(page, Arc::new(ReadOnly::unavailable()?), None, false).project();
        assert_eq!(
            unavailable["dashboard.ready"],
            Value::Bool(false),
            "{page:?}"
        );
        assert_eq!(unavailable["dashboard.control"], Value::Bool(false));

        let data = fixture(page.name());
        let shown = page_view(page, data).project();
        assert_eq!(shown["dashboard.ready"], Value::Bool(true), "{page:?}");
        assert_eq!(shown["dashboard.stale"], Value::Bool(false), "{page:?}");
        for key in shown.keys().filter(|key| key.starts_with("dashboard.")) {
            assert!(
                defaults.contains_key(key),
                "{} state key {key}",
                page.name()
            );
        }
        // No absolute path ever reaches the view.
        assert!(!format!("{shown:?}").contains("/preview"), "{page:?}");
        match page {
            Page::Focus => {
                assert_eq!(text(&shown, "dashboard.state"), "running");
                assert_eq!(text(&shown, "dashboard.metric"), "12:30");
                assert_eq!(shown["dashboard.progress"], Value::Number(0.5));
            }
            Page::Clipboard => {
                let list = rows(&shown, "dashboard.rows");
                assert_eq!(list.len(), 3);
                assert_eq!(list[0]["act"], json!("clip.copy:c12"));
                assert_eq!(list[1]["value"], json!("Image"));
            }
            Page::Screenshots => {
                let list = rows(&shown, "dashboard.rows");
                assert_eq!(list[0]["label"], json!("shot-2.png"));
                assert_eq!(list[0]["act"], json!("shot.open:s2"));
            }
            Page::AiUsage => assert_eq!(rows(&shown, "dashboard.rows").len(), 5),
            Page::Brain => {
                assert_eq!(rows(&shown, "dashboard.rows").len(), 2);
                assert_eq!(rows(&shown, "dashboard.secondary").len(), 1);
            }
            Page::Customize => {
                let list = rows(&shown, "dashboard.rows");
                assert_eq!(list.len(), 8);
                assert_eq!(list[0]["id"], json!("focus"));
                assert_eq!(list[0]["value"], json!("On"));
                assert_eq!(list[1]["value"], json!("Off"));
                assert_eq!(list[2]["toggle"], json!("tab.toggle:clipboard"));
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[test]
fn focus_counts_down_locally_and_never_shows_a_negative_or_invented_time() {
    let mut data = fixture("focus");
    let at = |now: fn() -> u64, data: &Json| {
        Dashboard::new(
            Page::Focus,
            Arc::new(FakeSource::new(data.clone()).unwrap()),
            None,
            true,
        )
        .with_now(now)
        .project()
    };
    assert_eq!(
        text(&at(|| 1709193600000 - 61_000, &data), "dashboard.metric"),
        "01:01"
    );
    assert_eq!(
        text(&at(|| 1709193600000 - 60_001, &data), "dashboard.metric"),
        "01:01"
    );
    // Past the deadline the daemon will flip to done; until then the view stays at zero.
    assert_eq!(
        text(&at(|| 1709193600000 + 5_000, &data), "dashboard.metric"),
        "00:00"
    );
    data["status"] = json!("paused");
    data["remaining_s"] = json!(90);
    data.as_object_mut().unwrap().remove("deadline_ms");
    data["owns_dnd"] = json!(false);
    let paused = at(|| 0, &data);
    assert_eq!(text(&paused, "dashboard.metric"), "01:30");
    assert_eq!(text(&paused, "dashboard.state"), "paused");
    data["status"] = json!("idle");
    data.as_object_mut().unwrap().remove("remaining_s");
    data["total_s"] = json!(0);
    let idle = at(|| 0, &data);
    assert_eq!(text(&idle, "dashboard.metric"), "—");
    assert_eq!(idle["dashboard.progress"], Value::Number(0.));
}

#[test]
fn invalid_unknown_and_disconnected_data_stays_unavailable_or_stale() -> Result<()> {
    for page in &PAGES[..5] {
        let invalid =
            page_view(*page, json!({"health":{"available":true,"stale":false}})).project();
        assert_eq!(invalid["dashboard.ready"], Value::Bool(false), "{page:?}");
        let mut stale = fixture(page.name());
        stale["health"]["stale"] = json!(true);
        assert_eq!(
            page_view(*page, stale).project()["dashboard.stale"],
            Value::Bool(true)
        );
        let source = Arc::new(FakeSource::new(fixture(page.name()))?);
        let dashboard = Dashboard::new(*page, source.clone(), None, true).with_now(fixed_now);
        let before = dashboard.project();
        source.disconnect()?;
        let mut after = dashboard.project();
        assert_eq!(
            after.insert("dashboard.stale".into(), Value::Bool(false)),
            Some(Value::Bool(true))
        );
        assert_eq!(before, after, "{page:?} keeps the last good projection");
    }
    // Control characters in a published preview never reach a text node.
    let mut data = fixture("clipboard");
    data["entries"]["items"]["c12"]["preview"] = json!("a\nb");
    assert_eq!(
        page_view(Page::Clipboard, data).project()["dashboard.ready"],
        Value::Bool(false)
    );
    Ok(())
}

#[test]
fn tabs_follow_the_revisioned_store_and_always_keep_the_open_page() {
    let tabs = |page: Page, store: Option<Json>| -> Vec<String> {
        let mut dashboard =
            Dashboard::new(page, Arc::new(ReadOnly::unavailable().unwrap()), None, true);
        dashboard.settings =
            store.map(|s| Arc::new(FakeSource::new(s).unwrap()) as Arc<dyn Source>);
        rows(&dashboard.project(), "dashboard.tabs")
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    let core = ["today", "weather", "media", "system"];
    assert_eq!(tabs(Page::Today, None), core);
    // Enabled pages follow `order`, not `enabled`; unknown names are ignored.
    let mut store = fixture("settings");
    store["data"]["enabled"] = json!(["brain", "focus", "bogus", "focus"]);
    store["data"]["order"] = json!(["brain", "focus"]);
    let listed = tabs(Page::Today, Some(store.clone()));
    assert_eq!(listed[4..], ["brain", "focus"]);
    // The open page is shown even when it is disabled; a disabled page is not.
    store["data"]["enabled"] = json!([]);
    assert_eq!(
        tabs(Page::Network, Some(store.clone())).last().unwrap(),
        "network"
    );
    assert_eq!(tabs(Page::Today, Some(store.clone())), core);
    assert_eq!(
        tabs(Page::Customize, Some(store)).last().unwrap(),
        "customize"
    );
}

#[test]
fn pages_have_deterministic_pixels_and_only_declared_actions() -> Result<()> {
    for page in PAGES {
        for scale in [1., 1.5] {
            let model = page_view(page, fixture(page.name())).project();
            let mut h = extra_harness(page, model, scale)?;
            for node in h.session.view_nodes() {
                assert!(
                    matches!(
                        node.action.as_deref(),
                        None | Some("view.open" | "dashboard.do" | "focus_minutes")
                    ),
                    "{page:?}: {:?}",
                    node.action
                );
            }
            let frame = h.render()?;
            let golden = root().join(format!(
                "tests/goldens/dashboard-{}-{scale}.png",
                page.name()
            ));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-dashboard-{}-{scale}-diff", page.name())),
            )?;
        }
    }
    Ok(())
}

#[test]
fn pages_release_their_sources_on_retirement() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};
    for page in PAGES {
        let main = Arc::new(FakeSource::new(fixture(page.name()))?);
        let settings = Arc::new(FakeSource::new(fixture("settings"))?);
        let before = (Arc::strong_count(&main), Arc::strong_count(&settings));
        let mut harness = extra_harness(page, BTreeMap::new(), 1.)?;
        harness.render()?;
        let dashboard = Dashboard::new(page, main.clone(), None, false).with_controls(
            Some(settings.clone() as Arc<dyn Source>),
            None,
            None,
        );
        let mut runtime =
            crate::reload::Runtime::new(harness.session).with_dashboard_view(dashboard)?;
        let events = Arc::new(AtomicUsize::new(0));
        let counter = events.clone();
        runtime.services(Box::new(move |_: UiEvent| {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        let scene = runtime
            .session
            .update_model(BTreeMap::new())?
            .ok_or("page state")?;
        runtime.session.commit(scene);
        let subscribed = events.load(Ordering::Relaxed);
        assert!(subscribed > 0);
        drop(runtime);
        assert_eq!(
            (Arc::strong_count(&main), Arc::strong_count(&settings)),
            before
        );
        main.publish(fixture(page.name()))?;
        settings.publish(fixture("settings"))?;
        assert_eq!(
            events.load(Ordering::Relaxed),
            subscribed,
            "{page:?} callbacks stop"
        );
        assert_eq!(
            main.calls() + settings.calls(),
            0,
            "no call without the daemon"
        );
    }
    Ok(())
}

#[test]
fn the_countdown_wake_runs_only_while_a_session_is_visible_and_running() -> Result<()> {
    use bead::wayland::UiEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dashboard = Dashboard::new(
        Page::Focus,
        Arc::new(FakeSource::new(fixture("focus"))?),
        None,
        true,
    )
    .with_now(fixed_now);
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    let _subscriptions = dashboard.subscribe(Arc::new(move |_: UiEvent| {
        counter.fetch_add(1, Ordering::Relaxed);
    }))?;
    let open = bead::core::Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))?;
    let closed = bead::core::Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))?;
    dashboard.project_with_stores(&closed);
    assert!(
        dashboard.ticker.borrow().is_none(),
        "hidden panel never ticks"
    );
    dashboard.project_with_stores(&open);
    assert!(dashboard.ticker.borrow().is_some());
    std::thread::sleep(Duration::from_millis(1300));
    assert!(wakes.load(Ordering::Relaxed) >= 1);
    dashboard.project_with_stores(&closed);
    assert!(dashboard.ticker.borrow().is_none(), "released when hidden");
    std::thread::sleep(Duration::from_millis(100));
    let settled = wakes.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        wakes.load(Ordering::Relaxed),
        settled,
        "no wake after release"
    );
    Ok(())
}

fn media(now: fn() -> u64) -> Dashboard {
    Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media")).unwrap()),
        Some(Arc::new(FakeSource::new(fixture("player")).unwrap()) as Arc<dyn Source>),
        true,
    )
    .with_now(now)
}
fn system_with_metrics() -> Json {
    let mut data = fixture("system");
    data["network"] = json!([
        {"down_bps": 200_000, "up_bps": 20_000}, {"down_bps": 900_000, "up_bps": 60_000},
        {"down_bps": 2_500_000, "up_bps": 90_000}, {"down_bps": 1_200_000, "up_bps": 30_000},
        {"down_bps": 300_000, "up_bps": 10_000}
    ]);
    data["cpu_percent"] = json!(37.4);
    data["memory"] =
        json!({"used": 6 * 1024 * 1024 * 1024_u64, "total": 16 * 1024 * 1024 * 1024_u64});
    data["gpu"] = json!({"percent": 12.0});
    // Booted 25 h before `fixed_now`.
    data["host"] = json!({"user":"siverteh","os":"Arch Linux","kernel":"6.12.1-arch1-1",
        "uptime_s": 1, "boot_ms": fixed_now() - 90_000_000});
    data
}

#[test]
fn media_projects_the_active_player_and_extrapolates_position_while_playing() {
    let at = |now: fn() -> u64| media(now).project();
    let model = at(|| 1709193600000 + 10_000);
    assert_eq!(text(&model, "dashboard.state"), "playing");
    assert_eq!(text(&model, "dashboard.track"), "Preview track");
    assert_eq!(
        text(&model, "dashboard.track_detail"),
        "Preview artist · Preview album"
    );
    assert_eq!(text(&model, "dashboard.metric"), "0:52 / 3:00");
    assert_eq!(model["dashboard.can_next"], Value::Bool(true));
    assert_eq!(model["dashboard.can_previous"], Value::Bool(false));
    // Past the track end the position clamps to the length instead of overrunning.
    assert_eq!(
        text(&at(|| 1709193600000 + 999_000_000), "dashboard.metric"),
        "3:00 / 3:00"
    );
    // Lyrics enrichment is unchanged beside the player.
    assert_eq!(text(&model, "dashboard.heading"), "Lyrics");
    // No player topic: the page keeps its honest unavailable text.
    let bare = Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media")).unwrap()),
        None,
        true,
    )
    .project();
    assert_eq!(text(&bare, "dashboard.track"), "Playback unavailable");
    assert_eq!(text(&bare, "dashboard.state"), "unavailable");
    // A paused player does not advance.
    let mut paused = fixture("player");
    paused["players"]["items"]["player-1"]["status"] = json!("paused");
    let model = Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media")).unwrap()),
        Some(Arc::new(FakeSource::new(paused).unwrap()) as Arc<dyn Source>),
        true,
    )
    .with_now(|| 1709193600000 + 60_000)
    .project();
    assert_eq!(text(&model, "dashboard.metric"), "0:42 / 3:00");
}

#[test]
fn system_gauges_show_published_readings_and_never_invent_zero() {
    let known = page_view(Page::System, system_with_metrics()).project();
    let cards = rows(&known, "dashboard.gauges");
    assert_eq!(cards[0]["value"], json!("37%"));
    assert_eq!(cards[1]["value"], json!("38%"));
    assert_eq!(cards[1]["detail"], json!("6.0 GiB / 16.0 GiB"));
    assert_eq!(cards[2]["value"], json!("12%"));
    assert_eq!(cards.len(), 3, "the placeholder disk card is gone");
    // The frozen fixture has none of the new members: every gauge stays unknown.
    let unknown = page_view(Page::System, fixture("system")).project();
    for card in rows(&unknown, "dashboard.gauges") {
        assert_eq!(card["value"], json!("—"), "{card}");
    }
}

#[test]
fn host_card_counts_uptime_locally_and_is_absent_without_a_host_block() {
    let model = page_view(Page::System, system_with_metrics()).project();
    let card = rows(&model, "dashboard.host")
        .iter()
        .map(|r| r["text"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        card,
        [
            "siverteh",
            "Arch Linux",
            "Hyprland",
            "Up 1d 1h 0m",
            "6.12.1-arch1-1"
        ]
    );
    // Only the daemon's boot-relative reading: members are independent and optional.
    let mut data = system_with_metrics();
    data["host"] = json!({"uptime_s": 3 * 3600 + 5 * 60});
    let model = page_view(Page::System, data).project();
    let card = rows(&model, "dashboard.host");
    assert_eq!(card[0]["text"], json!("Linux"));
    assert_eq!(card[2]["text"], json!("Up 3h 5m"));
    assert_eq!(card.len(), 3);
    assert!(
        !page_view(Page::System, fixture("system"))
            .project()
            .contains_key("dashboard.host")
    );
}

fn shots_dir() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "bead-shots-view-{}-{}",
        std::process::id(),
        unique()
    ));
    std::fs::create_dir_all(root.join("nacre/shell/screenshot-thumbs")).unwrap();
    root
}
fn with_thumbs(paths: [&std::path::Path; 2]) -> Dashboard {
    let mut data = fixture("screenshots");
    data["items"]["items"]["s2"]["thumb_path"] = json!(paths[0].to_str().unwrap());
    data["items"]["items"]["s1"]["thumb_path"] = json!(paths[1].to_str().unwrap());
    page_view(Page::Screenshots, data)
}

#[test]
fn screenshot_thumbnails_are_decoded_once_refused_when_unsafe_and_released_when_hidden()
-> Result<()> {
    let root = shots_dir();
    let good = root.join("nacre/shell/screenshot-thumbs/good.png");
    image::RgbaImage::from_pixel(256, 160, image::Rgba([10, 120, 200, 255])).save(&good)?;
    let outside = root.join("outside.png");
    std::fs::copy(&good, &outside)?;
    let dashboard = with_thumbs([&good, &outside]);
    let model = dashboard.project();
    let listed = rows(&model, "dashboard.rows");
    assert_eq!(listed[0]["thumb"], json!("shot-s2"));
    assert_eq!(listed[1]["thumb"], json!(""), "outside the daemon cache");
    assert!(
        !listed[0].to_string().contains("thumbs"),
        "no path in the view"
    );
    let held = dashboard.shots.borrow()["s2"].1.clone().unwrap();
    let decoded = image::load_from_memory(&held)?;
    assert_eq!((decoded.width(), decoded.height()), (144, 96));
    dashboard.project();
    assert!(Arc::ptr_eq(
        &held,
        &dashboard.shots.borrow()["s2"].1.clone().unwrap()
    ));
    // A symlink to a real PNG never reaches the decoder.
    let link = root.join("nacre/shell/screenshot-thumbs/link.png");
    std::os::unix::fs::symlink(&good, &link)?;
    assert_eq!(
        rows(&with_thumbs([&link, &link]).project(), "dashboard.rows")[0]["thumb"],
        json!("")
    );
    let closed = bead::core::Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))?;
    dashboard.project_with_stores(&closed);
    assert!(dashboard.shots.borrow().is_empty(), "released when hidden");
    std::fs::remove_dir_all(root).ok();
    Ok(())
}

#[test]
fn host_card_and_screenshot_thumbnails_have_deterministic_pixels() -> Result<()> {
    let root_dir = shots_dir();
    let good = root_dir.join("nacre/shell/screenshot-thumbs/good.png");
    image::RgbaImage::from_pixel(256, 160, image::Rgba([10, 120, 200, 255])).save(&good)?;
    let model = with_thumbs([&good, &root_dir.join("missing.png")]).project();
    for scale in [1., 1.5] {
        let mut h = Harness::load_with_icons(
            &root(),
            Path::new("dashboard/screenshots/main.luau"),
            &root().join("palette.json"),
            model.clone(),
            bead_test::Config {
                width: 1048,
                height: 714,
                scale,
            },
            BTreeMap::from([("shot-s2".into(), root().join("sample.png"))]),
        )?;
        h.render()?;
        let frame = h.render()?;
        let golden = root().join(format!(
            "tests/goldens/dashboard-screenshots-thumbs-{scale}.png"
        ));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-dashboard-shots-{scale}-diff")),
        )?;
    }
    std::fs::remove_dir_all(root_dir).ok();
    Ok(())
}

fn art_dir() -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("bead-art-view-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(root.join("nacre/shell/player-art")).unwrap();
    root
}
fn with_art(path: &std::path::Path) -> Dashboard {
    let mut data = fixture("player");
    data["players"]["items"]["player-1"]["art_path"] = json!(path.to_str().unwrap());
    Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media")).unwrap()),
        Some(Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>),
        true,
    )
    .with_now(fixed_now)
}

#[test]
fn album_art_is_decoded_once_held_while_visible_and_released_when_hidden() -> Result<()> {
    let root = art_dir();
    let good = root.join("nacre/shell/player-art/good.png");
    image::RgbaImage::from_pixel(300, 200, image::Rgba([10, 120, 200, 255])).save(&good)?;
    let dashboard = with_art(&good);
    assert_eq!(dashboard.project()["dashboard.art"], Value::Bool(true));
    let held = dashboard
        .art
        .borrow()
        .as_ref()
        .and_then(|(_, t)| t.clone())
        .unwrap();
    // Projecting again reuses the thumbnail instead of decoding again.
    dashboard.project();
    let again = dashboard
        .art
        .borrow()
        .as_ref()
        .and_then(|(_, t)| t.clone())
        .unwrap();
    assert!(Arc::ptr_eq(&held, &again));
    let closed = bead::core::Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))?;
    dashboard.project_with_stores(&closed);
    assert!(dashboard.art.borrow().is_none(), "released when hidden");
    // A file outside the daemon cache, or no art at all, shows the placeholder.
    let outside = root.join("outside.png");
    std::fs::copy(&good, &outside)?;
    assert_eq!(
        with_art(&outside).project()["dashboard.art"],
        Value::Bool(false)
    );
    assert_eq!(
        media(fixed_now).project()["dashboard.art"],
        Value::Bool(false)
    );
    std::fs::remove_dir_all(root).ok();
    Ok(())
}

#[test]
fn today_shows_the_mini_player_only_with_a_player_and_never_ticks() -> Result<()> {
    let today = |player: Option<Json>| {
        Dashboard::new(
            Page::Today,
            Arc::new(FakeSource::new(fixture("today")).unwrap()),
            None,
            true,
        )
        .with_now(fixed_now)
        .with_player(player.map(|d| Arc::new(FakeSource::new(d).unwrap()) as Arc<dyn Source>))
    };
    let model = today(Some(fixture("player"))).project();
    assert_eq!(text(&model, "dashboard.state"), "playing");
    assert_eq!(text(&model, "dashboard.track"), "Preview track");
    assert_eq!(
        text(&today(None).project(), "dashboard.state"),
        "unavailable"
    );
    let dashboard = today(Some(fixture("player")));
    let open = bead::core::Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))?;
    dashboard.project_with_stores(&open);
    assert!(
        dashboard.ticker.borrow().is_none(),
        "no per-second wake for the mini player"
    );
    Ok(())
}

#[test]
fn network_traffic_and_frequency_show_published_samples_only() {
    let mut data = system_with_metrics();
    data["network"] = json!([
        {"down_bps": 2 * 1024 * 1024, "up_bps": 1024},
        {"down_bps": 512 * 1024, "up_bps": 0}
    ]);
    let model = page_view(Page::System, data).project();
    assert_eq!(text(&model, "dashboard.net_rate"), "Down 512 KiB/s");
    assert_eq!(text(&model, "dashboard.net_up_rate"), "Up 0 KiB/s");
    assert_eq!(text(&model, "dashboard.net_peak"), "Peak 2.0 MiB/s");
    let down = rows(&model, "dashboard.net_down");
    assert_eq!(down.len(), 2);
    assert_eq!(down[0]["h"], json!(40));
    assert_eq!(down[1]["h"], json!(10));
    // Zero is drawn as a visible 2 px floor; no samples means no bars and no invented rate.
    assert_eq!(rows(&model, "dashboard.net_up")[1]["h"], json!(2));
    let none = page_view(Page::System, fixture("system")).project();
    assert!(rows(&none, "dashboard.net_down").is_empty());
    assert_eq!(text(&none, "dashboard.net_rate"), "—");
    let mut wifi = fixture("network");
    wifi["access_points"]["items"]["ap1"]["frequency_mhz"] = json!(5180);
    let model = page_view(Page::Network, wifi).project();
    assert_eq!(text(&model, "dashboard.detail"), "5180 MHz");
    assert_eq!(
        rows(&model, "dashboard.secondary")[0]["detail"],
        json!("Secured · 5180 MHz")
    );
    let model = page_view(Page::Network, fixture("network")).project();
    assert_eq!(text(&model, "dashboard.detail"), "");
    assert_eq!(
        rows(&model, "dashboard.secondary")[1]["detail"],
        json!("Open")
    );
}

#[test]
fn media_and_system_cards_have_deterministic_pixels() -> Result<()> {
    for (page, name, model) in [
        (
            Page::Media,
            "media-player",
            media(|| 1709193600000 + 10_000).project(),
        ),
        (
            Page::System,
            "system-gauges",
            page_view(Page::System, system_with_metrics()).project(),
        ),
    ] {
        for scale in [1., 1.5] {
            let mut h = {
                let mut h = extra_harness(page, model.clone(), scale)?;
                h.render()?;
                h
            };
            let frame = h.render()?;
            let golden = root().join(format!("tests/goldens/dashboard-{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-dashboard-{name}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}

#[test]
fn album_art_seek_and_mini_player_have_deterministic_pixels() -> Result<()> {
    let seekable = {
        let mut data = fixture("player");
        data["players"]["items"]["player-1"]["can_seek"] = json!(true);
        data
    };
    let mut media_model = Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media"))?),
        Some(Arc::new(FakeSource::new(seekable.clone())?) as Arc<dyn Source>),
        true,
    )
    .with_now(|| 1709193600000 + 10_000)
    .project();
    media_model.insert("dashboard.art".into(), Value::Bool(true));
    media_model.insert("dashboard.can_seek".into(), Value::Bool(true));
    let mut today_model = Dashboard::new(
        Page::Today,
        Arc::new(FakeSource::new(fixture("today"))?),
        None,
        true,
    )
    .with_now(fixed_now)
    .with_player(Some(Arc::new(FakeSource::new(seekable)?) as Arc<dyn Source>))
    .project();
    today_model.insert("dashboard.art".into(), Value::Bool(true));
    for (page, name, model) in [
        (Page::Media, "media-art-seek", media_model),
        (Page::Today, "today-mini-player", today_model),
    ] {
        for scale in [1., 1.5] {
            let mut h = Harness::load_with_icons(
                &root(),
                Path::new(&format!("dashboard/{}/main.luau", page.name())),
                &root().join("palette.json"),
                model.clone(),
                bead_test::Config {
                    width: 1048,
                    height: 714,
                    scale,
                },
                BTreeMap::from([("player-art".into(), root().join("sample.png"))]),
            )?;
            h.render()?;
            let frame = h.render()?;
            let golden = root().join(format!("tests/goldens/dashboard-{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-dashboard-{name}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}

// ---- daemon-backed calls ------------------------------------------------------

type Calls = mpsc::UnboundedReceiver<(String, Json)>;
fn unique() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
fn wake() -> CallWake {
    Arc::new(|| {})
}
fn ok() -> Reply {
    Reply::Ok {
        ok: json!({"changed":true}),
    }
}
/// Fake daemon advertising `methods` and `topics`, answering from `replies`.
async fn lab(
    methods: &[&str],
    topics: &[&str],
    replies: Vec<Reply>,
) -> (std::path::PathBuf, Client, Calls) {
    let dir = std::env::temp_dir().join(format!(
        "dashboard-more-{}-{}",
        std::process::id(),
        unique()
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut advertised = nacre_protocol::core_methods();
    for name in methods {
        advertised.insert(
            (*name).into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 5_000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
    }
    let welcome = Message::Welcome {
        proto: 1,
        min: 1,
        boot: "more-boot".into(),
        topics: topics.iter().map(|t| ((*t).to_string(), 1)).collect(),
        methods: advertised,
        frame_ms: None,
    };
    let (sender, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        framing::write(&mut writer, &welcome).await.unwrap();
        let mut replies = replies.into_iter();
        while let Ok(Some(message)) = reader.next().await {
            if let Message::Call { id, m, p, .. } = message {
                sender.send((m, p)).unwrap();
                let reply = replies.next().expect("unexpected extra call");
                framing::write(&mut writer, &Message::Ret { id, reply })
                    .await
                    .unwrap();
            }
        }
    });
    let client = Client::start(ClientConfig::new(socket, vec![])).unwrap();
    for _ in 0..500 {
        if client.snapshot().connected && client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (dir, client, received)
}
fn wired(page: Page, data: Json, client: &Client) -> Dashboard {
    Dashboard::new(page, Arc::new(FakeSource::new(data).unwrap()), None, false)
        .with_now(fixed_now)
        .with_controls(
            Some(Arc::new(FakeSource::new(fixture("settings")).unwrap()) as Arc<dyn Source>),
            Some(controls::Controls::new(client.clone())),
            Some(client.clone()),
        )
}
async fn settle(dashboard: &Dashboard) -> BTreeMap<String, Value> {
    for _ in 0..500 {
        let model = dashboard.project();
        if model["dashboard.busy"] == Value::Bool(false) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("call never completed");
}

#[tokio::test(flavor = "current_thread")]
async fn controls_stay_disabled_and_send_nothing_until_the_method_is_advertised() {
    let (dir, client, mut calls) = lab(&[], &[], vec![]).await;
    for (page, op) in [
        (Page::Focus, "focus.start:25"),
        (Page::Clipboard, "clip.copy:c12"),
        (Page::Screenshots, "shot.open:s2"),
        (Page::AiUsage, "usage.refresh"),
        (Page::Customize, "tab.toggle:brain"),
    ] {
        let dashboard = wired(page, fixture(page.name()), &client);
        let model = dashboard.project();
        assert_eq!(model["dashboard.control"], Value::Bool(false), "{page:?}");
        assert_eq!(model["dashboard.customize"], Value::Bool(false));
        dashboard.act(op, wake());
        assert!(settle(&dashboard).await["dashboard.busy"] == Value::Bool(false));
    }
    // The extras are offered only when the daemon advertises their topic.
    let tabs = rows(
        &wired(Page::Today, fixture("today"), &client).project(),
        "dashboard.tabs",
    );
    assert_eq!(tabs.len(), 4);
    assert!(calls.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn advertised_methods_send_exact_params_and_unknown_ids_are_never_sent() {
    let methods = [
        "focus.start",
        "focus.stop",
        "clipboard.copy",
        "clipboard.clear",
        "screenshots.open",
        "ai.usage.refresh",
    ];
    let (dir, client, mut calls) = lab(
        &methods,
        &["dashboard.focus"],
        vec![ok(), ok(), ok(), ok(), ok(), ok()],
    )
    .await;
    let run = |page: Page, op: &'static str| {
        let dashboard = wired(page, fixture(page.name()), &client);
        async move {
            dashboard.act(op, wake());
            settle(&dashboard).await
        }
    };
    let model = run(Page::Focus, "focus.start:25").await;
    assert_eq!(
        calls.recv().await.unwrap(),
        ("focus.start".into(), json!({"minutes":25}))
    );
    assert_eq!(text(&model, "dashboard.message"), "");
    run(Page::Focus, "focus.start:181").await;
    run(Page::Focus, "focus.start:0").await;
    run(Page::Focus, "focus.start:abc").await;
    run(Page::Focus, "focus.stop").await;
    assert_eq!(
        calls.recv().await.unwrap(),
        ("focus.stop".into(), json!({}))
    );
    run(Page::Clipboard, "clip.copy:c12").await;
    assert_eq!(
        calls.recv().await.unwrap(),
        ("clipboard.copy".into(), json!({"id":"c12"}))
    );
    run(Page::Clipboard, "clip.copy:c99").await; // not in the topic
    run(Page::Clipboard, "clip.clear").await; // unconfirmed
    run(Page::Clipboard, "focus.stop").await; // wrong page
    run(Page::Screenshots, "shot.open:s1").await;
    assert_eq!(
        calls.recv().await.unwrap(),
        ("screenshots.open".into(), json!({"id":"s1"}))
    );
    run(Page::AiUsage, "usage.refresh").await;
    assert_eq!(
        calls.recv().await.unwrap(),
        ("ai.usage.refresh".into(), json!({}))
    );
    assert!(calls.try_recv().is_err(), "rejected input sent nothing");
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn player_controls_resolve_the_active_player_and_obey_can_flags() {
    let (dir, client, mut calls) = lab(
        &["player.play_pause", "player.next", "player.previous"],
        &[],
        vec![ok(), ok()],
    )
    .await;
    let dashboard = Dashboard::new(
        Page::Media,
        Arc::new(FakeSource::new(fixture("media")).unwrap()),
        Some(Arc::new(FakeSource::new(fixture("player")).unwrap()) as Arc<dyn Source>),
        false,
    )
    .with_controls(
        None,
        Some(controls::Controls::new(client.clone())),
        Some(client.clone()),
    );
    assert_eq!(dashboard.project()["dashboard.control"], Value::Bool(true));
    dashboard.act("player.previous", wake()); // can_previous is false
    dashboard.act("player.seek:5", wake()); // not a published control here
    dashboard.act("player.play_pause", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        ("player.play_pause".into(), json!({"player":"player-1"}))
    );
    settle(&dashboard).await;
    dashboard.act("player.next", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        ("player.next".into(), json!({"player":"player-1"}))
    );
    settle(&dashboard).await;
    assert!(calls.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn seek_jumps_from_the_extrapolated_position_clamps_and_needs_can_seek() {
    let (dir, client, mut calls) = lab(&["player.seek"], &[], vec![ok(), ok(), ok(), ok()]).await;
    let view = |edit: &dyn Fn(&mut Json), now: fn() -> u64| {
        let mut data = fixture("player");
        edit(&mut data["players"]["items"]["player-1"]);
        Dashboard::new(
            Page::Media,
            Arc::new(FakeSource::new(fixture("media")).unwrap()),
            Some(Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>),
            false,
        )
        .with_now(now)
        .with_controls(
            None,
            Some(controls::Controls::new(client.clone())),
            Some(client.clone()),
        )
    };
    let seekable = |p: &mut Json| p["can_seek"] = json!(true);
    // Playing: 42 s at the reading plus 10 s elapsed, jump forward 10 s.
    let d = view(&seekable, || 1709193600000 + 10_000);
    assert_eq!(d.project()["dashboard.can_seek"], Value::Bool(true));
    d.act("player.seek:10", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        (
            "player.seek".into(),
            json!({"player":"player-1","position_ms":62_000})
        )
    );
    settle(&d).await;
    // Paused near the start: a jump back clamps to zero; near the end it clamps to the length.
    let d = view(
        &|p| {
            seekable(p);
            p["status"] = json!("paused");
            p["position_ms"] = json!(3000);
        },
        fixed_now,
    );
    d.act("player.seek:-10", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        (
            "player.seek".into(),
            json!({"player":"player-1","position_ms":0})
        )
    );
    settle(&d).await;
    let d = view(
        &|p| {
            seekable(p);
            p["position_ms"] = json!(175_000);
        },
        fixed_now,
    );
    d.act("player.seek:10", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        (
            "player.seek".into(),
            json!({"player":"player-1","position_ms":180_000})
        )
    );
    settle(&d).await;
    // can_seek false, an oversized step and a non-number send nothing.
    let d = view(&|_| {}, fixed_now);
    assert_eq!(d.project()["dashboard.can_seek"], Value::Bool(false));
    d.act("player.seek:10", wake());
    let d = view(&seekable, fixed_now);
    d.act("player.seek:61", wake());
    d.act("player.seek:x", wake());
    assert!(calls.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_the_clipboard_takes_two_steps_and_failures_show_a_fixed_message() {
    let failure = Reply::Err {
        err: Failure::new(ErrorCode::Io, "secret path /home/x", false),
    };
    let (dir, client, mut calls) =
        lab(&["clipboard.clear", "clipboard.copy"], &[], vec![failure]).await;
    let dashboard = wired(Page::Clipboard, fixture("clipboard"), &client);
    dashboard.act("clip.ask", wake());
    assert_eq!(dashboard.project()["dashboard.confirm"], Value::Bool(true));
    dashboard.act("clip.cancel", wake());
    assert_eq!(dashboard.project()["dashboard.confirm"], Value::Bool(false));
    dashboard.act("clip.ask", wake());
    dashboard.act("clip.clear", wake());
    assert_eq!(calls.recv().await.unwrap().0, "clipboard.clear");
    let model = settle(&dashboard).await;
    assert_eq!(text(&model, "dashboard.message"), "Request failed");
    assert_eq!(model["dashboard.confirm"], Value::Bool(false));
    dashboard.act("clip.clear", wake());
    assert!(calls.try_recv().is_err(), "confirmation is consumed");
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn customization_writes_the_full_value_with_the_current_revision() {
    let stale = Reply::Err {
        err: Failure::new(ErrorCode::StaleRevision, "Settings changed", true),
    };
    let (dir, client, mut calls) = lab(
        &["settings.set"],
        &[
            "settings.store.dashboard-extras",
            "dashboard.focus",
            "clipboard",
        ],
        vec![ok(), ok(), stale],
    )
    .await;
    let dashboard = wired(Page::Customize, fixture("settings"), &client);
    let model = dashboard.project();
    assert_eq!(model["dashboard.control"], Value::Bool(true));
    // Enabled is rewritten in registry order (focus, clipboard are on): toggling brain adds it.
    dashboard.act("tab.toggle:brain", wake());
    let (method, params) = calls.recv().await.unwrap();
    assert_eq!(method, "settings.set");
    assert_eq!(
        params,
        json!({"store":"dashboard-extras","key":["enabled"],"value":["focus","clipboard","brain"],"rev":3})
    );
    settle(&dashboard).await;
    // Toggling an enabled page removes only that page.
    dashboard.act("tab.toggle:focus", wake());
    assert_eq!(calls.recv().await.unwrap().1["value"], json!(["clipboard"]));
    settle(&dashboard).await;
    // Moving down swaps with the next page and sends the whole order.
    dashboard.act("tab.down:focus", wake());
    let (_, params) = calls.recv().await.unwrap();
    assert_eq!(params["key"], json!(["order"]));
    assert_eq!(params["value"][0], json!("notifications"));
    assert_eq!(params["value"][1], json!("focus"));
    assert_eq!(params["value"].as_array().unwrap().len(), 8);
    let model = settle(&dashboard).await;
    assert_eq!(
        text(&model, "dashboard.message"),
        "Changed elsewhere; try again"
    );
    // Edges and unknown pages send nothing.
    dashboard.act("tab.up:focus", wake());
    dashboard.act("tab.down:brain", wake());
    dashboard.act("tab.toggle:bogus", wake());
    dashboard.act("tab.toggle:today", wake());
    assert!(calls.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn a_store_read_error_blocks_writes_and_advertised_topics_offer_enabled_tabs() {
    let (dir, client, mut calls) = lab(
        &["settings.set"],
        &[
            "settings.store.dashboard-extras",
            "dashboard.focus",
            "clipboard",
            "brain.recent",
        ],
        vec![],
    )
    .await;
    let mut broken = fixture("settings");
    broken["error"] = json!({"file":"nacre/dashboard-extras.json","line":1,"message":"bad"});
    let dashboard = wired(Page::Customize, broken, &client);
    dashboard.act("tab.toggle:brain", wake());
    assert_eq!(
        text(&dashboard.project(), "dashboard.message"),
        "Settings are unavailable"
    );
    assert!(calls.try_recv().is_err());
    // Today lists focus and clipboard (enabled and advertised) and the customize link.
    let today = wired(Page::Today, fixture("today"), &client).project();
    let ids: Vec<_> = rows(&today, "dashboard.tabs")
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        ["today", "weather", "media", "system", "focus", "clipboard"]
    );
    assert_eq!(today["dashboard.customize"], Value::Bool(true));
    std::fs::remove_dir_all(dir).ok();
}

/// B0035: the effective preferences Source, driven by fake `settings.store.*` topics.
fn live_prefs() -> (
    Arc<FakeSource>,
    Arc<FakeSource>,
    Arc<dyn Source>,
    crate::prefs::Service,
) {
    let store = |id: &str| {
        Arc::new(
            FakeSource::new(
                json!({"store": id, "rev": 1, "exists": true, "data": {}, "dirty": false}),
            )
            .unwrap(),
        )
    };
    let (desktop, dashboard) = (store("desktop"), store("dashboard"));
    let (service, source) = crate::prefs::Service::start(
        desktop.clone(),
        dashboard.clone(),
        Arc::new(|_: &crate::prefs::Prefs| {}),
    )
    .unwrap();
    (desktop, dashboard, source, service)
}

#[test]
fn weather_follows_the_fahrenheit_preference_live() {
    let (desktop, _dashboard, prefs, _service) = live_prefs();
    let mut data = fixture("weather");
    data["current"]["temperature"] = json!(10.0);
    let weather = |prefs: Option<Arc<dyn Source>>| {
        Dashboard::new(
            Page::Weather,
            Arc::new(FakeSource::new(data.clone()).unwrap()),
            None,
            true,
        )
        .with_prefs(prefs)
        .project()
    };
    // 10 C; the store is empty, so Celsius like Nacre's default.
    let celsius = weather(Some(prefs.clone()));
    assert_eq!(text(&celsius, "dashboard.metric"), "10°");
    desktop
        .publish(json!({"store":"desktop","rev":2,"exists":true,"dirty":false,"data":{"weatherFahrenheit":true}}))
        .unwrap();
    let fahrenheit = weather(Some(prefs));
    assert_eq!(text(&fahrenheit, "dashboard.metric"), "50°");
    assert_ne!(
        serde_json::to_string(&rows(&fahrenheit, "dashboard.rows")).unwrap(),
        serde_json::to_string(&rows(&celsius, "dashboard.rows")).unwrap()
    );
    // Without a preferences source (preview) the page stays Celsius.
    assert_eq!(text(&weather(None), "dashboard.metric"), "10°");
}

#[test]
fn dashboard_wakes_on_a_preference_change_and_core_tabs_follow_dashboard_tabs() {
    let (_desktop, dashboard_store, prefs, _service) = live_prefs();
    let dashboard = Dashboard::new(
        Page::Today,
        Arc::new(ReadOnly::unavailable().unwrap()),
        None,
        true,
    )
    .with_prefs(Some(prefs));
    let ids = |d: &Dashboard| -> Vec<String> {
        rows(&d.project(), "dashboard.tabs")
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    // No `tabs` key: Nacre's registry order (media before weather).
    assert_eq!(ids(&dashboard), ["today", "media", "weather", "system"]);
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    let _subscriptions = dashboard
        .subscribe(Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }))
        .unwrap();
    let before = woken.load(std::sync::atomic::Ordering::SeqCst);
    dashboard_store
        .publish(json!({"store":"dashboard","rev":2,"exists":true,"dirty":false,"data":{"tabs":["system","today","bogus"]}}))
        .unwrap();
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > before);
    assert_eq!(ids(&dashboard), ["system", "today"]);
    // The open page is still shown when the owner hid it.
    let weather = Dashboard::new(
        Page::Weather,
        Arc::new(ReadOnly::unavailable().unwrap()),
        None,
        true,
    )
    .with_prefs(dashboard.prefs.clone());
    assert_eq!(ids(&weather), ["system", "today", "weather"]);
}
