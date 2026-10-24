# B0074 — Dashboard first-reveal delay

Status: **Review; dashboard first-reveal median target met.**
Control: `51055ac`; candidate code: `19696b0`; QML: `f032c7b4`.
Scope: dashboard admission-to-presentation latency; launcher implementation,
Nacre and other surface artwork are unchanged.

## Causes and changes

The reveal clock previously started in the first drawable frame, after worker,
configure and presentation preparation. Its cold admission seed also used the
primary animation store rather than the panel store. Reveal panels now target
visibility in the panel animation store at admission. Repeated opens retain the
running target; prepared/reopened panels take the same path. Duration remains
360 ms with the existing cubic easing; layout and page content stay fixed.

The joined contour's SVG also rasterized a full-height transparent canvas on
every frame (12–14 ms in the control). `RevealArt` now returns its artwork bounds
alongside its SVG. The shell crops transparent space on a 120-logical-pixel grid,
keeping crop offsets integral in scale120 physical pixels. Scene bounds and
content clips stay fixed; full progress restores the original artwork. A test
compares the top-edge crop with the former full-height raster, byte for byte at
scale 1.5, including checking that omitted rows are transparent. It also retains
the existing corner, placement, asset-release and settled-frame checks.

No warm-worker grace period, cache retention or panel release policy is added.
General primitive: content-sized reveal artwork, recorded in ADR 0004.

## Measurement method

Private nested niri in headless labwc, 2880×1800 at scale 1.5, tiny-skia CPU,
release builds with `--no-default-features`, identical presentation/settings and
private daemon. Three alternating control/candidate pairs include QML, cold
`dashboard-0` and `dashboard-warm` (open/close/reopen). The latter can reuse its
worker: its 1.5 s close interval is shorter than the 2.2 s release timer. A new
worker on every warm reopen is not supported by this unit's trace.

`BEAD_PROFILE_INPUT=clocks` enables `UI_TIMELINE`: CLOCK_MONOTONIC nanoseconds
and a thread identifier, no user content. Disabled calls read no clock and format
nothing; they check the cached profiling flag. VM/generation/layout marks apply
to loading; `worker_layout_ready` also covers retained-runtime updates. Worker
paint readiness is preparation/baking, not a compositor commit. Each table entry
uses the first corresponding mark after admission, tracking its worker thread;
`panel_commit` is the first companion buffer commit, and `UI_PRESENTED` with
`companion=true` is its compositor feedback (clock_id=1). Transparent fallback
presentation and content presentation are distinguished in the evidence.
These are nested presentation/image-progress measurements, not physical scanout.

## Timeline

Medians in ms after panel admission (except the dispatch-to-admission row):

| Stage | Cold control / candidate | Warm control / candidate |
| --- | --- | --- |
| Dispatch → admission | 0.182 / 0.179 | 0.161 / 0.160 |
| Worker spawn | 3.659 / 3.628 | retained / retained |
| Runtime ready | 3.799 / 3.768 | retained / retained |
| Behaviour VM ready | 6.498 / 6.444 | retained / retained |
| First layout + projection | 15.711 / 15.708 | 6.544 / 6.064 |
| Worker paint preparation ready | 31.647 / 31.529 | 6.694 / 6.218 |
| First content buffer commit | 51.885 / 42.515 | 39.363 / 31.230 |
| Content-generation presentation | 55.465 / 53.458 | 41.452 / 41.063 |

Presentation of a content generation can still be clipped near zero reveal
progress; it is not the first visible glyph. Earlier accepted updates can be
coalesced, so feedback is matched to a post-admission committed update that was
actually presented. The retained warm worker prepares in about 7 ms; the late
motion clock and repeated full-height raster work postpone meaningful image
progress. Early cropped-frame
SVG preparation drops from roughly 12–14 ms to 3–4 ms.

## Results

Three alternating pairs on the integrated base (harness-derived ms, medians):

| Open | QML 10/50/90% | Control | Candidate | Candidate − QML at 10% |
| --- | --- | --- | --- | --- |
| Cold | 201 / 263 / 306 | 256 / 319 / 367 | 197 / 266 / 331 | −4 ms |
| Warm | 200 / 264 / 306 | 250 / 309 / 355 | 205 / 279 / 331 | +5 ms |

Both median 10% differences meet the ±20 ms goal. Warm per-pair differences
are +5/+4/+5 ms; cold differences −22/−13/−2 ms (one is 2 ms outside the
window). The 25 ms capture cadence and image-distance interpolation do not
establish an every-open latency bound. The 90% image proxy remains about
25 ms later than QML; the easing/duration were not changed.

On the earlier `86c8d9a` base, the exploratory clock-only
candidate reduced warm image progress from 250/308/355 to 225/283/331 ms
(medians, 10/50/90%); matched QML was 200/264/306 ms, so that change alone missed
the ±20 ms 10% goal. The full-height artwork cost motivated the second change.

## Memory and evidence

CPU UI PSS+DRM+SwapPss, three alternating pairs. After a 20 s settle each phase
uses seven samples at one-second intervals. Deltas are medians of paired phase
medians; component columns show the same pair supplying that total median.

| Phase | Total Δ MiB | Anonymous | File-backed | Shmem / DRM / SwapPss |
| --- | ---: | ---: | ---: | ---: |
| Idle | −0.029 | −0.101 | +0.073 | 0 / 0 / 0 |
| Open | −0.235 | −0.055 | −0.174 | −0.006 / 0 / 0 |
| Closed + idle | +0.262 | +0.254 | +0.008 | 0 / 0 / 0 |

All phase medians pass +0.5 MiB; anonymous growth stays below 1 MiB. The closed
increase lies in anonymous/file residency; no framebuffer or swap remains.
Exact allocator attribution of the small anonymous difference was not measured.
The per-pair anonymous component median is −0.191 MiB (component medians need not
add to the median total). This uses an unavailable-data dashboard without a daemon;
provider/daemon totals and the laptop absolute cap are not measured here.

The initial latency-harness snapshots averaged only two one-shot points per phase per run
and showed +1.115 MiB idle, entirely file-backed, with wide ranges. The existing
3 s code-page release makes those points phase-sensitive (B0065/B0069). They are
retained in the numeric evidence alongside the expanded phase samples, rather
than counted as a successful standalone memory gate. No product change occurred
between the two sampling methods; the executable grew just 776 bytes.

All accepted arms passed containment, watchdog, geometry and process SwapPss
checks. Host swap stayed below 64 MiB; no reset occurred. One arm recorded two
4096-byte swap-in pages (8 KiB), below the existing **1 MiB** quiet limit. A copied
wrapper's stricter zero-page assertion was corrected to the documented gate;
the completed arm and original counters were retained, without recapture.

[Numeric evidence](evidence/0074/metrics.json) includes all pairs, spreads,
binary hashes, phase samples and the initial sparse results. [Open comparison](evidence/0074/dashboard-open.jpg)
is QML left / candidate right; [warm reveal strip](evidence/0074/dashboard-warm-open.jpg)
is QML / control / candidate from the fixed middle pair. Full galleries and logs
remain private. Existing Today/weather availability and Qt text differences are
visible in the pairs; this unit changes the reveal path, not those contracts.

## Typing and checks

Three matched 320-app fixture pairs, 60 serial `a`/BackSpace edits per arm:
first-key process CPU **17.991 → 17.915 ms**, `a` **6.775 → 6.701 ms**,
BackSpace **7.962 → 7.930 ms**, CPU p50 **7.273 → 7.158 ms**.
No measured typing regression; the absolute first-key ≤8 ms gate **fails in
both builds**, as already recorded by B0072. Launcher files are outside this unit.

Full `.github/workflows/ci.yml` equivalent passed in a disposable remote Arch
container (Rust 1.99.1): fmt, clippy with warnings denied, service boundary,
workspace and trace `memory_policy` tests, both renderer feature configurations,
facade/headless checks, generated Luau/analyzer, spike and packaging suites,
wallpaper smoke, private services/tray, and root plus independent framework
cargo-deny graphs. The capture-manifest suite requires a clean Git snapshot;
validation used one. Arch font/swaybg/wlr-randr dependencies and Sway capability
removal match B0073; no host installation or live desktop changes occurred.

Temporary duplicates: none introduced.
