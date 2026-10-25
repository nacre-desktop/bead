// SPDX-License-Identifier: GPL-3.0-only
//! Secure, descriptor-pinned decoding for the published wallpaper contract.
use bead::core::source::{Publisher, Source, Subscription};
use nacre_protocol::wallpaper::{ActiveWallpaper, WallpaperActive, WallpaperFileIdentity};
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

#[derive(Clone)]
struct PreviewSelection {
    boot: String,
    id: String,
    preview: String,
    kind: String,
    animated: bool,
    generation: u64,
    video_identity: Option<WallpaperFileIdentity>,
}
#[derive(Default)]
struct PreviewState {
    generation: u64,
    selected: Option<PreviewSelection>,
}

/// One shared, short-lived selection preview. It carries only bounded path and identity metadata;
/// raster bytes are decoded for a single output frame and then released.
pub(crate) struct Preview {
    publisher: Publisher,
    state: Mutex<PreviewState>,
    publish_order: Mutex<()>,
}
impl Default for Preview {
    fn default() -> Self {
        Self {
            publisher: Publisher::new(
                "wallpaper-preview".into(),
                serde_json::json!({"generation":0,"selected":null}),
            )
            .expect("static wallpaper preview source is valid"),
            state: Mutex::new(PreviewState::default()),
            publish_order: Mutex::new(()),
        }
    }
}
impl Preview {
    pub(crate) fn select(
        &self,
        boot: String,
        id: String,
        preview: String,
        kind: String,
        animated: bool,
    ) -> u64 {
        let _publish_order = self.publish_order.lock().unwrap_or_else(|p| p.into_inner());
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.generation = state.generation.saturating_add(1).max(1);
        let generation = state.generation;
        let video_identity = (kind == "video" || animated)
            .then(|| local_file_identity(&id, 64 * 1024 * 1024 * 1024))
            .flatten();
        let selected = (boot.len() <= 128
            && !boot.is_empty()
            && valid_local_path(&id)
            && valid_local_path(&preview)
            && matches!(kind.as_str(), "image" | "video"))
        .then(|| PreviewSelection {
            boot: boot.clone(),
            id: id.clone(),
            preview: preview.clone(),
            kind: kind.clone(),
            animated,
            generation,
            video_identity,
        });
        let published = selected.as_ref().map(|selection| {
            serde_json::json!({
                "boot":selection.boot,"id":selection.id,"preview":selection.preview,
                "kind":selection.kind,"animated":selection.animated
            })
        });
        state.selected = selected;
        drop(state);
        let _ = self
            .publisher
            .publish(serde_json::json!({"generation":generation,"selected":published}));
        generation
    }
    pub(crate) fn cancel(&self, generation: u64) -> bool {
        let _publish_order = self.publish_order.lock().unwrap_or_else(|p| p.into_inner());
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state
            .selected
            .as_ref()
            .is_some_and(|selected| selected.generation == generation)
        {
            return false;
        }
        state.selected = None;
        drop(state);
        let _ = self.publisher.publish(serde_json::json!({
            "generation":generation,"selected":null
        }));
        true
    }
    pub(crate) fn generation_for(&self, boot: &str, id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selected
            .as_ref()
            .filter(|selected| selected.boot == boot && selected.id == id)
            .map(|selected| selected.generation)
    }
    fn selected(&self) -> Option<PreviewSelection> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selected
            .clone()
    }
}
fn valid_local_path(path: &str) -> bool {
    path.len() <= 4096
        && !path.is_empty()
        && !path.contains(['\0', '\n', '\r'])
        && !path.contains("://")
        && std::path::Path::new(path).is_absolute()
        && !std::path::Path::new(path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}
fn local_file_identity(path: &str, limit: u64) -> Option<WallpaperFileIdentity> {
    if !valid_local_path(path) {
        return None;
    }
    let metadata = std::fs::symlink_metadata(path).ok()?;
    (metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit)
        .then(|| WallpaperFileIdentity::from_metadata(&metadata))
}

pub(crate) struct Consumer {
    client: crate::nacre_source::Client,
    topic: crate::nacre_source::Topic,
    preview: Arc<Preview>,
    subscriptions: Mutex<Vec<Subscription>>,
    displayed: Mutex<BTreeMap<String, ActiveWallpaper>>,
    preview_visual: Mutex<BTreeMap<String, ActiveWallpaper>>,
}
struct PairToken {
    boot: String,
    pair: ActiveWallpaper,
}
struct PreviewToken {
    boot: String,
    id: String,
    generation: u64,
    pair: ActiveWallpaper,
}
impl Consumer {
    #[cfg(test)]
    pub(crate) fn start(socket: std::path::PathBuf) -> Result<Arc<Self>, String> {
        Self::start_with_preview(socket, Arc::new(Preview::default()))
    }
    pub(crate) fn start_with_preview(
        socket: std::path::PathBuf,
        preview: Arc<Preview>,
    ) -> Result<Arc<Self>, String> {
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
            preview,
            subscriptions: Mutex::new(Vec::new()),
            displayed: Mutex::new(BTreeMap::new()),
            preview_visual: Mutex::new(BTreeMap::new()),
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
        let status = self.client.status();
        if !supports_wallpaper_v1(&status) {
            return None;
        }
        let snapshot = Source::snapshot(&self.topic);
        if !snapshot.connected
            || snapshot.data["available"] != true
            || snapshot.data["stale"] == true
        {
            return None;
        }
        let selected = self.preview.selected();
        if let Some(selected) = selected.filter(|selected| {
            status.boot.as_deref() == Some(&selected.boot)
                && !active_matches_id(&snapshot.data, &selected.id)
        }) {
            return pending_motion_source(&selected);
        }
        parse_motion(&snapshot.data)
    }
    /// `settings.store.wallpaper-picker` on this connection (motion mode, pause, cover rule).
    pub(crate) fn picker(&self) -> Option<Arc<dyn Source>> {
        let topic = self.client.topic(crate::prefs::WALLPAPER_PICKER).ok()?;
        Some(Arc::new(topic))
    }
    pub(crate) fn watch_motion(
        &self,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Vec<Subscription>> {
        let preview_changed = changed.clone();
        let preview = self
            .preview
            .publisher
            .subscribe(Arc::new(move |_| preview_changed()))
            .ok()?;
        let active = Source::subscribe(&self.topic, Arc::new(move |_| changed())).ok()?;
        Some(vec![preview, active])
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
        let changed: Arc<dyn Fn() + Send + Sync> = Arc::from(changed);
        let active_changed = changed.clone();
        let subscription = Source::subscribe(&self.topic, Arc::new(move |_| active_changed()))
            .map_err(|_| "wallpaper subscription failed")?;
        self.subscriptions
            .lock()
            .map_err(|_| "wallpaper subscription lock failed")?
            .push(subscription);
        let preview_subscription = self
            .preview
            .publisher
            .subscribe(Arc::new(move |_| changed()))
            .map_err(|_| "wallpaper preview subscription failed")?;
        self.subscriptions
            .lock()
            .map_err(|_| "wallpaper subscription lock failed")?
            .push(preview_subscription);
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
        let boot = status.boot.ok_or("wallpaper daemon boot unavailable")?;
        if let Some(selection) = self.preview.selected()
            && selection.boot == boot
        {
            if state
                .current
                .as_ref()
                .is_some_and(|pair| pair_matches_id(pair, &selection.id))
                || state
                    .outputs
                    .values()
                    .any(|pair| pair_matches_id(pair, &selection.id))
            {
                self.preview.cancel(selection.generation);
            } else if let Some(previous) = self
                .displayed
                .lock()
                .ok()
                .and_then(|displayed| displayed.get(&output).cloned())
                .or_else(|| state.outputs.get(&output).cloned())
                && let Some(pair) = preview_pair(&selection, &previous)
            {
                bead::core::profile::mark("wallpaper_decode_started");
                let decoded = decode_pair(pair, width, height)?;
                bead::core::profile::mark("wallpaper_decode_finished");
                let preview_pair = decoded.pair;
                return Ok(bead::wayland::WallpaperFrame {
                    output,
                    revision: preview_pair.revision.max(1),
                    width: decoded.width,
                    height: decoded.height,
                    rgba: decoded.rgba,
                    previous_rgba: None,
                    // A pending selection never advances the palette. This stays on the last
                    // frame the compositor actually confirmed, even during queued activations.
                    palette: palette(&previous),
                    commit_token: Arc::new(PreviewToken {
                        boot,
                        id: selection.id,
                        generation: selection.generation,
                        pair: preview_pair,
                    }),
                });
            }
        }
        bead::core::profile::mark("wallpaper_decode_started");
        let decoded = decode(&state, &output, width, height)?;
        bead::core::profile::mark("wallpaper_decode_finished");
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
        let previous = self
            .preview_visual
            .lock()
            .ok()?
            .get(output)
            .cloned()
            .or_else(|| self.displayed.lock().ok()?.get(output).cloned())?;
        let state = self.state().ok()?;
        let current = state.outputs.get(output)?.clone();
        let status = self.client.status();
        let preview_pending = self.preview.selected().is_some_and(|selection| {
            status.boot.as_deref() == Some(&selection.boot)
                && !pair_matches_id(&current, &selection.id)
        });
        if previous == current && !preview_pending {
            return None;
        }
        let fallback = self
            .displayed
            .lock()
            .ok()?
            .get(output)
            .filter(|confirmed| **confirmed != previous)
            .cloned();
        decode_pair(previous, width, height)
            .or_else(|_| {
                fallback.map_or_else(
                    || Err("wallpaper unavailable".into()),
                    |p| decode_pair(p, width, height),
                )
            })
            .ok()
            .map(|decoded| decoded.rgba)
    }
    fn is_current(&self, frame: &bead::wayland::WallpaperFrame) -> bool {
        let status = self.client.status();
        if let Some(token) = frame.commit_token.downcast_ref::<PreviewToken>() {
            if status.boot.as_deref() != Some(&token.boot)
                || !supports_wallpaper_v1(&status)
                || self.preview.generation_for(&token.boot, &token.id) != Some(token.generation)
            {
                return false;
            }
            return self.state().is_ok_and(|state| {
                if !state.available || state.stale {
                    return false;
                }
                let matches = state
                    .outputs
                    .get(&frame.output)
                    .is_some_and(|pair| pair_matches_id(pair, &token.id));
                if matches {
                    self.preview.cancel(token.generation);
                }
                !matches
            });
        }
        let Some(token) = frame.commit_token.downcast_ref::<PairToken>() else {
            return false;
        };
        status.boot.as_deref() == Some(&token.boot)
            && supports_wallpaper_v1(&status)
            && self.preview.selected().is_none_or(|selection| {
                selection.boot != token.boot || pair_matches_id(&token.pair, &selection.id)
            })
            && self.state().is_ok_and(|state| {
                state.available
                    && !state.stale
                    && state.outputs.get(&frame.output) == Some(&token.pair)
            })
    }
    fn committed(&self, output: &str, revision: u64, token: &Arc<dyn std::any::Any + Send + Sync>) {
        if let Some(preview) = token.downcast_ref::<PreviewToken>() {
            let status = self.client.status();
            let active_pending = self.state().ok().is_some_and(|state| {
                state.available
                    && !state.stale
                    && state
                        .outputs
                        .get(output)
                        .is_some_and(|pair| !pair_matches_id(pair, &preview.id))
            });
            if preview.pair.revision.max(1) != revision
                || status.boot.as_deref() != Some(&preview.boot)
                || !supports_wallpaper_v1(&status)
                || self.preview.generation_for(&preview.boot, &preview.id)
                    != Some(preview.generation)
                || !active_pending
            {
                return;
            }
            if let Ok(mut visual) = self.preview_visual.lock() {
                visual.insert(output.into(), preview.pair.clone());
            }
            return;
        }
        let Some(pair) = token.downcast_ref::<PairToken>() else {
            return;
        };
        let status = self.client.status();
        if pair.pair.revision != revision
            || status.boot.as_deref() != Some(&pair.boot)
            || !supports_wallpaper_v1(&status)
            || self.preview.selected().is_some_and(|selection| {
                selection.boot == pair.boot && !pair_matches_id(&pair.pair, &selection.id)
            })
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
        if let Ok(mut visual) = self.preview_visual.lock() {
            visual.remove(output);
        }
    }
    fn discarded(&self, _output: &str, _revision: u64) {}
    fn output_removed(&self, output: &str) {
        if let Ok(mut displayed) = self.displayed.lock() {
            displayed.remove(output);
        }
        if let Ok(mut visual) = self.preview_visual.lock() {
            visual.remove(output);
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

fn pair_matches_id(pair: &ActiveWallpaper, id: &str) -> bool {
    pair.path == id || pair.motion.as_ref().is_some_and(|motion| motion.path == id)
}
fn active_matches_id(state: &serde_json::Value, id: &str) -> bool {
    state.get("available") == Some(&serde_json::Value::Bool(true))
        && state.get("stale") == Some(&serde_json::Value::Bool(false))
        && (state
            .get("current")
            .and_then(|pair| serde_json::from_value::<ActiveWallpaper>(pair.clone()).ok())
            .is_some_and(|pair| pair_matches_id(&pair, id))
            || state["outputs"].as_object().is_some_and(|outputs| {
                outputs.values().any(|value| {
                    serde_json::from_value::<ActiveWallpaper>(value.clone())
                        .is_ok_and(|pair| pair_matches_id(&pair, id))
                })
            }))
}
fn pending_motion_source(selected: &PreviewSelection) -> Option<bead_wallpaper::protocol::Source> {
    if selected.kind != "video" && !selected.animated {
        return None;
    }
    let identity = selected.video_identity.as_ref()?;
    let metadata = std::fs::symlink_metadata(&selected.id).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || !identity.matches(&metadata) {
        return None;
    }
    Some(bead_wallpaper::protocol::Source::Video {
        path: selected.id.clone(),
        fit: bead_wallpaper::protocol::Fit::Crop,
        looped: true,
    })
}

/// Replace only the image source on a confirmed pair, retaining its published palette and
/// presentation. The normal descriptor-pinned decoder then scales the bounded preview.
fn preview_pair(
    selection: &PreviewSelection,
    confirmed: &ActiveWallpaper,
) -> Option<ActiveWallpaper> {
    const PREVIEW_LIMIT: u64 = 2 * 1024 * 1024;
    let identity = local_file_identity(&selection.preview, PREVIEW_LIMIT)?;
    let reader = image::ImageReader::open(&selection.preview)
        .ok()?
        .with_guessed_format()
        .ok()?;
    if reader.format() != Some(image::ImageFormat::Jpeg) {
        return None;
    }
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > 1600 || height > 1000 {
        return None;
    }
    if local_file_identity(&selection.preview, PREVIEW_LIMIT).as_ref() != Some(&identity) {
        return None;
    }
    let mut pair = confirmed.clone();
    pair.path = selection.preview.clone();
    pair.file_identity = identity;
    pair.motion = None;
    Some(pair)
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
    fn pending_motion_is_video_only_and_source_identity_fenced() {
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        let video = root.join("selected.mp4");
        std::fs::write(&video, b"video source").unwrap();
        let preview = Preview::default();
        let generation = preview.select(
            "boot-preview".into(),
            video.to_string_lossy().into_owned(),
            root.join("preview.jpg").to_string_lossy().into_owned(),
            "video".into(),
            false,
        );
        let selected = preview.selected().unwrap();
        assert!(matches!(
            pending_motion_source(&selected),
            Some(bead_wallpaper::protocol::Source::Video { ref path, .. }) if path == video.to_str().unwrap()
        ));
        let mut animated_still = selected.clone();
        animated_still.kind = "image".into();
        animated_still.animated = true;
        assert!(matches!(
            pending_motion_source(&animated_still),
            Some(bead_wallpaper::protocol::Source::Video { .. })
        ));
        let mut still = animated_still.clone();
        still.animated = false;
        assert!(pending_motion_source(&still).is_none());
        std::fs::write(&video, b"replacement source").unwrap();
        assert!(pending_motion_source(&selected).is_none());
        assert!(preview.cancel(generation));
        let _ = std::fs::remove_dir_all(root);
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
        let preview_path = root.join("selection-preview.jpg");
        image::DynamicImage::new_rgb8(1, 1)
            .save(&preview_path)
            .unwrap();
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
        let preview = Arc::new(Preview::default());
        let preview_for_client = preview.clone();
        let consumer = tokio::task::spawn_blocking(move || {
            Consumer::start_with_preview(socket_for_client, preview_for_client).unwrap()
        })
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
        let selected_id = root.join("selected.jpg").to_string_lossy().into_owned();
        let generation = preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            selected_id.clone(),
            preview_path.to_string_lossy().into_owned(),
            "image".into(),
            false,
        );
        let prepare_consumer = consumer.clone();
        let frame = tokio::task::spawn_blocking(move || {
            prepare_consumer.prepare("DP-1".into(), 2, 1).unwrap()
        })
        .await
        .unwrap();
        let token = Arc::downgrade(&frame.commit_token);
        assert_eq!(frame.revision, 1);
        assert_eq!(frame.rgba.len(), 8);
        assert!(frame.commit_token.is::<PreviewToken>());
        assert!(bead::wayland::WallpaperProvider::is_current(
            &*consumer, &frame
        ));
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            frame.revision,
            &frame.commit_token,
        );
        assert!(consumer.displayed("DP-1").is_none());
        assert_eq!(
            preview.generation_for("00000000-0000-4000-8000-000000000021", &selected_id),
            Some(generation)
        );
        let superseding = preview.select(
            "00000000-0000-4000-8000-000000000021".into(),
            root.join("second-selection.jpg")
                .to_string_lossy()
                .into_owned(),
            preview_path.to_string_lossy().into_owned(),
            "image".into(),
            false,
        );
        assert!(superseding > generation);
        assert!(!bead::wayland::WallpaperProvider::is_current(
            &*consumer, &frame
        ));
        assert!(preview.cancel(superseding));
        let prepare_consumer = consumer.clone();
        let confirmed = tokio::task::spawn_blocking(move || {
            prepare_consumer.prepare("DP-1".into(), 2, 1).unwrap()
        })
        .await
        .unwrap();
        assert!(confirmed.commit_token.is::<PairToken>());
        assert!(bead::wayland::WallpaperProvider::is_current(
            &*consumer, &confirmed
        ));
        bead::wayland::WallpaperProvider::committed(
            &*consumer,
            "DP-1",
            confirmed.revision,
            &confirmed.commit_token,
        );
        assert_eq!(consumer.displayed("DP-1"), Some(expected_pair));
        drop(confirmed);
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
