// SPDX-License-Identifier: MIT
//! MPRIS discovery, signal updates, owner loss and actions on a private bus.
use bead_services::media::{Media, MediaAction};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const NAME: &str = "org.mpris.MediaPlayer2.beadtest";
const NAME2: &str = "org.mpris.MediaPlayer2.beadtest2";
const PATH: &str = "/org/mpris/MediaPlayer2";
const TRACK: &str = "/org/mpris/MediaPlayer2/track/1";

#[derive(Default)]
struct State {
    status: String,
    title: String,
    calls: Vec<String>,
    seek: Option<(String, i64)>,
    can_seek: bool,
    track: String,
}
struct Fake(Arc<Mutex<State>>);
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Fake {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.0.lock().unwrap().status.clone()
    }
    #[zbus(property)]
    fn set_playback_status(&self, value: String) {
        self.0.lock().unwrap().status = value;
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "xesam:title".into(),
            OwnedValue::try_from(Value::from(self.0.lock().unwrap().title.clone())).unwrap(),
        );
        metadata.insert(
            "xesam:artist".into(),
            OwnedValue::try_from(Value::from(vec!["Artist A", "Artist B"])).unwrap(),
        );
        metadata.insert(
            "xesam:album".into(),
            OwnedValue::try_from(Value::from("Private Album")).unwrap(),
        );
        metadata.insert(
            "mpris:length".into(),
            OwnedValue::try_from(Value::from(90_000_000_i64)).unwrap(),
        );
        metadata.insert(
            "mpris:artUrl".into(),
            OwnedValue::try_from(Value::from("file:///cover.png")).unwrap(),
        );
        let track = self.0.lock().unwrap().track.clone();
        let path = OwnedObjectPath::try_from(track).unwrap();
        metadata.insert("mpris:trackid".into(), OwnedValue::from(path.as_ref()));
        metadata
    }
    #[zbus(property)]
    fn set_metadata(&self, value: HashMap<String, OwnedValue>) {
        if let Some(title) = value
            .get("xesam:title")
            .and_then(|v| <&str>::try_from(v).ok())
        {
            self.0.lock().unwrap().title = title.into();
        }
    }
    #[zbus(property)]
    fn can_play_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.lock().unwrap().can_seek
    }
    #[zbus(property)]
    fn set_can_seek(&self, value: bool) {
        self.0.lock().unwrap().can_seek = value;
    }
    fn play_pause(&self) {
        self.0.lock().unwrap().calls.push("PlayPause".into());
    }
    fn next(&self) {
        self.0.lock().unwrap().calls.push("Next".into());
    }
    fn previous(&self) {
        self.0.lock().unwrap().calls.push("Previous".into());
    }
    fn set_position(&self, track: OwnedObjectPath, position: i64) {
        self.0.lock().unwrap().seek = Some((track.to_string(), position));
    }
}
struct Root(&'static str);
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        self.0.into()
    }
}

async fn wait_for(media: &Media, predicate: impl Fn(&bead_services::media::MediaSnapshot) -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let s = media.snapshot();
            if predicate(&s) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("media state timeout");
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn mpris_discovery_updates_actions_and_owner_loss() {
    assert!(std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS").is_some());
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    let first_state = Arc::new(Mutex::new(State {
        status: "Paused".into(),
        title: "Private Track".into(),
        can_seek: true,
        track: TRACK.into(),
        ..Default::default()
    }));
    let first = Connection::session().await.unwrap();
    first.request_name(NAME).await.unwrap();
    first
        .object_server()
        .at(PATH, Root("Private Fake Player"))
        .await
        .unwrap();
    first
        .object_server()
        .at(PATH, Fake(first_state.clone()))
        .await
        .unwrap();
    let client = Connection::session().await.unwrap();
    let media = Media::start(client);
    wait_for(&media, |s| {
        s.players
            .iter()
            .any(|p| p.id == NAME && p.title == "Private Track")
    })
    .await;
    assert_eq!(media.snapshot().players[0].artist, "Artist A, Artist B");
    assert_eq!(media.snapshot().players[0].length_ms, 90_000);
    let mut changed_metadata: HashMap<String, OwnedValue> = HashMap::new();
    changed_metadata.insert(
        "xesam:title".into(),
        OwnedValue::try_from(Value::from("Updated Track")).unwrap(),
    );
    let player_props = zbus::Proxy::new(&first, NAME, PATH, "org.mpris.MediaPlayer2.Player")
        .await
        .unwrap();
    player_props
        .set_property("Metadata", changed_metadata)
        .await
        .unwrap();
    wait_for(&media, |s| {
        s.players
            .first()
            .is_some_and(|p| p.title == "Updated Track")
    })
    .await;
    let props = player_props;
    props
        .set_property("PlaybackStatus", "Playing")
        .await
        .unwrap();
    wait_for(&media, |s| {
        s.players.first().is_some_and(|p| p.status == "Playing")
    })
    .await;
    let second_state = Arc::new(Mutex::new(State {
        status: "Playing".into(),
        track: TRACK.into(),
        ..Default::default()
    }));
    let second = Connection::session().await.unwrap();
    second.request_name(NAME2).await.unwrap();
    second
        .object_server()
        .at(PATH, Root("Second Player"))
        .await
        .unwrap();
    second
        .object_server()
        .at(PATH, Fake(second_state.clone()))
        .await
        .unwrap();
    wait_for(&media, |s| s.active.as_deref() == Some(NAME2)).await;
    let second_props = zbus::Proxy::new(&second, NAME2, PATH, "org.mpris.MediaPlayer2.Player")
        .await
        .unwrap();
    second_props
        .set_property("PlaybackStatus", "Paused")
        .await
        .unwrap();
    wait_for(&media, |s| s.active.as_deref() == Some(NAME)).await;
    props
        .set_property("PlaybackStatus", "Paused")
        .await
        .unwrap();
    wait_for(&media, |s| s.active.as_deref() == Some(NAME)).await;
    second_props
        .set_property("PlaybackStatus", "Playing")
        .await
        .unwrap();
    wait_for(&media, |s| s.active.as_deref() == Some(NAME2)).await;
    for action in [
        MediaAction::PlayPause(NAME.into()),
        MediaAction::Next(NAME.into()),
        MediaAction::Previous(NAME.into()),
        MediaAction::Seek(NAME.into(), 12_000),
    ] {
        let media = media.clone();
        tokio::task::spawn_blocking(move || media.action(action))
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        first_state.lock().unwrap().calls,
        ["PlayPause", "Next", "Previous"]
    );
    assert_eq!(
        first_state.lock().unwrap().seek.as_ref().unwrap(),
        &(TRACK.into(), 12_000_000)
    );
    first_state.lock().unwrap().track = "/org/mpris/MediaPlayer2/track/2".into();
    let media_for_action = media.clone();
    assert!(
        tokio::task::spawn_blocking(
            move || media_for_action.action(MediaAction::Seek(NAME.into(), 1))
        )
        .await
        .unwrap()
        .unwrap_err()
        .contains("track changed")
    );
    let changed = zbus::Proxy::new(&first, NAME, PATH, "org.mpris.MediaPlayer2.Player")
        .await
        .unwrap();
    changed.set_property("CanSeek", false).await.unwrap();
    wait_for(&media, |s| s.players.first().is_some_and(|p| !p.can_seek)).await;
    let media_for_action = media.clone();
    assert!(
        tokio::task::spawn_blocking(
            move || media_for_action.action(MediaAction::Seek(NAME.into(), 1))
        )
        .await
        .unwrap()
        .unwrap_err()
        .contains("cannot seek")
    );
    first.release_name(NAME).await.unwrap();
    wait_for(&media, |s| s.players.len() == 1 && s.players[0].id == NAME2).await;
    assert_eq!(media.snapshot().active.as_deref(), Some(NAME2));
}
