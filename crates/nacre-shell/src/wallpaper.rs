// SPDX-License-Identifier: GPL-3.0-only
//! Secure, descriptor-pinned decoding for the published wallpaper contract.
use bead::core::source::{Source, Subscription};
use nacre_protocol::wallpaper::{ActiveWallpaper, WallpaperActive};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

fn supports_wallpaper_v1(status: &crate::nacre_source::Status) -> bool {
    status.topics.get("wallpaper.active") == Some(&1)
        && status
            .methods
            .get("wallpaper.activate")
            .is_some_and(|method| method.v == 1)
}

pub(crate) struct Consumer {
    client: crate::nacre_source::Client,
    topic: crate::nacre_source::Topic,
    subscriptions: Mutex<Vec<Subscription>>,
    displayed: Mutex<BTreeMap<String, ActiveWallpaper>>,
}
struct PairToken {
    boot: String,
    pair: ActiveWallpaper,
}
impl Consumer {
    pub(crate) fn start(socket: std::path::PathBuf) -> Result<Arc<Self>, String> {
        let client = crate::nacre_source::Client::start(crate::nacre_source::Config::new(
            socket,
            // The picker settings drive motion pause rules; a daemon without the topic is
            // simply not subscribed to it.
            vec![
                "daemon".into(),
                "wallpaper.active".into(),
                crate::prefs::WALLPAPER_PICKER.into(),
            ],
        ))
        .map_err(|_| "wallpaper daemon connection failed")?;
        let topic = client
            .topic("wallpaper.active")
            .map_err(|_| "wallpaper topic unavailable")?;
        let consumer = Arc::new(Self {
            client,
            topic,
            subscriptions: Mutex::new(Vec::new()),
            displayed: Mutex::new(BTreeMap::new()),
        });
        // Retain the reconnecting client even when the daemon starts later.
        // Contract checks belong to state/activation, never startup admission.
        Ok(consumer)
    }

    fn state(&self) -> Result<WallpaperActive, String> {
        let snapshot = Source::snapshot(&self.topic);
        if !snapshot.connected {
            return Err("wallpaper daemon disconnected".into());
        }
        let state: WallpaperActive = serde_json::from_value((*snapshot.data).clone())
            .map_err(|_| "wallpaper state malformed")?;
        state.validate().map_err(|_| "wallpaper state malformed")?;
        Ok(state)
    }

    /// Motion source of the active wallpaper: the additive optional `motion` member of the
    /// published `current` pair (a video or animation file for the poster being shown).
    /// `None` for a still wallpaper, an older daemon, or an unavailable/stale state.
    pub(crate) fn motion(&self) -> Option<bead_wallpaper::protocol::Source> {
        let snapshot = Source::snapshot(&self.topic);
        snapshot.connected.then(|| parse_motion(&snapshot.data))?
    }
    /// `settings.store.wallpaper-picker` on this connection (motion mode, pause, cover rule).
    pub(crate) fn picker(&self) -> Option<Arc<dyn Source>> {
        let topic = self.client.topic(crate::prefs::WALLPAPER_PICKER).ok()?;
        Some(Arc::new(topic))
    }
    pub(crate) fn watch_motion(
        &self,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Subscription> {
        Source::subscribe(&self.topic, Arc::new(move |_| changed())).ok()
    }

    /// Palette is deliberately committed from the matching activation pair only
    /// after the corresponding background buffer is attached and committed.
    #[allow(dead_code)]
    pub(crate) fn displayed(&self, output: &str) -> Option<ActiveWallpaper> {
        self.displayed.lock().ok()?.get(output).cloned()
    }
}
impl bead::wayland::WallpaperProvider for Consumer {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let subscription = Source::subscribe(&self.topic, Arc::new(move |_| changed()))
            .map_err(|_| "wallpaper subscription failed")?;
        self.subscriptions
            .lock()
            .map_err(|_| "wallpaper subscription lock failed")?
            .push(subscription);
        Ok(())
    }
    fn prepare(
        &self,
        output: String,
        width: u32,
        height: u32,
    ) -> Result<bead::wayland::WallpaperFrame, String> {
        let status = self.client.status();
        if !supports_wallpaper_v1(&status) {
            return Err("wallpaper v1 contract unavailable".into());
        }
        let state = self.state()?;
        if !state.available || state.stale {
            return Err("wallpaper is stale".into());
        }
        let decoded = decode(&state, &output, width, height)?;
        let boot = status.boot.ok_or("wallpaper daemon boot unavailable")?;
        let request = serde_json::json!({"revision": decoded.pair.revision});
        let reply = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|_| "wallpaper activation runtime unavailable")?
            .block_on(Source::call(
                &self.topic,
                "wallpaper.activate".into(),
                request,
                Duration::from_secs(1),
            ))
            .map_err(|_| "wallpaper activation failed")?;
        let activated: ActiveWallpaper =
            serde_json::from_value(reply).map_err(|_| "wallpaper activation response malformed")?;
        activated
            .validate()
            .map_err(|_| "wallpaper activation response malformed")?;
        let latest = self.state()?;
        let after_status = self.client.status();
        if after_status.boot.as_deref() != Some(&boot)
            || !supports_wallpaper_v1(&after_status)
            || latest.revision != activated.revision
            || latest.outputs.get(&output) != Some(&activated)
            || decoded.pair != activated
        {
            return Err("wallpaper activation superseded".into());
        }
        let commit_token: Arc<dyn std::any::Any + Send + Sync> = Arc::new(PairToken {
            boot,
            pair: activated.clone(),
        });
        let palette = palette(&activated);
        Ok(bead::wayland::WallpaperFrame {
            output,
            revision: activated.revision,
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
            previous_rgba: None,
            palette,
            commit_token,
        })
    }
    fn prepare_transition(&self, output: &str, width: u32, height: u32) -> Option<Vec<u8>> {
        let previous = self.displayed.lock().ok()?.get(output)?.clone();
        let current = self.state().ok()?.outputs.get(output)?.clone();
        if previous == current {
            return None;
        }
        decode_pair(previous, width, height)
            .ok()
            .map(|decoded| decoded.rgba)
    }
    fn is_current(&self, frame: &bead::wayland::WallpaperFrame) -> bool {
        let Some(token) = frame.commit_token.downcast_ref::<PairToken>() else {
            return false;
        };
        let status = self.client.status();
        status.boot.as_deref() == Some(&token.boot)
            && supports_wallpaper_v1(&status)
            && self.state().is_ok_and(|state| {
                state.available
                    && !state.stale
                    && state.outputs.get(&frame.output) == Some(&token.pair)
            })
    }
    fn committed(&self, output: &str, revision: u64, token: &Arc<dyn std::any::Any + Send + Sync>) {
        let Some(pair) = token.downcast_ref::<PairToken>() else {
            return;
        };
        let status = self.client.status();
        if pair.pair.revision != revision
            || status.boot.as_deref() != Some(&pair.boot)
            || status.topics.get("wallpaper.active") != Some(&1)
            || !status
                .methods
                .get("wallpaper.activate")
                .is_some_and(|method| method.v == 1)
        {
            return;
        }
        if let Ok(mut displayed) = self.displayed.lock() {
            displayed.insert(output.into(), pair.pair.clone());
        }
    }
    fn discarded(&self, _output: &str, _revision: u64) {}
    fn output_removed(&self, output: &str) {
        if let Ok(mut displayed) = self.displayed.lock() {
            displayed.remove(output);
        }
    }
}

/// `current.motion` = `{kind: "video"|"animation", path, file_identity}`. Same path rules as
/// the poster (absolute, local, no `..`), a regular non-symlink file, and the published
/// identity must still match, so a swapped file is never played.
pub(crate) fn parse_motion(state: &serde_json::Value) -> Option<bead_wallpaper::protocol::Source> {
    use nacre_protocol::wallpaper::WallpaperFileIdentity;
    if state.get("available")? != true || state.get("stale")? != false {
        return None;
    }
    let motion = state.get("current")?.get("motion")?;
    let path = motion.get("path")?.as_str()?;
    if !matches!(motion.get("kind")?.as_str()?, "video" | "animation")
        || path.len() > 4096
        || path.contains(['\0', '\n'])
        || path.contains("://")
        || !std::path::Path::new(path).is_absolute()
        || std::path::Path::new(path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let identity: WallpaperFileIdentity =
        serde_json::from_value(motion.get("file_identity")?.clone()).ok()?;
    let linked = std::fs::symlink_metadata(path).ok()?;
    if !linked.is_file() || !identity.matches(&linked) {
        return None;
    }
    Some(bead_wallpaper::protocol::Source::Video {
        path: path.to_owned(),
        fit: bead_wallpaper::protocol::Fit::Crop,
        looped: true,
    })
}

pub(crate) struct Decoded {
    pub width: u32,
    pub height: u32,
    /// Output-sized RGBA bytes; no pathname or source-sized image is retained.
    pub rgba: Vec<u8>,
    pub pair: ActiveWallpaper,
}

/// Decode one active pair only after validating the complete contract snapshot.
/// The short-lived worker streams from a pinned descriptor and scales during
/// decoding; the UI receives only bounded output pixels.
pub(crate) fn decode(
    state: &WallpaperActive,
    output: &str,
    width: u32,
    height: u32,
) -> Result<Decoded, String> {
    state.validate().map_err(|_| "invalid wallpaper state")?;
    if !state.available || state.stale || width == 0 || height == 0 {
        return Err("wallpaper unavailable".into());
    }
    let pair = state
        .outputs
        .get(output)
        .ok_or("output has no wallpaper")?
        .clone();
    decode_pair(pair, width, height)
}

pub(crate) fn decode_pair(
    pair: ActiveWallpaper,
    width: u32,
    height: u32,
) -> Result<Decoded, String> {
    #[cfg(test)]
    let rgba = crate::wallpaper_decode::decode_pair(pair.clone(), width, height)?;
    #[cfg(not(test))]
    let rgba = decode_in_worker(&pair, width, height)?;
    Ok(Decoded {
        width,
        height,
        rgba,
        pair,
    })
}

#[cfg(not(test))]
fn decode_in_worker(pair: &ActiveWallpaper, width: u32, height: u32) -> Result<Vec<u8>, String> {
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
    };
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n > 0 && *n <= 64 * 1024 * 1024)
        .ok_or("output dimensions exceed wallpaper bound")?;
    if width > 16_384 || height > 16_384 {
        return Err("output dimensions exceed wallpaper bound".into());
    }
    let executable = std::env::current_exe()
        .map_err(|_| "wallpaper worker unavailable")?
        .with_file_name("nacre-wallpaper-decode");
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "wallpaper worker unavailable")?;
    let result = (|| {
        let mut input = child
            .stdin
            .take()
            .ok_or("wallpaper worker input unavailable")?;
        let request =
            serde_json::to_vec(&(pair, width, height)).map_err(|_| "invalid wallpaper request")?;
        input
            .write_all(&request)
            .map_err(|_| "wallpaper worker input failed")?;
        drop(input);
        let mut rgba = vec![0; expected];
        let mut output = child
            .stdout
            .take()
            .ok_or("wallpaper worker output unavailable")?;
        output
            .read_exact(&mut rgba)
            .map_err(|_| "wallpaper worker output failed")?;
        if output
            .read(&mut [0])
            .map_err(|_| "wallpaper worker output failed")?
            != 0
        {
            return Err("wallpaper worker output size mismatch");
        }
        Ok(rgba)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|_| "wallpaper worker wait failed")?;
    if !status.success() {
        return Err("wallpaper decode failed".into());
    }
    result.map_err(String::from)
}

pub(crate) fn palette(pair: &ActiveWallpaper) -> bead::render::Palette {
    let colours = pair
        .palette
        .colours
        .iter()
        .filter_map(|(role, value)| {
            u32::from_str_radix(value.trim_start_matches('#'), 16)
                .ok()
                .map(|rgb| {
                    (
                        role.clone(),
                        [
                            ((rgb >> 16) & 255) as u8,
                            ((rgb >> 8) & 255) as u8,
                            (rgb & 255) as u8,
                            255,
                        ],
                    )
                })
        })
        .collect();
    bead::render::Palette(std::sync::Arc::new(colours))
}

/// B0068: the live palette of the `wallpaper.active` topic for an app process (no wallpaper
/// surface of its own). A stale or unavailable state keeps the last palette the app wore.
pub(crate) struct Feed {
    topic: crate::nacre_source::Topic,
    subscription: Mutex<Option<Subscription>>,
}
impl Feed {
    pub(crate) fn new(topic: crate::nacre_source::Topic) -> Arc<Self> {
        Arc::new(Self {
            topic,
            subscription: Mutex::new(None),
        })
    }
}
fn live(snapshot: &bead::core::source::Snapshot) -> Option<BTreeMap<String, [u8; 4]>> {
    if !snapshot.connected {
        return None;
    }
    let state: WallpaperActive = serde_json::from_value((*snapshot.data).clone()).ok()?;
    if !state.available || state.stale {
        return None;
    }
    let pair = state
        .current
        .or_else(|| state.outputs.into_values().next())?;
    Some(
        palette(&pair)
            .0
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
    )
}
impl bead::wayland::PaletteFeed for Feed {
    fn current(&self) -> Option<BTreeMap<String, [u8; 4]>> {
        live(&Source::snapshot(&self.topic))
    }
    fn watch(&self, changed: Box<dyn Fn(BTreeMap<String, [u8; 4]>) + Send + Sync>) {
        let subscription = Source::subscribe(
            &self.topic,
            Arc::new(move |snapshot| {
                if let Some(palette) = live(&snapshot) {
                    changed(palette);
                }
            }),
        );
        *self.subscription.lock().unwrap_or_else(|p| p.into_inner()) = subscription.ok();
    }
}

/// Select an applicable topic revision; unavailable input never erases a good
/// displayed pair. Activation results are accepted only if still current.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct CommitState {
    pub pending_revision: u64,
    pub displayed: Option<ActiveWallpaper>,
}
#[cfg(test)]
impl CommitState {
    pub fn observe(&mut self, state: &WallpaperActive) -> Option<u64> {
        if state.validate().is_err() || !state.available || state.stale {
            return None;
        }
        let pair = state.current.as_ref()?;
        if self
            .displayed
            .as_ref()
            .is_some_and(|shown| shown.revision == pair.revision)
        {
            return None;
        }
        self.pending_revision = pair.revision;
        Some(pair.revision)
    }
    pub fn activate(&mut self, revision: u64, pair: ActiveWallpaper) -> bool {
        if revision != self.pending_revision
            || pair.revision != revision
            || pair.palette_revision != revision
            || pair.validate().is_err()
        {
            return false;
        }
        self.displayed = Some(pair);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::wayland::WallpaperProvider as _;
    use image::ImageEncoder;
    use nacre_protocol::{Capability, Message, Method, Reply, framing};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn stale_unavailable_and_superseded_activation_keep_last_good_pair() {
        let mut state = CommitState::default();
        let first = pair(1);
        state.pending_revision = 1;
        assert!(state.activate(1, first.clone()));
        assert_eq!(state.displayed.as_ref().unwrap().palette, first.palette);
        let second = pair(2);
        let active = WallpaperActive {
            available: true,
            stale: false,
            revision: 2,
            current: Some(second.clone()),
            outputs: BTreeMap::from([("DP-1".into(), second.clone())]),
            error: None,
        };
        assert_eq!(state.observe(&active), Some(2));
        // A failed activation has no commit call, so both image token and palette stay old.
        assert_eq!(state.displayed.as_ref().unwrap().revision, 1);
        assert!(!state.activate(3, second.clone()));
        assert_eq!(state.displayed, Some(first));
        assert!(
            state
                .observe(&WallpaperActive {
                    available: false,
                    stale: true,
                    revision: 1,
                    current: None,
                    outputs: Default::default(),
                    error: None
                })
                .is_none()
        );
        assert_eq!(state.displayed.as_ref().unwrap().revision, 1);
        assert!(state.activate(2, second.clone()));
        assert_eq!(state.displayed, Some(second));
    }
    #[test]
    fn runtime_pixbuf_modules_cover_every_contract_codec() {
        let available: Vec<String> = gdk_pixbuf::Pixbuf::formats()
            .into_iter()
            .filter_map(|format| format.name().map(|name| name.to_string()))
            .collect();
        for codec in ["png", "jpeg", "webp", "gif", "tiff"] {
            assert!(
                available.iter().any(|name| name == codec),
                "missing GdkPixbuf loader {codec}: {available:?}"
            );
        }
    }

    #[test]
    fn wallpaper_topic_and_method_require_exact_published_v1() {
        let mut status = crate::nacre_source::Status::default();
        assert!(!supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 1);
        assert!(!supports_wallpaper_v1(&status));
        status.methods.insert(
            "wallpaper.activate".into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 1000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
        assert!(supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 2);
        assert!(!supports_wallpaper_v1(&status));
        status.topics.insert("wallpaper.active".into(), 1);
        status.methods.get_mut("wallpaper.activate").unwrap().v = 2;
        assert!(!supports_wallpaper_v1(&status));
    }

    #[test]
    fn decoder_rejects_malformed_bytes_identity_mismatch_and_symlink_paths() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let malformed = root.join("malformed.png");
        std::fs::write(&malformed, b"not an image").unwrap();
        std::fs::set_permissions(&malformed, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut malformed_pair = pair(1);
        malformed_pair.path = malformed.to_string_lossy().into_owned();
        malformed_pair.file_identity =
            nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&malformed).unwrap(),
            );
        let state = active(malformed_pair.clone());
        assert!(decode(&state, "DP-1", 2, 1).is_err());

        let other = root.join("other.png");
        std::fs::write(&other, b"other bytes").unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o600)).unwrap();
        malformed_pair.file_identity =
            nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&other).unwrap(),
            );
        assert!(decode(&active(malformed_pair), "DP-1", 2, 1).is_err());

        let alias = root.join("alias.png");
        symlink(&malformed, &alias).unwrap();
        let mut alias_pair = pair(1);
        alias_pair.path = alias.to_string_lossy().into_owned();
        alias_pair.file_identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&malformed).unwrap(),
        );
        assert!(decode(&active(alias_pair), "DP-1", 2, 1).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn private_bus_snapshot_activation_and_frame_commit_keep_the_pair_atomic() {
        use tokio::net::UnixListener;
        let root = std::env::temp_dir().join(format!("bead-wallpaper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("poster.png");
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[30, 80, 120, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&path).unwrap(),
        );
        let pair = ActiveWallpaper {
            revision: 1,
            palette_revision: 1,
            path: path.to_string_lossy().into_owned(),
            file_identity: identity,
            presentation: nacre_protocol::wallpaper::Presentation::Crop,
            motion: None,
            palette: nacre_protocol::wallpaper::WallpaperPalette {
                mode: nacre_protocol::wallpaper::PaletteMode::Dark,
                colours: BTreeMap::from([
                    ("background".into(), "#101820".into()),
                    ("onSurface".into(), "#f0f4f8".into()),
                    ("primary".into(), "#22aa88".into()),
                ]),
            },
        };
        let state = WallpaperActive {
            available: true,
            stale: false,
            revision: 1,
            current: Some(pair.clone()),
            outputs: BTreeMap::from([("DP-1".into(), pair.clone())]),
            error: None,
        };
        let expected_pair = pair.clone();
        let socket = root.join("daemon.sock");
        let socket_for_client = socket.clone();
        let consumer =
            tokio::task::spawn_blocking(move || Consumer::start(socket_for_client).unwrap())
                .await
                .unwrap();
        assert!(!consumer.topic.snapshot().connected);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
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
                Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout: 1000,
                    params: serde_json::json!({"type":"object"}),
                    result: serde_json::json!({}),
                },
            );
            framing::write(
                &mut write,
                &Message::Welcome {
                    proto: 1,
                    min: 1,
                    boot: "00000000-0000-4000-8000-000000000021".into(),
                    topics: BTreeMap::from([("wallpaper.active".into(), 1)]),
                    methods,
                    frame_ms: Some(16),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                reader.next().await.unwrap().unwrap(),
                Message::Sub {
                    topics: vec!["wallpaper.active".into()]
                }
            );
            framing::write(
                &mut write,
                &Message::Snap {
                    topic: "wallpaper.active".into(),
                    seq: 0,
                    data: serde_json::to_value(&state).unwrap(),
                },
            )
            .await
            .unwrap();
            let Message::Call { id, m, p, .. } = reader.next().await.unwrap().unwrap() else {
                panic!("wallpaper.activate call expected");
            };
            assert_eq!(m, "wallpaper.activate");
            assert_eq!(p, serde_json::json!({"revision":1}));
            framing::write(
                &mut write,
                &Message::Ret {
                    id,
                    reply: Reply::Ok {
                        ok: serde_json::to_value(&pair).unwrap(),
                    },
                },
            )
            .await
            .unwrap();
            let _ = release_rx.await;
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !consumer.topic.snapshot().connected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let prepare_consumer = consumer.clone();
        let frame = tokio::task::spawn_blocking(move || {
            prepare_consumer.prepare("DP-1".into(), 2, 1).unwrap()
        })
        .await
        .unwrap();
        let token = Arc::downgrade(&frame.commit_token);
        assert_eq!(frame.revision, 1);
        assert_eq!(frame.rgba.len(), 8);
        assert!(bead::wayland::WallpaperProvider::is_current(
            &*consumer, &frame
        ));
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            frame.revision,
            &frame.commit_token,
        );
        assert_eq!(consumer.displayed("DP-1"), Some(expected_pair));
        drop(frame);
        assert!(
            token.upgrade().is_none(),
            "dropping the frame releases its pending raster token"
        );
        bead::wayland::WallpaperProvider::output_removed(&*consumer, "DP-1");
        assert!(
            consumer.displayed("DP-1").is_none(),
            "output removal must release the committed image/palette pair"
        );
        let _ = release_tx.send(());
        server.await.unwrap();
        drop(consumer);
        let _ = std::fs::remove_dir_all(root);
    }
    fn pair(revision: u64) -> ActiveWallpaper {
        serde_json::from_value(serde_json::json!({"revision":revision,"palette_revision":revision,"path":"/wall.png","file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":0,"mtime_nsec":0,"ctime_sec":0,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}})).unwrap()
    }
    fn active(pair: ActiveWallpaper) -> WallpaperActive {
        WallpaperActive {
            available: true,
            stale: false,
            revision: pair.revision,
            current: Some(pair.clone()),
            outputs: BTreeMap::from([("DP-1".into(), pair)]),
            error: None,
        }
    }
}
