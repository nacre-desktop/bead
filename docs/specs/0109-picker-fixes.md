# B0109 — Wallpaper picker fixes

Status: implementation complete; integrated validation and guarded captures in progress.

## Causes and changes

- Spotlight, Hexagons and Dynamic used `settings.set` for picker-only preferences.
  These fields have no Settings registry descriptors, so the daemon rejects them.
  Use the existing `settings.provider` contract with the complete, revision-fenced
  document, preserving unrelated and unknown fields. Restore document version 1
  on writes: the published store deliberately strips it from snapshots. Reuse permanent topic sources
  when present; otherwise lease them for the picker lifetime.
- Carousel cards were placed directly at their final coordinates. Luau now tracks
  unwrapped circular travel and interpolates it for 300 ms with QML's OutCubic
  curve. The generic `ui.animate(key, target, seconds, curve)` primitive runs through
  Session wakeups: drawing, clipping and hit geometry share the interpolated pose.
  Reduced motion snaps; missing keys retire; failed staging restores animation state.
  At most 1,024 scalar keys are retained. Resolve animated values outside memo fragments.
- QML shows its selected preview before palette publication; Bead waited for the
  final atomic record. A shared, generation/boot-fenced preview now shows the
  catalogue's bounded image and starts identity-checked video/animation playback
  immediately. It retains the last confirmed palette until the real pair commits;
  activation failure cancels only its own generation and restores the confirmed
  scene. Metadata-only transition state keeps rapid selections continuous.
  Superseded decoding jobs also drain the latest coalesced request.
- Static wallpaper decoding already runs outside the UI thread and requests the
  output size. Avoid decoding the previous raster again when the surface already
  retains it for a transition. Existing visible-neighbour catalogue hydration
  prefetches bounded thumbnails/previews; no full-resolution image enters the view.
  Opt-in numeric marks separate selection admission/confirmation, decoding and commit.
- Video replacement reused the process and output surfaces but recreated mpv's
  render context. Reuse it with `loadfile replace`, clearing old frame notifications
  and waiting for the replacement's loaded/frame events before announcing a frame.
  Release, failure recovery and the last displayed fallback remain intact.
- Current scene was an unimplemented custom section. Settings now lazily follows
  `wallpaper.active`, hydrates only the current catalogue thumbnail, shows its name
  and Still/Motion kind, and opens the picker through `ui.dispatch`. The preview
  follows QML's 16:9/max-height geometry and rounded clip. Cache bytes retire when
  the row leaves the scene; reconnects invalidate hydration state. QML's current
  section offers only the picker action, so no unpublished Next contract is added.

Nacre and its protocol are unchanged. No dependencies or duplicate product paths
were introduced. UI layout and travel policy remain in Luau; the scalar primitive
is listed in ADR 0004.

## Validation

Private sessions use generated PNG/JPEG/WebP images (including 4K) and MP4/WebM/MKV/
MOV/M4V clips at several resolutions. QML and Bead use the same daemon, media,
palette, 2880×1800 geometry and scale 1.5. The nested parent advertises 60 Hz;
these clocks do not measure physical scanout. UI rendering is CPU; video uses EGL.
The harness extends the existing guarded parity driver, takes the shared capture
lock per command, checks swap/containment/window geometry, and retires its watchers.

Before: the real Spotlight toolbar click fails to persist its preference. Setting
preferences through the helper makes both fullscreen layouts and Dynamic work,
which isolates the failure from layout code and media classification.
One matched cold next-image sample reached the active-topic receiver in 1,206 ms
(QML) / 723 ms (Bead). These are receiver clocks, not renderer presentation times.
The baseline background image-progress proxy reaches 10–90% at 245–305 ms
(QML) / 806–838 ms (Bead), exposing the earlier QML preview despite its later
palette publication. Final timings, captures, CI and memory medians follow below.
