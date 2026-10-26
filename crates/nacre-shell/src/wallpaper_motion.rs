// SPDX-License-Identifier: GPL-3.0-only
//! B0046: motion (video) wallpaper control. The decoder lives in the separate
//! `bead-wallpaper-video` process; this module is the Nacre edge that decides what it plays
//! and when it may run. Nacre knowledge is confined here: the `wallpaper.active` motion member
//! (see `wallpaper::parse_motion`) and the `wallpaper-picker` settings (`motionMode`,
//! `paused`, `pauseCovered`). The generic pieces (policy, protocol, supervisor) are in
//! `bead-wallpaper`; the renderer binary has no Nacre ids.
use bead::{
    backends::{Backend, Snapshot as Windows},
    core::source::Source,
    services::{Desktop, Snapshot as Services},
};
use bead_wallpaper::{
    policy::{Config, Group, Inputs},
    protocol::{Command, Decoder, Event},
    supervisor,
};
use serde_json::Value as Json;
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

/// Pseudo-output used when the compositor adapter lists none: the renderer treats `*` as
/// every output.
const ANY: &str = "*";
const RENDERER: &str = "bead-wallpaper-video";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Full,
    Battery,
    Still,
}
/// `wallpaper-picker` keys with Nacre's defaults; wrong types fall back like the QML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefs {
    mode: Mode,
    paused: bool,
    pause_covered: bool,
}
impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: Mode::Full,
            paused: false,
            pause_covered: true,
        }
    }
}
impl Prefs {
    /// `store` is the published `SettingsStore` document.
    fn decode(store: &Json) -> Self {
        let data = store.get("data").unwrap_or(&Json::Null);
        Self {
            mode: match data.get("motionMode").and_then(Json::as_str) {
                Some("battery") => Mode::Battery,
                Some("still") => Mode::Still,
                _ => Mode::Full,
            },
            paused: data.get("paused").and_then(Json::as_bool) == Some(true),
            pause_covered: data.get("pauseCovered").and_then(Json::as_bool) != Some(false),
        }
    }
    fn policy(&self) -> Config {
        Config {
            pause_on_battery: self.mode == Mode::Battery,
            pause_when_covered: self.pause_covered,
            ..Config::default()
        }
    }
}

/// QML `NacreWallpaperScene`: an output is covered when a window of its active workspace is
/// fullscreen or (with `pauseCovered`) not floating; fullscreen also hides it entirely.
/// A locked or sleeping session hides every output.
fn inputs(windows: &Windows, services: &Services, prefs: Prefs) -> BTreeMap<String, Inputs> {
    let base = Inputs {
        on_battery: services.battery.on_battery,
        user_paused: prefs.paused,
        stopped: prefs.mode == Mode::Still,
        hidden: services.session.locked || services.session.sleeping,
        ..Inputs::default()
    };
    if windows.outputs.is_empty() {
        return BTreeMap::from([(ANY.to_owned(), base)]);
    }
    windows
        .outputs
        .iter()
        .map(|output| {
            let active: Vec<_> = windows
                .workspaces
                .iter()
                .filter(|w| {
                    w.active && (w.output.as_ref() == Some(&output.id) || w.output.is_none())
                })
                .map(|w| &w.id)
                .collect();
            let on_output = windows.windows.iter().filter(|w| {
                w.workspace.as_ref().is_some_and(|ws| active.contains(&ws))
                    && w.output.as_ref().is_none_or(|o| *o == output.id)
            });
            let (mut covered, mut fullscreen) = (false, false);
            for window in on_output {
                let full = window.fullscreen == Some(true);
                fullscreen |= full;
                covered |= full || !window.floating;
            }
            let mut input = base;
            input.hidden |= fullscreen;
            input.covered = covered;
            (output.name.clone(), input)
        })
        .collect()
}

enum Msg {
    Poke,
    Stop,
}
pub(crate) struct Deps {
    pub wallpaper: Arc<crate::wallpaper::Consumer>,
    /// `settings.store.wallpaper-picker`; absent means Nacre's defaults.
    pub picker: Option<Arc<dyn Source>>,
    pub backend: Option<Arc<dyn Backend>>,
    pub desktop: Option<Arc<Desktop>>,
}
pub(crate) struct Controller {
    stop: mpsc::Sender<Msg>,
    thread: Option<std::thread::JoinHandle<()>>,
    _subscriptions: Vec<Box<dyn std::any::Any + Send>>,
}
impl Controller {
    /// `None` when the renderer binary is not installed next to the shell.
    pub(crate) fn start(deps: Deps) -> Option<Self> {
        let renderer = std::env::current_exe().ok()?.with_file_name(RENDERER);
        if !renderer.is_file() {
            eprintln!("WALLPAPER_VIDEO_DISABLED code=renderer_missing");
            return None;
        }
        Self::start_with(deps, move || {
            let mut command = std::process::Command::new(&renderer);
            command.arg("--hwdec=vaapi");
            command
        })
    }
    fn start_with(
        deps: Deps,
        renderer: impl Fn() -> std::process::Command + Send + 'static,
    ) -> Option<Self> {
        let (sender, receiver) = mpsc::channel();
        let poke = {
            let sender = sender.clone();
            Arc::new(move || {
                let _ = sender.send(Msg::Poke);
            }) as Arc<dyn Fn() + Send + Sync>
        };
        let mut subscriptions: Vec<Box<dyn std::any::Any + Send>> = Vec::new();
        if let Some(watched) = deps.wallpaper.watch_motion(poke.clone()) {
            for sub in watched {
                subscriptions.push(Box::new(sub));
            }
        }
        if let Some(picker) = &deps.picker {
            let poke = poke.clone();
            if let Ok(sub) = picker.subscribe(Arc::new(move |_| poke())) {
                subscriptions.push(Box::new(sub));
            }
        }
        if let Some(backend) = &deps.backend {
            let poke = poke.clone();
            subscriptions.push(Box::new(backend.subscribe(Box::new(move |_| poke()))));
        }
        if let Some(desktop) = &deps.desktop {
            let poke = poke.clone();
            subscriptions.push(Box::new(desktop.subscribe(move |_| poke())));
        }
        let supervisor =
            supervisor::Supervisor::start(supervisor::Config::new(renderer), log_event);
        let thread = std::thread::Builder::new()
            .name("wallpaper-motion".into())
            .spawn(move || run(deps, supervisor, receiver))
            .ok()?;
        Some(Self {
            stop: sender,
            thread: Some(thread),
            _subscriptions: subscriptions,
        })
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self._subscriptions.clear();
        let _ = self.stop.send(Msg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn log_event(event: Event) {
    match event {
        Event::Ready { pid } => println!("WALLPAPER_VIDEO_READY pid={pid}"),
        Event::FirstFrame { output } => {
            bead::core::profile::mark("wallpaper_video_first_frame");
            println!("WALLPAPER_VIDEO_FIRST_FRAME output={output}");
        }
        Event::Hwdec { mode } => println!("WALLPAPER_VIDEO_HWDEC mode={mode}"),
        Event::Error { message } => eprintln!("WALLPAPER_VIDEO_ERROR {message}"),
    }
}

fn run(deps: Deps, supervisor: supervisor::Supervisor, receiver: mpsc::Receiver<Msg>) {
    let start = Instant::now();
    let mut group = Group::default();
    let mut source = None;
    let mut sent: Option<(Decoder, Vec<String>)> = None;
    loop {
        let prefs = deps
            .picker
            .as_ref()
            .map_or_else(Prefs::default, |p| Prefs::decode(&p.snapshot().data));
        let wanted = deps.wallpaper.motion();
        if wanted != source {
            supervisor.send(Command::Source {
                source: wanted.clone(),
            });
            source = wanted;
            sent = None;
            group = Group::default();
        }
        let mut wait = Duration::from_secs(3600);
        if source.is_some() {
            let windows = deps
                .backend
                .as_ref()
                .map(|b| b.snapshot())
                .unwrap_or_default();
            let services = deps
                .desktop
                .as_ref()
                .map(|d| d.snapshot())
                .unwrap_or_default();
            let now = start.elapsed().as_millis() as u64;
            let decision = group.step(&prefs.policy(), &inputs(&windows, &services, prefs), now);
            let state = (
                decision.decoder,
                decision.render.into_iter().collect::<Vec<_>>(),
            );
            if sent.as_ref() != Some(&state) {
                supervisor.send(Command::State {
                    decoder: state.0,
                    render: state.1.clone(),
                });
                sent = Some(state);
            }
            if let Some(deadline) = decision.deadline {
                wait = Duration::from_millis(deadline.saturating_sub(now) + 1);
            }
        }
        match receiver.recv_timeout(wait) {
            Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Msg::Poke) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        // Coalesce queued window events without delaying a poster/source handoff.
        while let Ok(message) = receiver.try_recv() {
            if matches!(message, Msg::Stop) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::backends::{Id, Output, Window, Workspace};
    use serde_json::json;

    fn output(name: &str) -> Output {
        Output {
            id: Id(name.into()),
            name: name.into(),
            width: 1920,
            height: 1080,
            scale: 1.0,
            refresh_mhz: 60000,
            x: 0,
            y: 0,
            focused: false,
        }
    }
    fn workspace(id: &str, on: &str, active: bool) -> Workspace {
        Workspace {
            id: Id(id.into()),
            name: id.into(),
            index: 1,
            output: Some(Id(on.into())),
            active,
            focused: false,
            urgent: false,
            fixed_slot: false,
            visiting: false,
        }
    }
    fn window(ws: &str, floating: bool, fullscreen: Option<bool>) -> Window {
        Window {
            id: Id(format!("w-{ws}-{floating}")),
            title: String::new(),
            app_id: String::new(),
            workspace: Some(Id(ws.into())),
            output: None,
            focused: false,
            floating,
            fullscreen,
            urgent: false,
        }
    }
    fn desktop() -> Windows {
        Windows {
            outputs: vec![output("A"), output("B")],
            workspaces: vec![
                workspace("a1", "A", true),
                workspace("a2", "A", false),
                workspace("b1", "B", true),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn prefs_follow_the_qml_defaults_and_ignore_wrong_types() {
        assert_eq!(Prefs::decode(&Json::Null), Prefs::default());
        let store = json!({"store":"wallpaper-picker","data":{"motionMode":"battery","paused":true,"pauseCovered":false}});
        assert_eq!(
            Prefs::decode(&store),
            Prefs {
                mode: Mode::Battery,
                paused: true,
                pause_covered: false
            }
        );
        let wrong = json!({"data":{"motionMode":3,"paused":"yes","pauseCovered":"no"}});
        assert_eq!(Prefs::decode(&wrong), Prefs::default());
        assert!(
            Prefs {
                mode: Mode::Battery,
                ..Prefs::default()
            }
            .policy()
            .pause_on_battery
        );
        assert!(!Prefs::default().policy().pause_on_battery);
    }

    #[test]
    fn tiled_and_fullscreen_windows_cover_only_their_own_outputs_active_workspace() {
        let mut windows = desktop();
        windows.windows = vec![
            window("a1", false, None),
            window("a2", false, None),
            window("b1", true, None),
        ];
        let map = inputs(&windows, &Services::default(), Prefs::default());
        assert!(map["A"].covered && !map["A"].hidden);
        assert!(!map["B"].covered, "a floating window does not cover");
        windows.windows.push(window("b1", true, Some(true)));
        let map = inputs(&windows, &Services::default(), Prefs::default());
        assert!(
            map["B"].covered && map["B"].hidden,
            "fullscreen hides the output"
        );
        windows.windows = vec![window("a2", false, None)];
        let map = inputs(&windows, &Services::default(), Prefs::default());
        assert!(
            !map["A"].covered,
            "a window on an inactive workspace is not covering"
        );
    }

    #[test]
    fn session_battery_and_settings_flow_into_every_output() {
        let mut services = Services::default();
        services.battery.on_battery = true;
        services.session.locked = true;
        let prefs = Prefs {
            mode: Mode::Still,
            paused: true,
            pause_covered: true,
        };
        let map = inputs(&desktop(), &services, prefs);
        for input in map.values() {
            assert!(input.on_battery && input.hidden && input.user_paused && input.stopped);
        }
        let none = inputs(&Windows::default(), &Services::default(), Prefs::default());
        assert_eq!(none.keys().collect::<Vec<_>>(), [ANY]);
    }

    /// The whole Nacre edge over a real daemon connection: a published motion member starts
    /// the renderer with the source and a play state; clearing it stops the source.
    #[tokio::test(flavor = "current_thread")]
    async fn published_motion_drives_the_renderer_and_clearing_it_stops_it() {
        use nacre_protocol::{Message, framing};
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("bead-motion-wire-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let clip = dir.join("clip.mp4");
        std::fs::write(&clip, b"clip").unwrap();
        std::fs::set_permissions(&clip, std::fs::Permissions::from_mode(0o600)).unwrap();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&clip).unwrap(),
        );
        let pair = json!({"revision":1,"palette_revision":1,"path":clip,"file_identity":identity,
            "presentation":"crop","motion":{"kind":"video","path":clip,"file_identity":identity},
            "palette":{"mode":"dark","colours":{"background":"#101820","onSurface":"#f0f4f8","primary":"#22aa88"}}});
        let with_motion = json!({"available":true,"stale":false,"revision":1,
            "outputs":{"DP-1":pair.clone(),"DP-2":pair.clone()},"current":pair});
        // Patches are merge patches: null removes the member.
        let without = json!({"current": {"motion": null}});
        let socket = dir.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (clear_tx, clear_rx) = tokio::sync::oneshot::channel::<()>();
        let (snapshot_tx, snapshot_rx) = tokio::sync::oneshot::channel::<()>();
        let (outputs_tx, outputs_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut reader = framing::Reader::new(tokio::io::BufReader::new(read));
            assert!(matches!(
                reader.next().await.unwrap().unwrap(),
                Message::Hello { .. }
            ));
            let mut methods = nacre_protocol::core_methods();
            methods.insert(
                "wallpaper.activate".into(),
                nacre_protocol::Method {
                    v: 1,
                    class: nacre_protocol::Capability::Desktop,
                    timeout: 1000,
                    params: json!({"type": "object"}),
                    result: json!({}),
                },
            );
            framing::write(
                &mut write,
                &Message::Welcome {
                    proto: 1,
                    min: 1,
                    boot: "00000000-0000-4000-8000-000000000046".into(),
                    topics: BTreeMap::from([("wallpaper.active".into(), 1)]),
                    methods,
                    frame_ms: Some(16),
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                reader.next().await.unwrap().unwrap(),
                Message::Sub { .. }
            ));
            framing::write(
                &mut write,
                &Message::Snap {
                    topic: "wallpaper.active".into(),
                    seq: 0,
                    data: {
                        let mut initial = with_motion.clone();
                        initial["outputs"] = json!({});
                        initial
                    },
                },
            )
            .await
            .unwrap();
            snapshot_tx.send(()).unwrap();
            let _ = outputs_rx.await;
            framing::write(
                &mut write,
                &Message::Patch {
                    topic: "wallpaper.active".into(),
                    base: 0,
                    seq: 1,
                    data: json!({"outputs":with_motion["outputs"]}),
                },
            )
            .await
            .unwrap();
            let _ = clear_rx.await;
            framing::write(
                &mut write,
                &Message::Patch {
                    topic: "wallpaper.active".into(),
                    base: 1,
                    seq: 2,
                    data: without,
                },
            )
            .await
            .unwrap();
            while reader.next().await.is_ok_and(|m| m.is_some()) {}
        });
        let socket_for_client = socket.clone();
        let wallpaper = tokio::task::spawn_blocking(move || {
            crate::wallpaper::Consumer::start(socket_for_client).unwrap()
        })
        .await
        .unwrap();
        let log = dir.join("renderer.log");
        let log_for_command = log.clone();
        let controller = Controller::start_with(
            Deps {
                wallpaper: wallpaper.clone(),
                picker: None,
                backend: None,
                desktop: None,
            },
            move || {
                let mut p = std::process::Command::new("sh");
                p.arg("-c")
                    .arg("cat >> \"$1\"")
                    .arg("sh")
                    .arg(&log_for_command);
                p
            },
        )
        .unwrap();
        let seen = |needle: &'static str| {
            let log = log.clone();
            async move {
                for _ in 0..200 {
                    if std::fs::read_to_string(&log).is_ok_and(|text| text.contains(needle)) {
                        return true;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                false
            }
        };
        snapshot_rx.await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            wallpaper.motion().is_none(),
            "a published source without known outputs is not ready"
        );
        assert!(
            !std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("\"kind\":\"video\""),
            "startup cannot play before any poster/palette commit"
        );
        outputs_tx.send(()).unwrap();
        let mut token = None;
        for _ in 0..200 {
            token = wallpaper.commit_token_for_test("DP-1");
            if token.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let token = token.expect("published pair reached the consumer");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("\"kind\":\"video\""),
            "publishing a motion topic must not bypass the poster/palette commit"
        );
        bead::wayland::WallpaperProvider::committed(&*wallpaper, "DP-1", 1, &token);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            wallpaper.motion().is_none(),
            "the shared source waits for every output using it"
        );
        let second = wallpaper.commit_token_for_test("DP-2").unwrap();
        bead::wayland::WallpaperProvider::committed(&*wallpaper, "DP-2", 1, &second);
        assert!(
            seen("\"kind\":\"video\"").await,
            "source reached the renderer"
        );
        assert!(
            seen("\"decoder\":\"play\",\"render\":[\"*\"]").await,
            "plays on every output"
        );
        clear_tx.send(()).unwrap();
        assert!(
            seen("\"cmd\":\"source\",\"source\":null").await,
            "clearing motion hides video without terminating the warm renderer"
        );
        drop(controller);
        server.abort();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn motion_member_is_validated_against_the_file_it_names() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bead-motion-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.mp4");
        std::fs::write(&clip, b"not really a video").unwrap();
        std::fs::set_permissions(&clip, std::fs::Permissions::from_mode(0o600)).unwrap();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&clip).unwrap(),
        );
        let state = |path: &str, kind: &str, available: bool| {
            json!({"available": available, "stale": !available, "current": {"motion":
                {"kind": kind, "path": path, "file_identity": identity}}})
        };
        let path = clip.to_str().unwrap();
        assert!(crate::wallpaper::parse_motion(&state(path, "video", true)).is_some());
        assert!(crate::wallpaper::parse_motion(&state(path, "animation", true)).is_some());
        assert!(crate::wallpaper::parse_motion(&state(path, "shader", true)).is_none());
        assert!(crate::wallpaper::parse_motion(&state(path, "video", false)).is_none());
        assert!(crate::wallpaper::parse_motion(&state("clip.mp4", "video", true)).is_none());
        assert!(crate::wallpaper::parse_motion(&state("/a/../b.mp4", "video", true)).is_none());
        assert!(crate::wallpaper::parse_motion(&state("http://x/a.mp4", "video", true)).is_none());
        assert!(
            crate::wallpaper::parse_motion(
                &json!({"available": true, "stale": false, "current": {}})
            )
            .is_none()
        );
        // A symlink alias and a replaced file are refused.
        let alias = dir.join("alias.mp4");
        std::os::unix::fs::symlink(&clip, &alias).unwrap();
        assert!(
            crate::wallpaper::parse_motion(&state(alias.to_str().unwrap(), "video", true))
                .is_none()
        );
        std::fs::write(&clip, b"replaced with longer content").unwrap();
        assert!(crate::wallpaper::parse_motion(&state(path, "video", true)).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
