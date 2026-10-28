# B0173 — Wallpaper picker cold open

Status: implementation and paired QML/Bead parity are complete. New lead live
evidence on the owner laptop shows every Super+W open taking 4.0–4.6 s and
popping in without a slide on stable Bead 900369fb and 86efa7e8. The earlier
acceptance arm measured from `PanelVisibility`, and its open trigger used the
Bead command socket; it therefore did not cover the `nacrectl ui` route or the
delay before panel admission. Source inspection found a full-catalogue request
on every reopen despite valid cached rows; the guard now skips that child when
the current daemon boot already has a catalogue. The three-open same-session
route measurement is pending. The final no-shim selection and CPU gates also
failed; the cold first-frame gate remains unmeasured after desktop preflight
rejected it for host swap use above 64 MiB. Full Bead CI and both pushes remain
pending.

## Problem and baseline

B0171 recorded a 2.6 s first open in a nested 2880×1800, scale-1.5 session and
about 4 s live; warm reopen was 17–24 ms. The opening path waited for catalogue
metadata and the worker used `wallpaper.catalogue {ids:[]}` to enumerate IDs. A
separate warmer issued the same full-catalogue request during shell startup.
B0171 selection-to-presented was 61 ms p50, 89 ms p95, 206 ms max across three
cycles of five selections; the max was the first click 1.5 s after a cold open,
before prefetch completed.

The lead's 2026-10-28 screen recording establishes a separate regression: with
stable Bead 900369fb and 86efa7e8, Super+W shows the wallpaper picker after
4.0–4.6 s on every open and it appears in one frame. Earlier task measurements
started at panel visibility and opened through Bead's private command socket,
so those results cannot be used as evidence for this end-to-end route.

## Implementation

- `content_ready()` no longer gates the panel commit. The first frame can show a
  themed loading message while rows or thumbnails arrive; the panel can slide
  without waiting for catalogue or image work.
- A bounded, versioned JSON metadata snapshot lives at
  `$XDG_CACHE_HOME/bead/wallpaper-catalogue-v1.json`. Atomic writes use a
  same-directory temporary file and rename; reads reject oversized, malformed,
  wrong-version, symlink, and invalid-entry files. Cached Nacre thumbnail and
  preview paths are rebased to the active XDG cache root.
- A full catalogue scan is now requested only when the current daemon boot has
  no cached rows. Previously, every reopen set `wallpaper_opened=true` and
  respawned `wallpaper-media catalog --catalogue-ids` despite the retained
  catalogue; a close/reopen regression check asserts that valid rows do not
  issue another `wallpaper.catalogue {ids:[]}` call.
- Nacre's existing `wallpaper-media` service stores the thumbnail and preview
  image bytes. The Bead snapshot stores metadata and those paths, not image
  bytes. A valid locally loaded visible raster skips catalogue/prewarm RPCs;
  only a visible cache miss requests that ID, and the reply progressively fills
  the view and refreshes the snapshot.
- `wallpaper_warm` consumes the saved snapshot instead of issuing a second full
  catalogue query. The cancellable, one-decode display-raster prefetch waits
  100 ms after the first frame and now runs at nice 19 so activation work at
  nice 10 gets CPU first.
- Reopening a retained panel whose layer role was unmapped recreates its surface
  before rendering. The retained-panel CPU snapshot starts at 1.5 s after close;
  its work completes before the 2 s CPU-idle boundary in measured runs. Epoch,
  close-epoch, UI-serial, scale, and `panel_target` checks discard stale snapshot
  results if the panel reopens while the worker runs. The existing view-release
  timer is unchanged.
- The daily acceptance harness can reuse a task-private HOME/XDG root for cache
  priming and measurement while starting a fresh shell, daemon, and compositor
  each time. Its runtime directory can be separately placed on a short path for
  UNIX socket limits. CPU evidence sums per-thread ticks, reports every raw
  interval, and compares late thread ticks with a five-second pre-open baseline.

Generic additions: persistent wallpaper metadata and an epoch-checked retained
panel CPU snapshot. No new renderer primitive or Nacre-specific engine
dependency was added. UI state, layout, color, and sizing remain in Luau/tokens.

## Measurements so far

All desktop runs use the private task workspace, owner's 17 staged `Static`
wallpapers (including the 7680×4320 PNG), a 2880×1800 output at scale 1.5, and
the CPU renderer. The wallpaper source path, inode, size, and mtime are stable.
The warm prime and measurement use the same private HOME/XDG config, state,
share/runtime, and cache roots, with fresh shell/daemon/compositor processes.
`wallpaper-media prewarm --catalogue-ids` over all 17 files returned
`{"prepared":17,"computed":0}` (exit 0), confirming palette marker hits.

| Arm | Open-to-first-feedback ms | Unique selections, p95 ms | CPU after close | Pss_Anon after 120 s |
| --- | ---: | ---: | --- | ---: |
| Warm stable HOME, candidate nice10 helper | 64.685 / 49.438 / 49.383 | 15 samples, 113.626 | 2/3 cycles pass; one late `nacre-link` tick | 64.320→61.020 MiB (-3.301 MiB) |
| Warm repeat, candidate nice10 helper | 71.270 / 49.493 / 49.413 | 15 samples, 124.101 | one shell tick at 2.051–2.101 s and one `nacre-link` tick at 2.201–2.301 s | 56.027→60.223 MiB (+4.195 MiB) |
| Diagnostic only, task-private nice5 shim for `wallpaper-media select` | 79.665 / 49.515 / 49.337 | 15 samples, 114.302 | 3/3 cycles pass with five-second baselines; no post-2 s ticks | 67.414→64.133 MiB (-3.281 MiB) |
| Final warm stable HOME, no shim, Nacre helper 71b1ff35 | 80.650 / 49.456 / 49.226 | 14 samples, 161.836 | 2/3 cycles pass; one unmatched `nacre-link` tick at 2.201–2.301 s | 57.410→59.996 MiB (+2.586 MiB) |

The selection sample is the first presentation feedback for each accepted output
revision (five distinct revisions per cycle); duplicate feedback for the same
revision is counted separately. The 15 accepted samples use nearest-rank p95,
so the displayed aggregate p95 is the largest value. The nice5 row uses a
temporary task-private `nice` shim to test the one priority change; it is not
the product/helper acceptance result. The two earlier no-shim runs met the p95
limit once and missed it once by 4.101 ms. The final no-shim run with helper
71b1ff35 missed the gate: cycle counts were 5, 5, and 4 presentations, with
per-cycle samples [148.114, 161.836, 123.658, 157.748, 127.385],
[108.870, 108.120, 95.920, 98.820, 106.741], and
[116.596, 99.914, 109.491, 148.288] ms. Combined p95 was 161.836 ms, so both
the five-per-cycle minimum and <120 ms gate fail. The warm first-frame target
passes in all three cycles. A priority-0-only diagnostic was worse: 12 samples,
aggregate p95 785.754 ms; it does not justify a priority change and its shim was
removed.

Panel snapshot timing from the nice10 candidate arm: `PANEL_SNAPSHOT_TRIGGER`
at 1.008 s after the CPU-window anchor, snapshot completed in 8.124 ms, and
`PANEL_GPU_SUSPENDED` at 1.022 s. Thus the delayed tick in cycle 2 was not the
snapshot worker; it was one `nacre-link` tick, with no wallpaper UI/worker
activity. One five-second baseline observed that same link TID twice.

The first implementation's warm p95 failures were 148.745 ms with a fresh HOME
and incomplete Nacre cache keys, then 135.385 ms with the stable HOME. After
skipping helper requests for successfully cached visible images, warm p95 fell
to 113.626 ms in one arm. A repeat exposed a single 124.101 ms activation delay.
The bounded nice5 diagnostic made that delay disappear, but priority 0 made it
worse. The final no-shim arm used Nacre helper 71b1ff35, whose desktop CI passed;
its measured p95 still failed as detailed above.

The diagnostic child trace recorded 10 `wallpaper-media select` children at
192–228 ms, three startup `catalog` children, no `prewarm` child, no catalogue
or prewarm overlap with selections, and no observed flock wait. The trace
wrapper adds scheduling overhead, so these durations are diagnostic only. A
cache-key audit confirmed all 17 exact preview thumbnails, previews, and
source-matching palette markers are present in the stable warm HOME; prewarm
returned `{"prepared":17,"computed":0}`. The final warm log shows no Secret
Service timeout: `ksecretd` was started on the private D-Bus bus with task-only
HOME/XDG roots and no host keyring connection.

The slow selection phases spend 93–127 ms between input and the shell's first
`decode_start`, while image decode itself takes about 0.1 ms and upload plus
presentation adds about 20–30 ms. Bead waits for the matching active wallpaper
state, which the Nacre helper publishes after its asynchronous selection child
completes. The remaining delay is upstream of Bead's cached-raster upload.

The earlier warm first-frame boundary was `PanelVisibility(true)` to the first
matching Wayland presentation-feedback callback; it passed at 80.650/49.456/49.226
ms but omitted the route-to-panel delay. The new boundary is Super+W's
`nacrectl ui nacre launcher ">wallpaper "` invocation to the first matching
presentation feedback; its three-open measurement is pending. The empty-cache target remains unmeasured. The first
cold attempt was interrupted when the desktop powered off after a short
power-key press. A later general acceptance run failed in the unrelated
dropdown/image-diff phase before reaching wallpaper. A dedicated open-only mode
initially used IPC, which timed out; it was changed to the actual Super+W
virtual key. The key-path attempt was rejected before session start because
host swap use was at least 64 MiB; after a 20 s wait it rose from 116 MiB to
808 MiB with unrelated Steam web-helper processes holding swap. No host swap
reset or further desktop run was performed. The 5 s pre-open tick windows and 2 s
post-close windows, including all TID/name counts, are preserved in the warm
evidence. The shell retains no warm daemon. Catalogue and display-raster
snapshots are disk cache; shell Pss_Anon is measured separately from them.

## QML and Bead parity

The final `.44` scrim, `.5` Spotlight hero width, `1000` maximum card width,
and `.62` side-card fraction match QML tokens. The final paired desktop capture
has 20 matched states: idle; launcher open, hover, pressed, and Wallpapers
sub-page; wallpaper carousel open, hover, pressed, search, Spotlight, Hexagons,
Dynamic Spotlight, Dynamic Hexagons, Empty, Video Activated, Dynamic,
Activated, and the Carousel, Spotlight, and Hexagons settled states. Same-shell reopen
was checked before pointer input; both shells show the picker layer mapped.

Paired screenshots and open/close frame strips are in
[the parity evidence](evidence/0173/parity/index.html). Per-state RMSE ranges
from 1.295% (idle) to 4.762% (Hexagons); Spotlight is 4.196%, Dynamic Spotlight
4.436%, Dynamic Hexagons 2.340%, and Video Activated 4.103%. The final
Spotlight geometry and scrim match QML; remaining pixel differences are mainly
renderer/antialiasing edges. The video parity fixture is a still frame generated
from the same wallpaper, so animated-video playback timing is not claimed.

## Acceptance and CI

| Check | Result |
| --- | --- |
| Warm repeated opens in one session, each <150 ms from `nacrectl ui` invocation | PENDING: corrected route boundary |
| Earlier warm `PanelVisibility`-to-frame sample | 80.650 / 49.456 / 49.226 ms; superseded for end-to-end latency |
| Fully-cold first content frame <300 ms | UNMEASURED: desktop safety preflight rejected swap use ≥64 MiB |
| Warm selection-to-presented p95 <120 ms | FAIL: 161.836 ms; 14 samples, cycle 2 has 4/5 |
| Shell picker work idle by 2 s after close | FAIL: cycle 2 one `nacre-link` tick at 2.201–2.301 s, absent from 5 s baseline |
| QML/Bead paired states | PASS: 20 matched states |
| Full Bead CI | Pending |
| Nacre desktop CI for N0089/N0091 | PASS for 71b1ff35, reported by the lead |

The final warm raw report, shell/daemon logs, per-cycle phase logs, five-second
idle baselines, close-event logs, build hashes, and cache identity are in
[acceptance evidence](evidence/0173/acceptance-final/). The cold-arm preflight
report, memory counters, and post-wait swap snapshot are in
[cold preflight evidence](evidence/0173/acceptance-final/cold-preflight/).

No duplicate product implementation remains in the repository. The C `nice`
tracing wrapper and nice-0 shim were task-private diagnostics; the nice shim
was removed after its logs were saved. The private Secret Service override and
`ksecretd` supervisor remain only in task artifacts; delete them seven days
after the replacement acceptance harness is trusted. The new
`--wallpaper-open-only` path is isolated test support and does not replace or
duplicate the default full/live acceptance paths.

## Files

Bead changes are in `crates/nacre-shell/src/extras.rs`,
`wallpaper_catalogue_cache.rs`, `wallpaper_warm.rs`, and
`wallpaper_prefetch.rs`; the view remains in Luau/tokens. Reopen and snapshot
timing changes are in `crates/bead-wayland/src/panel.rs` and `rendering.rs`.
Acceptance/paired-capture support is in `tools/daily-acceptance/` and
`spikes/parity-compare.py`. Nacre's nice5 selection change is separate N0091
commit 71b1ff35.
