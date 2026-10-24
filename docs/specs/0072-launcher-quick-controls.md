# B0072 — Launcher Quick controls

Status: **Review — first-key CPU gate fails.** Implementation and evidence complete. Continues B0067 §4.

The launcher leases `controls`, `notifications`, and `dashboard.updates` from its
panel factory. The runtime, its subscriptions, rows, and pending calls retire
after close and idle. When the notice service already owns `notifications`, the
launcher borrows that existing topic and owns only its temporary callback.
The persistent bar acquires no new topic or row model.

`launcher.quick` projects DND, night light, and stay awake from the published
state, with honest unavailable/external-controller reasons. Activation uses
the control centre's existing reading, advertised-method guard, pending-call
slot, calls, and completion handling. State never flips optimistically.
`dashboard.updates.count` is an optional integer in Nacre `origin/main`
`2748720`; known zero is “Up to date”, unknown/stale/disconnected is hidden.
No Nacre contract change is needed.

The QML reference is `NacreAppBrowser.qml` / `NacreMenuButton.qml` at that
revision: 234-wide detail column at y=84, 13 padding, 8 spacing, compact
toggle rows, on/off text and switches. Shell Luau on kit components owns the
column, order, theme sizes and palette roles. It hides below the QML width
threshold and for calculations. Launcher opens on Home and rises from bottom.
The general Button options `activate_on_release` and `focus_on_pointer` let
shell code match QML release activation while preserving the search field's
focus; other buttons retain their defaults. `kit.Paragraph.advance` exposes its
existing text-width estimate so compact descriptions fit the reference rows.
A deferred press establishes its hover target even if the preceding hover
frame has not committed. These reusable options are recorded in ADR 0004.
No temporary implementation duplicate is introduced. CPU Rounded repairs also intersect the existing
conservative primitive bounds, reducing broad scratch copies for small controls.
The original masked painter is retained; existing pixels/goldens remain exact.

## Evidence and acceptance

Full Arch-container CI passed: fmt, clippy with warnings denied, service boundary,
workspace tests, trace memory-policy tests, renderer configurations, minimal and
shell checks, generated Luau schema/analyzer, spike and packaging suites, video
smoke, private services/tray, workspace and independent framework cargo-deny.
Fake-daemon tests cover all three methods/absolute values, state updates,
unavailable/stale/external state, pending-call deduplication, exact release
activation, retained search focus, and topic retirement. Existing permanent
notifications subscriptions survive launcher retirement. The three launcher
row goldens were regenerated and inspected; search/scroll retain Quick controls.
Private proof tools were staged beside the binaries. Container-only additions:
`wlr-randr`, `swaybg`, `wlr-protocols`, and Nacre helper build dependencies
`cmake`, `gtk3`, `librsvg`, `lcms2`, `libraqm`, `libjpeg-turbo`.

The private nested QML/Bead arms use the same presentation/settings and
2880×1800 at 1.5, with a verified 2880×1800 parent for pointer coordinates.
Bead uses CPU tiny-skia; QML uses its default Qt Quick backend and has DRM
allocations. The memory comparison below uses matched Bead CPU builds.
[Paired evidence](evidence/0072/launcher.jpg) is QML left, Bead right; adjacent
files cover idle, hover, and held press. The capture uses current-main's virtual
pointer helper. Unknown update counts and unsupported controls stay honest.
Whole-image RMSE was 2.447% idle, 4.166% open, 4.142% hover and 4.118% held
press. Sampled 10/50/90% progress was QML 152/164/295 ms versus Bead
135/167/300 ms opening; 105/136/225 versus 130/153/190 ms closing.
These are capture/dispatch estimates at 25 ms scheduling, not physical scanout.
See [metrics](evidence/0072/metrics.json).

Five alternating desktop pairs against `86c8d9a`, CPU tiny-skia, 2880×1800 at
1.5, 320 fixture apps, 60 serial `a`/BackSpace keys per typing arm. Values are
medians of run medians; first key is the median of five first-key samples.
The existing `typing-proof.py`/`measure.py` lane uses clocks-only profiling.
The existing B0069 `Run.open_panel` measures IPC-to-first-`PANEL_PRESENTED`,
two cycles per arm with 12 s quiet intervals; each arm uses its own source bundle.

| Metric, ms | Main | Quick | Gate |
| --- | ---: | ---: | --- |
| First key UI process CPU | 17.656 | 18.359 | **FAIL ≤8** |
| `a` / BackSpace CPU | 6.720 / 9.005 | 6.670 / 7.952 | PASS median ≤8 |
| CPU p50 / p99 | 8.332 / 17.656 | 7.286 / 18.359 | first-key failure remains |
| Presented p99 | 15.643 | 15.789 | nested callback timing |
| Launcher open | 29.978 | 30.001 | PASS paired delta −0.116; range −0.384..+1.174 |

The first-key CPU target already fails in main. The initial Quick build on the
previous `91a6227` base measured 22.966 ms; a diagnostic arm located most extra
cost in rounded composition (14.062 vs 9.770 ms), not topic projection
(0.201 vs 0.184 ms). Conservative primitive clipping removes broad scratch
copies for small controls. Mask elision was rejected because it changed edge
alpha by one; no pixel tolerance or golden was relaxed. The final first-key
result still fails acceptance; no broader renderer rewrite was added.

| UI PSS+DRM+SwapPss, MiB | Main median | Quick median | Paired delta median |
| --- | ---: | ---: | ---: |
| Initial bar | 10.764 | 10.919 | −0.053 |
| Shown cycle 1 / 2 | 47.852 / 48.595 | 47.780 / 48.143 | −0.099 / −0.625 |
| Closed cycle 1 / 2 | 9.080 / 11.195 | 9.018 / 11.170 | −0.032 / +0.044 |

Relative memory passes every phase (≤+0.5 MiB). The median final-close pair's
+0.044 MiB is +0.039 anonymous, +0.005 file-backed, zero frame-buffer change.
Maximum paired anonymous increase in any phase is +0.547 MiB (initial bar),
below the 1 MiB stop line. Independent category/arm medians need not add up.
The matched Bead CPU clients have no DRM allocations. Process SwapPss and host
swap-in are zero
in all twenty accepted headline arms and both accepted parity arms. One final
parity arm was discarded after one host swap-in page; a stable-host replacement
passed without resetting swap. Absolute laptop acceptance remains with the lead.

## Known parity gaps

- The private published controls omit usable stay-awake state and night-light
  capability. Bead shows disabled rows with “Unavailable right now”; QML's
  legacy source offers stay awake and an install recommendation for night light.
  The latter wraps to two lines, moving QML's following row down.
- QML's legacy updates provider reports 129; the published private
  `dashboard.updates` count is unknown, so Bead hides that row. Known counts,
  including zero, are covered by fixtures and goldens. The count is informational;
  QML also opens its updates item.
- Existing launcher typography/rasterization, search-focus border, empty-Home
  message placement and motion timing still differ. The bottom opening edge,
  Home route, duration/easing settings and surrounding frame are retained.
  Nested sampled timing is not physical scanout timing.
