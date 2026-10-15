// SPDX-License-Identifier: MIT
use bead_core::{
    Stores, Value,
    source::{self, Applied, Callback, Error, Mirror, Patch, Publisher, Snapshot, Source},
};
use bead_test::{Config, Harness, Result, source::FakeSource};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
fn snapshot(epoch: &str, revision: u64, data: Json) -> Arc<Snapshot> {
    Arc::new(Snapshot {
        epoch: epoch.into(),
        revision,
        data: Arc::new(data),
        connected: true,
    })
}
fn title(data: &Json) -> source::Result<BTreeMap<String, Value>> {
    Ok(BTreeMap::from([(
        "title".into(),
        Value::Text(
            data["title"]
                .as_str()
                .ok_or_else(|| Error::new("validation", "title must be text", false))?
                .into(),
        ),
    )]))
}
#[test]
fn snapshot_patch_coalescing_gap_reconnect_and_transactional_projection() -> Result<()> {
    let now = Instant::now();
    let mut mirror = Mirror::new(now);
    let mut stores =
        Stores::default().reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))?;
    mirror.project(
        &mut stores,
        snapshot(
            "one",
            0,
            json!({"title":"Ready","items":{"a":{"v":1}},"order":["a"]}),
        ),
        title,
        now,
    )?;
    assert_eq!(stores.get("panel.open"), Some(&Value::Bool(true)));
    let patch = Patch {
        epoch: "one".into(),
        base: 0,
        revision: 3,
        data: json!({"title":"Changed","items":{"a":null,"b":{"v":2}},"order":["b"]}),
    };
    assert_eq!(
        mirror.project_patch(&mut stores, &patch, title, now)?,
        Applied::Updated
    );
    assert_eq!(stores.get("title"), Some(&Value::Text("Changed".into())));
    assert!(
        mirror.snapshot().ok_or("snapshot")?.data["items"]
            .get("a")
            .is_none()
    );
    let before = mirror.snapshot().ok_or("snapshot")?.data.clone();
    assert!(
        mirror
            .project_patch(
                &mut stores,
                &Patch {
                    epoch: "one".into(),
                    base: 3,
                    revision: 4,
                    data: json!({"title":9})
                },
                title,
                now
            )
            .is_err()
    );
    assert_eq!(mirror.snapshot().ok_or("snapshot")?.data, before);
    assert_eq!(stores.get("title"), Some(&Value::Text("Changed".into())));
    assert_eq!(
        mirror.patch("one", 1, 5, &json!({"title":"Wrong base"}), now)?,
        Applied::Resync
    );
    assert_eq!(mirror.snapshot().ok_or("snapshot")?.revision, 3);
    assert!(mirror.needs_snapshot());
    assert_eq!(
        mirror.patch("one", 3, 6, &json!({"title":"Wait for snapshot"}), now)?,
        Applied::Resync
    );
    mirror.accept(snapshot("one", 6, json!({"title":"Resynced"})), now)?;
    assert!(!mirror.stale());
    mirror.disconnected(now);
    assert!(!mirror.reconnecting(now + Duration::from_millis(1999)));
    assert!(mirror.reconnecting(now + Duration::from_secs(2)));
    assert!(mirror.reconnected("two", now)?);
    assert_eq!(
        mirror.snapshot().ok_or("old retained")?.data["title"],
        "Resynced"
    );
    mirror.accept(
        snapshot("two", 0, json!({"title":"New boot","optional":true})),
        now,
    )?;
    assert_eq!(
        mirror.accept(snapshot("one", 999, json!({"title":"Old reader"})), now)?,
        Applied::Ignored
    );
    assert_eq!(
        mirror.snapshot().ok_or("new retained")?.data["title"],
        "New boot"
    );
    Ok(())
}
#[test]
fn fake_source_drives_same_luau_bindings_and_keeps_last_picture_during_disconnect() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut ui = Harness::load(
        &root,
        Path::new("source-ui/main.luau"),
        &root.join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 384,
            height: 164,
            scale: 1.,
        },
    )?;
    let source = FakeSource::new(json!({"title":"Ready"}))?;
    let now = Instant::now();
    let mut mirror = Mirror::new(now);
    mirror.accept(source.snapshot(), now)?;
    ui.update(mirror.values("transport", title, now)?)?;
    let frame = ui.render()?;
    source.disconnect()?;
    mirror.accept(source.snapshot(), now)?;
    ui.update(mirror.values("transport", title, now + Duration::from_secs(1))?)?;
    assert!(
        ui.render()?.pixels == frame.pixels,
        "last snapshot should remain visible before reconnect mark"
    );
    ui.update(mirror.values("transport", title, now + Duration::from_secs(2))?)?;
    assert!(
        ui.render()?.pixels != frame.pixels,
        "reconnecting mark should appear after two seconds"
    );
    let fresh = FakeSource::new(json!({"title":"Ready"}))?;
    mirror.reconnected(&fresh.snapshot().epoch, now)?;
    mirror.accept(fresh.snapshot(), now)?;
    ui.update(mirror.values("transport", title, now)?)?;
    assert!(ui.render()?.pixels == frame.pixels);
    Ok(())
}
#[test]
fn source_deadline_errors_and_reconnect_do_not_replay_mutations() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let source = FakeSource::new(json!({"title":"Before"}))?;
        source.delay(Duration::from_millis(50));
        let result = source
            .call(
                "fake.set".into(),
                json!({"title":"Side effect already happened"}),
                Duration::from_millis(5),
            )
            .await;
        assert_eq!(result.err().ok_or("missing timeout")?.code, "timeout");
        assert_eq!(
            source.snapshot().data["title"],
            "Side effect already happened"
        );
        assert_eq!(source.calls(), 1);
        source.reconnect().await?;
        assert_eq!(source.calls(), 1);
        assert_eq!(
            source
                .call("fake.set".into(), json!({}), Duration::ZERO)
                .await
                .err()
                .ok_or("deadline")?
                .code,
            "validation"
        );
        assert_eq!(source.calls(), 1);
        source.delay(Duration::ZERO);
        assert_eq!(
            source
                .call("missing".into(), json!({}), Duration::from_secs(1))
                .await
                .err()
                .ok_or("unknown")?
                .code,
            "unknown_method"
        );
        source.stop().await?;
        assert!(!source.snapshot().connected);
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}
#[test]
fn bounded_publication_subscription_lifetimes_and_rfc7396_cases() -> Result<()> {
    let publisher = Publisher::new("one".into(), json!({"v":0}))?;
    let calls = Arc::new(AtomicU64::new(0));
    let callback: Callback = {
        let count = calls.clone();
        Arc::new(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        })
    };
    let mut subscriptions = Vec::new();
    for _ in 0..64 {
        subscriptions.push(publisher.subscribe(callback.clone())?);
    }
    assert!(publisher.subscribe(callback).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 64);
    publisher.publish(json!({"v":1}))?;
    assert_eq!(calls.load(Ordering::Relaxed), 128);
    drop(subscriptions);
    publisher.publish(json!({"v":2}))?;
    assert_eq!(calls.load(Ordering::Relaxed), 128);
    let before = publisher.snapshot();
    publisher.disconnect()?;
    assert!(
        Arc::ptr_eq(&before.data, &publisher.snapshot().data),
        "disconnect must share the last payload"
    );
    let mut value = json!({"a":{"x":1},"list":[1,2],"keep":true});
    source::merge_patch(&mut value, &json!({"a":{"x":null,"z":2},"list":[3]}))?;
    assert_eq!(value, json!({"a":{"z":2},"list":[3],"keep":true}));
    source::merge_patch(&mut value, &json!(7))?;
    assert_eq!(value, json!(7));
    source::merge_patch(&mut value, &json!({"a":null}))?;
    assert_eq!(value, json!({}));
    let mut deep = json!(0);
    for _ in 0..66 {
        deep = json!({"a":deep});
    }
    let old = value.clone();
    assert!(source::merge_patch(&mut value, &deep).is_err());
    assert_eq!(value, old);
    assert!(source::bounded(&json!("x".repeat(8 * 1024 * 1024))).is_err());
    assert!(source::bounded(&Json::Array(vec![Json::Null; 65537])).is_err());
    let mut retry = source::Reconnect::default();
    let delays = (0..8)
        .map(|_| retry.delay().as_millis())
        .collect::<Vec<_>>();
    assert_eq!(delays, vec![50, 100, 200, 400, 800, 1600, 2000, 2000]);
    retry.reset();
    assert_eq!(retry.delay(), Duration::from_millis(50));
    Ok(())
}
#[test]
fn origin_revisions_prevent_concurrent_callback_rollback() -> Result<()> {
    let publisher = Publisher::at("native".into(), 0, json!({"value":0}))?;
    publisher.publish_revision(2, json!({"value":2}))?;
    publisher.publish_revision(1, json!({"value":1}))?;
    assert_eq!(publisher.snapshot().revision, 2);
    assert_eq!(publisher.snapshot().data["value"], 2);
    assert!(publisher.publish_revision(2, json!({"value":0})).is_err());
    Ok(())
}

#[test]
fn publisher_shares_admitted_immutable_payload() {
    use std::sync::Arc;
    let publisher =
        bead_core::source::Publisher::new("bridge".into(), serde_json::json!({})).unwrap();
    let data = Arc::new(serde_json::json!({"items":{"one":{"label":"one"}},"order":["one"]}));
    publisher.publish_shared(data.clone()).unwrap();
    assert!(Arc::ptr_eq(&publisher.snapshot().data, &data));
    assert!(
        publisher
            .publish_shared(Arc::new(serde_json::json!([])))
            .is_err()
    );
    assert!(Arc::ptr_eq(&publisher.snapshot().data, &data));
}
