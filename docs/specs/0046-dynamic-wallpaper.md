# B0046 — Dynamic (video) wallpaper

Status: **Review — renderer, policy, supervisor and Nacre edge implemented and tested; end-to-end on the
real daemon is blocked on one additive Nacre contract field (below).** Owner decision 2026-10-23: port every
QML feature to Bead. Follows B0021 (static wallpaper), which stays the fallback underneath.

## What it is

A video (or animated GIF) wallpaper that plays on every output, crop-filled, looping, with the QML's pause rules
plus the power rules below. It is built from three general pieces and one Nacre edge:

| Piece | Where | Licence | Knows Nacre? |
| --- | --- | --- | --- |
| Protocol (`Source` kinds, `Command`, `Event`), pause/release **policy** state machine, crash-tolerant **supervisor** | `crates/bead-wallpaper` | MIT OR Apache-2.0 | no |
| **Renderer** `bead-wallpaper-video`: one libmpv decoder, one bottom-layer surface per output | `crates/nacre-shell/src/bin/bead-wallpaper-video/` | GPL-3.0-only (libmpv) | no |
| Nacre edge: reads the `wallpaper.active` motion member and the `wallpaper-picker` settings, turns compositor/battery/session state into policy inputs | `crates/nacre-shell/src/wallpaper_motion.rs`, `wallpaper.rs` | GPL-3.0-only | yes, only here |

Wallpaper *kinds* are protocol data: `Source::Video{path, fit, loop}` today; a new kind is a new enum variant,
and a renderer that does not know a kind reports an error event instead of guessing. Pause behaviour is
`policy::Config` (`pause_on_battery`, `pause_when_covered`, release delays; `None` = never release) and per-output
`Inputs`; the Nacre settings only map onto that struct. Nothing in `bead-wallpaper` or the renderer names a
product, a setting or a daemon.

## ADR 0005 exception (deliberate)

The renderer is **not** a UI provider and not a UI session. It is a separate small process supervised by the
shell: no daemon registration, no Luau, no fonts, no UI renderer; it owns background layer surfaces and one
decoder. Reason: libmpv and the codec libraries cost ~37 MiB file-backed plus 35-75 MiB anonymous while
decoding; they must neither count against the UI memory gate nor be able to take the shell down. Recorded in
ADR 0005. Its memory and CPU are reported separately (table below).

## Design choices and why

- **libmpv, software render API, copy-back hardware decode.** The renderer loads `libmpv.so.2` with `dlopen`
  (no build dependency, no new crate, nothing to cargo-deny; a missing library only disables video and is
  reported as an event). mpv renders the frame already scaled and cropped into a `wl_shm` buffer
  (`panscan=1.0` is the QML's `PreserveAspectCrop`). Hardware decode is `hwdec=vaapi-copy`; if VA-API is
  unavailable or fails, mpv falls back to software decode on its own (CPU fallback). Zero-copy GL/dmabuf would
  save the copy and the CPU scale but needs an EGL context per output, a GPU in nested/CI runs and ~600 more
  lines; it is a recorded follow-up, not needed for the gates. Smallest dependency set: `libmpv` only
  (`mpv` package), no GStreamer, no EGL.
- **One decoder for all outputs.** Each wanted output gets the same decoded frame rendered at its own size
  (per-output placement). The QML ran one decoder per output.
- **Fallback is the static wallpaper.** The renderer's surface sits on layer `bottom`, above B0021's `background`
  surface, and is only mapped after the first decoded frame (QML `frameReady`). If the renderer dies the
  compositor drops its surfaces and the poster underneath is visible again; the supervisor restarts it with
  backoff (250 ms, 1 s, 4 s) and gives up after 4 restarts in 60 s (`abandoned`) until a new source arrives. The
  shell never blocks on or links the renderer.
- **Keep the last frame.** Pause leaves the last buffer on screen. On *release* the decoder is destroyed,
  libmpv is `dlclose`d, the heap is trimmed and the pool is compacted to the one frame that stays attached;
  resume restarts at the saved position and swaps buffers only when the first new frame is ready.
- **DPMS / occlusion.** Frame callbacks stop when an output is off or fully occluded. Rendering is throttled by
  frame callbacks; if one is missing for 1.5 s the decoder pauses itself and resumes when the callback returns.

## Settings and contract mapping

| QML | Bead |
| --- | --- |
| `NacreWallpapers.displayPath/displayDynamic` (private `media.json`) | **Contract gap**: `wallpaper.active.current.motion` (below) |
| `WallpaperPlayback.paused` (`wallpaper-picker.paused`) | `user_paused` input (hold) |
| `motionMode` `full` / `battery` / `still` | `full`: no battery rule. `battery`: `pause_on_battery` (hold). `still`: `stopped` (decoder released at once, poster shows). Default `full` as in the QML |
| `pauseCovered` (default true) | `pause_when_covered`; covered = a non-floating window on the output's active workspace |
| fullscreen window on the workspace | `hidden` (pause, release after 5 s like the QML `releaseDelay`) |
| `WallpaperPlayback.locked` / `sleeping` | `hidden`; logind `LockedHint` and `PrepareForSleep` now in `bead-services` (`Snapshot.session`) |
| `UPower.onBattery` | `Snapshot.battery.on_battery` (UPower `OnBattery`) |
| picker open overrides covered | not ported: the Bead launcher has no wallpaper mode (B0028); `force_visible` exists in the policy for it |

Release delays: hidden 5 s (QML), held 30 s (new; keeps resume instant for short pauses). Held pauses keep the
last frame instead of the QML's drop back to the poster. DPMS is the frame-callback watchdog above.

## Nacre blocker: the video path is not published (needs a Nacre unit)

`wallpaper.active` v1 carries only the static poster (`path` = a cached PNG) and states "motion/video playback is
outside this unit"; the selected video lives only in private `media.json`, which Bead must not read. The
smallest additive change (optional field, no version bump, per the foundation exception): on the pair, publish

```json
"motion": {"kind": "video" | "animation", "path": "/abs/clip.mp4", "file_identity": { ...same shape as the poster... }}
```

taken by the daemon's `active_wallpaper.rs` from `media.json` when `media.poster == presentation.poster` and
`dynamic == true` (`kind` = `animation` for animated GIFs), under the same ownership/ancestry checks as the poster
but with a larger size cap than 128 MiB (identity `size` is not bounded for motion). Also add `motion` to the
`ActiveWallpaper` type, schema and a golden. Bead already parses it (`wallpaper::parse_motion`: absolute, no `..`,
no URL, regular non-symlink file, identity must still match) and tolerates its absence (still wallpapers and older
daemons behave exactly as before). The `settings.store.wallpaper-picker` store is already published.

## Memory and CPU (CPU backend recorded: libmpv software render, wl_shm; headless sway, pixman; Ubuntu 24.04 CI image, no GPU)

Host: prod, Ubuntu 24.04 container (12 CPUs), headless sway 1.9 (pixman), 1920x1080 output, 1080p30 H.264 test clip
made by ffmpeg in the container (never the owner's files), release build, libmpv 0.37 software decode (VA-API is
not available there, mpv reported `hwdec: no`). `spikes/wallpaper-video-shell-proof.py --runs 3`: three alternating
control/video pairs of the real `nacre-shell --daemon-socket` against a fake daemon, same timeline in every run
(steady, user-paused, held 33 s until the decoder is released, resumed), medians of 5 or 3 samples per run.

| Shell UI process (PSS MiB) | control (still poster) | video ON | delta (gate <= +0.5) | anon delta |
| --- | --- | --- | --- | --- |
| steady | 78.99 | 78.45 | -0.54 | +0.05 |
| paused | 79.00 | 78.46 | -0.54 | +0.07 |
| released | 78.75 | 79.16 | +0.41 | +0.07 |
| resumed | 78.86 | 78.47 | -0.39 | +0.08 |

| Renderer `bead-wallpaper-video` (separate process, not in the UI budget) | PSS MiB | CPU |
| --- | --- | --- |
| playing 1080p30, software decode + render | 107.0 | 23.7 % of one core |
| paused (decoder kept, first 30 s) | 107.1 | 0.0 % |
| released (decoder freed, last frame kept) | 11.8 (anon 3, file 5, last frame 4) | 0.0 % |
| resumed | 105.3 | 25.0 % |

The idle renderer before any source is 2.5 MiB. Three release/resume cycles stay at 12.6-13.0 MiB released (no
growth), so the `dlclose` + `malloc_trim` release is stable. The small 108x108 smoke clip plays at 0.3-1 % CPU.
The CPU figure is software decode in a container; on hardware `vaapi-copy` it is expected to be much lower, which
this host cannot measure (needs the owner's desktop, nested lock was not used). Not measured on real hardware:
battery draw, VA-API CPU, 4K.

## Tests

- `bead-wallpaper`: policy state machine (hold/hide timing, escalation keeps the clock, battery/cover rules are
  config, stopped/force-visible, group aggregation, removed outputs); protocol round trip and unknown kinds;
  supervisor: renderer killed and restarted with the desired state replayed, crash loop abandoned without spin and
  retried on a new source, missing binary, clean stop.
- `nacre-shell`: settings decode with QML defaults and wrong types; cover/fullscreen/session/battery to policy
  inputs; motion member validation (symlink, replaced file, bad kind/path, stale); a fake daemon over the real
  `nacre_source` client starts the renderer with the source and a play state and stops it when the member is
  removed.
- `spikes/wallpaper-video-smoke.py` (CI step): real renderer under headless sway with an ffmpeg-made clip - crop
  fill, playing, pause freezes the frame and CPU, release keeps the last frame and drops memory, resume, three
  release/resume cycles, bad source reports an error and survives, killing the renderer leaves the compositor and
  the fallback visible.
- `spikes/wallpaper-video-shell-proof.py`: the relative gate against a control shell (below).

## Packaging and CI (B0029)

`bead-wallpaper-video` installs beside `nacre-shell` (`stage.sh`, `bead-shell check`, stage test); the Arch package
depends on `mpv` (libmpv); the Ubuntu CI image adds `libmpv2 ffmpeg sway grim python3-pil` for the smoke step. The
renderer is optional at runtime: if the binary or libmpv is missing the shell logs `WALLPAPER_VIDEO_DISABLED` /
an error event and keeps the static wallpaper.

## Temporary duplicates

None.

## Follow-ups (not in this unit)

- Nacre unit for the `motion` field (above) - blocks the daily switch.
- Dashboard Today "Pause wallpaper" button (writes `wallpaper-picker.paused` through `settings.set`); the
  renderer already honours the key.
- Zero-copy GL/dmabuf render path and fractional-scale-exact buffers.
- niri does not report fullscreen windows to the adapter (`fullscreen: None`); with the default `pauseCovered`
  every tiled window already pauses, so only `pauseCovered=false` + fullscreen on niri is not detected.
- Rotated outputs (buffer transform) and an explicit wlr-output-power DPMS signal.
