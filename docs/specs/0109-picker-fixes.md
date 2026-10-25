# B0109 — Wallpaper picker fixes

Status: done as a review candidate; memory gates fail with a documented smoothness trade.

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
MOV/M4V/AVI clips at several resolutions. QML and Bead use the same daemon, media,
palette, 2880×1800 geometry and scale 1.5. The nested parent advertises 60 Hz;
these clocks do not measure physical scanout. Bead UI rendering is CPU and its
video worker uses EGL. QML uses Qt's default backend; the driver does not force
its renderer with `--cpu`. Both memory variants use the same Bead CPU flags.
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

The Settings picker action exposed two additional cold-path defects: release-only
kit actions were not dispatched by the Settings host, and pruning an unavailable
image left retained layer/clip indices stale. Release now follows the Session
input contract. Generic `Scene::retain_primitives` remaps all indexed metadata;
a CPU preparation regression covers nested layers, clipping and reveal parts.
Integration with B0110's longer retention exposed closed fullscreen layouts
holding keyboard/pointer input without motion metadata; a pending frame could
also defer release, and a closed transaction could reacquire focus. The final
tree uses B0119's independently integrated keyboard-mode/input fix, which
supersedes the local B0109 repair. It releases input immediately and prevents
warm publications from stealing focus. Toolbar-then-Settings is covered by the
integration refresh.

Matched single-key samples, milliseconds after synthetic input:

| Stage | QML before | Bead before | QML final pair | Bead final pair |
| --- | ---: | ---: | ---: | ---: |
| Background t10 / t50 / t90 | 245 / 266 / 305 | 806 / 822 / 838 | 307 / 342 / 375 | 154 / 164 / 174 |
| Active-topic receive | 1,206 | 723 | 782 | 807 |

These are individual cold samples, not population percentiles. Background progress
is normalized L1 image distance in a clear backdrop crop, reduced 8×. Striped-card
image distance is not a useful motion clock: changing overlap aliases the pattern.
Instead, the paired frame series and configured clock demonstrate 300 ms OutCubic
carousel travel; retargeting, wrapping and matching hit geometry have unit coverage.
Final numeric marks put admission at 38 ms, bounded image decode at 39–139 ms and
first surface commit at 158 ms. RPC confirmation arrives at 1,297 ms; it no longer
delays the preview. The three-key burst lands on its final selection.
Motion references and native paired screenshots are in [evidence/0109](evidence/0109/).
`final-capture-metadata.json` identifies refreshed states and nearest frame times;
`capture-metadata.json` retains the earlier staged source pins. Neither screenshot
spawn clocks nor surface commit marks measure physical presentation.

For MP4 / WebM / MKV / MOV / M4V, candidate key-to-first-video-frame marks are
258 / 32 / 75 / 36 / 40 ms; admission is 4–6 ms. These marks indicate the first
new mpv frame submitted by the renderer, not compositor scanout. The first MP4
also creates the player; subsequent formats reuse it. Source-specific colour
markers and two changing full-window frames verify replacement identity and
actual playback, rather than accepting an unchanged old video. Return activates
the selection again through the existing public picker contract; both requests
complete, and the preview remains continuous while palette work finishes. A final
cold WebM refresh submits its first frame at 327 ms. AVI (the sixth format accepted
by the helper) also passes the paired identity/playback check with a generated
960×540 MPEG-4 clip. Cold and reused-player figures are kept separate.

Known parity limits: CPU font shaping and thumbnail sampling differ slightly;
the Bead Settings title/section spacing inherits its existing host layout.
Current scene additionally displays the requested filename and kind. Spotlight side cards are slightly brighter; fullscreen
backdrop/side-card transitions and in-panel dynamic posters retain B0100's gaps.
Video samples are not synchronized to a common media clock, and the Bead video
composition has a small colour/lightness difference (a saturated marker can
acquire about 39/255 in another channel). Identity checks allow 64/255 per channel;
the markers differ by 255 in at least one channel. These are not pixel-parity or
physical smoothness acceptance claims. No live desktop or laptop-cap test ran.

Source pins: QML/helper `713d38b3`; before-timing Bead `4f8db4cf`.
The original staged timing/format series uses `c8f96ac`; final layout, travel,
Settings-after-toolbar and WebM/AVI pairs use `7edbd65`. The final raster-admission
repair `c37951c` changes cache limits/validation only; its combined six-state
functional refresh passes with no UI errors. The final memory cohort compares
`2331552` against `c37951c`, including B0110's 120-second retention in both arms.
The final `17a4559d` tree includes independently published widget/reconnect,
status-pill, B0119 focus/title, B0111 sidebar/dashboard and B0117 renderer changes.
Full remote CI, default-parallel renderer tests (54 passed), and a fifteen-state
integration refresh pass. The refresh covers the combined layout sequence,
Current scene with an active movie, actual buttons/action, travel and AVI.
Upstream changes are not attributed to B0109 in the frozen paired measurements;
B0119 supersedes the local focus repair. The receipt is
[integrated-refresh.json](evidence/0109/integrated-refresh.json).
CI uses the Arch container required by the task and Rust 1.99.1; the repository
job specifies Ubuntu / Rust 1.99.0. No host package, configuration or installed
shell was changed.

The pre-rebase five-pair diagnostic cohort (`718ca83` / `c8f96ac`) fails the
relative gate: closed paired medians are +0.578 MiB idle, +0.734 carousel,
+0.586 Spotlight, +0.688 Hexagons, +0.594 Dynamic and +0.957 after Settings.
[Raw measurements](evidence/0109/memory-pre-integration.json) preserve these
failures. They do not validate the later 120-second retained-panel lifetime.

A combined new-base control and candidate both failed the existing raster
admission cap, so neither run is an accepted memory arm. The trace shows
47.8 MiB of Spotlight rasters still held while staging Hexagons, against a
64 MiB owner cap; total logical process cache use was below 49 MiB.
[Admission samples](evidence/0109/cache-overlap-before.json) are logical charges,
not physical memory. Evicting lookup entries cannot release a frame still held
by the worker/presenter. Raising the cap without a steady-frame check would
allow oversized imported/reveal frames and weaken rollback.

The renderer now allows 128 MiB of **charged old/new raster overlap**, while
retaining the 64 MiB steady-frame/normal-entry-cache limit and unchanged 192 MiB process
limit. Every old allocation remains charged until its last reference drops.
Normal, imported and revealed candidates are checked before replacing the last
good frame; entry-cap rejection also preserves it. Frame totals conservatively
count repeated raster references, so very large repeated-image frames can be
refused even when their pixels are shared. Remote renderer tests cover
large normal replacement, last-reference retirement, reuse, oversized import
rejection and the 1,025-entry rollback boundary (52 tests passed). Final matched
memory cases use fresh sessions per layout to avoid the control's cross-layout
admission failure; budget, containment, swap and UI-error gates remain active.

## Memory cost and acceptance limits

All 50 arms pass containment, geometry, zero swap-in/SwapPss, and UI-error checks.
The [complete cohort](evidence/0109/memory-final.json) preserves five alternating
pairs per phase (25 paired idle samples). Values below are **paired median
candidate minus control**, in MiB; every +0.5 MiB relative phase gate fails.
Both arms use the Bead CPU backend. Closed samples are warm, about 9.5 seconds
after close, before B0110's shared 120-second retention expires; these do not
measure final post-retention release. Absolute laptop-cap acceptance is unmeasured.

| Phase | Open unreclaimable Δ | Closed warm Δ | Closed anonymous Δ | Closed clean-file Δ (info) |
| --- | ---: | ---: | ---: | ---: |
| Idle | +0.629 | +0.621 | +0.621 | -0.043 |
| Carousel | +1.559 | +1.156 | +1.156 | -0.122 |
| Spotlight | +0.559 | +0.777 | +0.777 | -0.057 |
| Hexagons | +0.547 | +0.617 | +0.957 | +0.307 |
| Dynamic | +10.797 | +11.301 | +1.414 | -0.009 |
| Current scene | +0.586 | +0.586 | +0.586 | +0.093 |

Unreclaimable = Pss_Anon + Pss_Shmem + DRM + SwapPss. Full PSS and clean-file
metrics remain informational and are included separately in the JSON. These are
stable costs, not a gate pass obtained by dismissing a single arena outlier.
The new Settings process costs +2.617 MiB while open (26.385 / 29.010 MiB
control/candidate unreclaimable medians), and exits after close in all ten arms.
The separate EGL video worker measures −0.055 / −0.066 MiB paired open/closed
deltas, with 329.400 / 329.342 MiB open control/candidate medians including DRM;
its retained player/context policy is inherited from B0099.

Dynamic's main-process SHM delta is 10,124 KiB in every pair. This is consistent
with one additional 2880×1800 RGBA slot shared with the compositor (about
10,125 KiB proportional share). The early preview commits an additional frame
before the previous slot releases. Both builds log `reason=no_release`; existing
wallpaper cleanup retains the whole two-slot pool until all buffers release.
The buffer-count attribution is an inference from this code path, logs and the
exact SHM step, not a direct count probe. The early fallback avoids a blank or late first movie frame; the spare-slot
residue follows the current pool retirement policy. No live buffer is discarded
to make the gate pass.

A separate [guarded allocator diagnostic](evidence/0109/heap-attribution.json)
uses one trace-enabled pair per Carousel/Dynamic phase, not the gate cohort.
After close, live heap increases only 114 / 134 KiB and logical cache bytes
3.3 / 1.8 KiB. Free allocator backing increases about 5.0 / 3.5 MiB, while
resident anonymous memory increases 1.18 / 1.48 MiB; mapped-heap bytes are unchanged.
This is consistent with allocator backing explaining most of the anonymous cost,
rather than an equivalent increase in live view/cache data. These aggregate
counters do not attribute individual resident pages, and do not turn the failed
five-pair gate into a pass. Temporary instrumentation copies were removed.

Under the updated **SMOOTHNESS FIRST / SPEND** instruction, this is a review
candidate with a measured memory trade: animated card travel replaces snapping,
and image-background t10 drops from 806 to 154 ms in the recorded cold sample.
The failed memory gates remain visible; no memory or physical-smoothness
acceptance is claimed.

The final renderer integration preserves B0117's edge-template warmup and
revealed-pose cache. Cached poses were already validated; new candidates are
validated before fallback retirement/swap. Edge-template, reveal and normal
raster allocations all use the charged live cap; B0117 retains at most two
edge templates. A Luna integration review
found no concrete regression. The frozen memory cohort predates B0117; no
post-B0117 five-pair memory acceptance is claimed.

No dependency crates were added. Tokio's two-worker runtime feature is enabled
only for tests: the fake daemon must remain schedulable during synchronous UI
staging, without relaxing its one-second method deadline. CI prerequisites mirror the existing
job (Rust/Clang/CMake, Wayland/fontconfig/xkbcommon/PipeWire/GDK/WebP,
mpv/ffmpeg, sway/DBus/grim and Python/Pillow). Desktop runtime packages were
unpacked privately: mpv, MuJS, LuaJIT, sixel, XPresent and patchelf.

Reproduce on the authorized desktop, with the shared lock around each command:

```sh
nice -n 10 python3 spikes/picker-fixes-proof.py \
  --nacre "$NACRE_SOURCE" --bead-bin "$BEAD_BIN" --bead-shell "$BEAD_SHELL" \
  --out "$EVIDENCE" --arms qml,bead --cpu --size 2880x1800 --scale 1.5 \
  --wallpapers 10 --wallpaper-videos 6 --wallpaper-fixture --presentation "" \
  --states wallpaper-button-spotlight,wallpaper-button-hexagons,wallpaper-button-dynamic,current-scene-picker,wallpaper-travel,wallpaper-format-webm,wallpaper-format-avi
```

The phase cohort uses `--arms bead --states idle,memory-PHASE`, replacing PHASE
with carousel/spotlight/hexagons/dynamic/scene; five alternating control/candidate
pairs per phase, fresh private HOME/session per arm, `--settle 0 --closed-settle 2.5`
and the case's seven-second close wait. It uses five fixture videos. Parent output
and virtual-pointer tool paths are supplied by the existing private capture setup.
