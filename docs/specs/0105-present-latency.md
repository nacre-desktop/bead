# B0105 — pointer-to-presentation latency

Status: blocked. The latency target is unmet; the scheduling experiment was rejected.
Only the opt-in measurement probe and research evidence are integrated; runtime scheduling is unchanged.

## Result

At the B0103 private geometry, a newer-scene replacement reduced CPU drag median by only 0.26 ms.
It increased feedback-matched drag updates from 36 to 65. The closed-memory cohort also
contains an unexplained +1.516 MiB anonymous outlier, requiring the owner memory stop.
The queued native-GL candidate was stopped before its arm began. Missing measurements are not zeros.

All pointer values below are injection→niri presentation-feedback p50/p99 milliseconds.
Output is 2880×1800, scale 1.5; the canonical private headless parent is 60 Hz.
QML uses its normal threaded OpenGL renderer. Bead uses a CPU bar with the listed panel backend.

| Input / backend | Control | Rejected candidate | QML reference |
| --- | ---: | ---: | ---: |
| hover / CPU | 32.09/32.13 | 32.09/32.12 | 95.72/109.38 |
| drag / CPU | 66.45/77.15 | 66.19/71.62 | 98.13/103.68 |
| hover / GL | 32.09/32.12 | stopped | 95.72/109.38 |
| drag / GL | 66.02/78.58 | stopped | 98.13/103.68 |

Bead hover has 50 matched updates in every completed arm; CPU drag has 36/65 control/candidate,
and GL control has 35. QML has 50 hover and 39 drag samples. Coalesced input is reported in
[the numerical evidence](evidence/0105/measurements.json), rather than counted as a presented update.

## Per-stage breakdown

Cells are p50/p99 ms for matched CPU pointer updates; percentiles are not additive.
Host-routed pointer feedback does not call a Luau handler, so that separate stage is unavailable.

| Stage | Hover control | Hover candidate | Drag control | Drag candidate |
| --- | ---: | ---: | ---: | ---: |
| Injection → input dispatch | 0.28/0.39 | 0.29/0.43 | 18.27/28.08 | 18.22/23.60 |
| Dispatch → worker | 0.02/0.03 | 0.02/0.03 | 4.60/16.30 | 4.73/10.44 |
| Worker → local view | 0.02/0.05 | 0.02/0.05 | 2.84/3.66 | 2.68/3.57 |
| View → layout | 1.28/1.73 | 1.28/1.61 | 1.36/1.50 | 1.32/1.59 |
| Worker bake | 0.23/0.33 | 0.23/0.30 | 0.24/0.27 | 0.24/0.28 |
| Bake → accepted scene | 0.38/0.62 | 0.37/0.55 | 0.42/3.21 | 0.40/0.70 |
| Accepted → surface commit | 0.68/1.31 | 0.68/1.04 | 7.46/11.01 | 8.80/13.86 |
| Surface commit → feedback | 29.19/29.29 | 29.17/29.30 | 31.26/31.54 | 30.52/31.32 |

The candidate's whole-arm buffer-slot check is 0.001/0.002 ms, with 115 buffer-busy retries
that each schedule an 8 ms retry. These counts include animations and are not isolated to drag.
Existing rendering already draws on demand when no callback is pending, uses three reusable SHM
slots, and tracks CPU damage. The rejected patch adds callback/deferred/replacement markers and
buffer-acquisition spans. Attach/damage/commit CPU is inside the accepted→surface interval.
Callback pacing accounts for some of that interval; the roughly 31 ms post-commit interval remains.
GPU completion is inside the feedback interval and is not separately measured.

QML receipt→synchronization is 16.08/48.10 ms for hover and 16.01/16.04 ms for drag.
Its synchronization→feedback interval is 47.92/48.00 / 63.95/63.98 ms respectively.

## Why the capture cadence matters

The installed nested compositor is niri 26.04 (`8ed0da4`). Its
[Winit backend](https://github.com/niri-wm/niri/blob/8ed0da4/src/backend/winit.rs)
advertises 60 Hz independently of its parent and emits synthetic monotonic feedback after parent
submission, with unknown refresh. These timestamps do not report physical scanout of the nested window.

One diagnostic changed only the private parent from 60 to 120 Hz, keeping the same control binary,
frontend, geometry, scale and niri-reported 60 Hz mode. Hover became 16.08/16.12 ms and drag
became 35.16/55.32 ms. This indicates a substantial parent-cadence effect in this topology. It is one diagnostic arm,
not a repeatability proof, canonical-gate pass or physical 60 Hz measurement. The canonical run also measures QML
slower than Bead, so it does not reproduce the owner’s live QML advantage.

Qt 6.12’s [Wayland update path](https://github.com/qt/qtbase/blob/v6.12.0/src/plugins/platforms/wayland/qwaylandwindow.cpp)
coalesces updates while a frame callback is pending. The rejected Bead policy permits one newer
accepted scene to replace that submission, retaining the same callback; further updates coalesce.
It requires presentation feedback, preserving the callback-only persistence fallback. The focused
39-test Wayland suite covers stale callback rejection and the one-replacement budget. The
[rejected patch](evidence/0105/callback-replacement.rejected.patch) is retained as research evidence.

## Other gates and the stop

Three alternating populated-Home CPU pairs use 320 fixture apps, four keys, and separate private
sessions. First-key UI-process CPU medians are 7.313 → 7.323 ms; every candidate sample is below 8 ms.
No real-engine ranking/RPC latency is inferred. Unreclaimable is anonymous + shmem + DRM + SwapPss.
PSS and file-backed PSS are informational. Memory figures below are MiB and median paired deltas.

| Phase | Paired unreclaimable Δ | Control PSS/file | Candidate PSS/file |
| --- | ---: | ---: | ---: |
| idle | -0.035 | 25.959/19.641 | 25.725/19.316 |
| open | -0.156 | 41.715/20.311 | 41.458/20.183 |
| closed | +0.090 | 26.365/19.031 | 26.515/18.892 |

The medians satisfy +0.5 MiB, but closed paired deltas are +1.516/+0.004/+0.090 MiB.
The first anonymous increase remains unexplained. Full smaps are retained privately; allocator
variation is a hypothesis, not an accepted explanation. It triggered the stop despite the median.
Control/candidate idle CPU ranges both span 0–0.1% of one CPU over settled 10-second windows;
medians are 0/0.1%, at the one-jiffy measurement resolution. No increase beyond control spread
is resolved; exact idle equality is unproven. The absolute laptop memory cap is unmeasured.
Control-panel memory snapshots precede the stress drag and do not establish its post-drag gate.

One 25 ms image-sequence pair observes open-end 409→384 ms, unchanged first-change 108 ms,
and unchanged close-end 334 ms. This is consistent with no regression in that pair, not a
repeatability claim. QML open/close ends are 359/309 ms with the same approximate sampling.

## Evidence and source boundary

Runtime/control source is `ba2642df`; the candidate is that source plus the rejected patch.
Both use the same `ba2642df` frontend. Baseline/candidate binary SHA256 values and tool hashes
are in the JSON. QML is published `ab04e938`; probe source SHA256 is
`265f3c5a73fd1a28c20334643181bd4a1c2ad1e9cccd7a8faf852022ce83b8f6`.
Upstream B0101/B0102/B0104 subsequently landed; these frozen timings are not measurements of that newer runtime.
Only the probe and evidence are integrated on the newer source; final full remote CI passes.

The opt-in probe samples after [synchronization with the GUI thread blocked](https://doc.qt.io/qt-6.12/qquickwindow.html#afterSynchronizing).
Frame IDs join that exact numeric state to feedback. Hover requires positive actual feedback
opacity; drag requires the synchronized slider position within 1.1% of its pointer target
(the QML step is 1%). Hover-derived coordinate calibration is independent of the linear drag.
Traversal and synchronous logging are included in measured time. Screenshot latency is not
substituted for presentation. Initial loader and forwarded-window probe failures were rejected.

[Paired idle/open/hover/pressed and all four subpages, animation strips and residual gaps](evidence/0105/README.md)
preserve the matched wallpaper/palette and full-window geometry proofs. Canonical captures
passed source hashes, containment/watchdog, full-window and no-swap-in checks. Swap was zero
before every arm. No live desktop, installed shell, configuration or service state changed.

Rejected-candidate CI, final-source CI and probe build validation pass; details are in the evidence README. New measurement
build dependencies: Qt 6 Quick/Gui development and private QPA headers, plus wayland-protocols.
Framework dependencies are unchanged. Temporary product duplicates: none. Private harness
copies expire seven days after review; the rejected patch remains a historical evidence artifact.

The next attempt needs a capture path with a verified single 60 Hz cadence and a reproducible
live-equivalent lag comparison. The current evidence does not justify an engine latency fix.
