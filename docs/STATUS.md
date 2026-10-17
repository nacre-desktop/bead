# Status

## 2026-10-17 — Read-only service views measured; review boundary

Network, Bluetooth and battery popouts share existing hosted state through a
selected-field Source. No new backend or device action. Modules, payloads and
rasters load on demand; closed workers retire after 2.2 seconds, unused hover
preparation after 2 seconds. All 48 checked status closes drop their module/view
cache owners, leaving only the bar. Six populated scale 1/1.5 goldens pass.

| Five memory pairs + native 20 cycles | Hyprland | niri |
| --- | ---: | ---: |
| Settled UI medians, control → candidate MiB |23.205 → 23.354|23.756 → 23.952|
| Median paired delta / control spread MiB |+0.069 /0.294|+0.090 /0.188|
| Maximum candidate settled UI MiB |23.831|24.482|

All 394 close/settled observations (244 candidate) fit bar + 5 MiB. Both 25 MiB
ceilings, cache caps, stable process groups and cleanup pass. Total differences
fit control spreads; no saving or extra memory exception is claimed. Native20
UI + daemon/workers is 56.219/56.404 MiB, before combined notices/wallpaper.
213 workspace tests, isolated trace proof, fmt, warnings-denied clippy, CPU-only
and normal release pass. [CI](https://github.com/nacre-desktop/bead/actions/workflows/ci.yml).
[Measurements and scope](research/feature9-gate-d.md#lazy-read-only-service-views--2026-10-17).

Six matched 60-second QML status/header references and both native bar-click runs
pass. Bead open-status UI is 26.088–27.568 /26.980–27.052 MiB H/N; QML fixture
184.521–190.371 /176.366–180.194. Same unavailable panel text/geometry/tokens;
header data, shadows and service activity differ. Full service/hardware parity
is unmeasured. Both updated private preview smokes stop bar/notices/daemon cleanly.

## Typing remains unaccepted

The latest 800-key compatibility check uses the standalone CPU launcher, not the
bar+panel process: UI 9.963/10.922 ms/key, presented p99 30.886/31.562 ms H/N.
One pair establishes no gain or absence of a small regression. UI exceeds 8 ms,
so the two-frame candidate rule also does not pass. The earlier five-pair prefix
optimization reduced UI CPU by paired 0.583/0.575 ms; p99 16.7 ms remains open.
Stale queries still serialize. Keep the historical 29.61 → 32.66% regression.

Protocol 94b86fe: [CI 38997728436 green](https://github.com/nacre-desktop/bead/actions/runs/38997728436).
Pin 1e2d6755 and 19 typed frames pass; exact provider ACK precedes a held outbound
reply. Production advertises only ui.dispatch; actual executor/shared QML pending.
Source/restoration c13f2c9 and closed-panel cleanup 3fb011a are green. Calendar
follows today until browsed; unchanged Source wakes stage no scene. Transient
queries/month selection release on retirement. No settled saving was measured.

## Stop for service-contract review, before dashboard

The safe view subset is integrated; B remains incomplete. Full AP/pairing,
mic/device/routing, power and daemon tray/notification hosting need contracts.
Updates/session completeness and same-engine QML remain open. Nacre main was
fetched at e0a81ce7; network.watch is a trigger, not an AP/action model. Backends
stay frozen. The next dependency decision is post-F4 service contracts/hosting,
or an explicit narrower B. [Review and forecast](research/feature9-gate-d.md#service-view-review-boundary--2026-10-17).

Core forecast: 40–81 eight-hour agent-days (320–648 task hours), 16–41 active days
at the assumed 16–20 task-hours/day scenario; roughly 3–8 calendar weeks including
coordination/hardware waits. Full parity remains 102–207 days, roughly 7–18 weeks.
Complete-core idle 90–150 MiB is an estimate, not a measured combined shell.

Allowance is 0.5 MiB per explained feature. Every close stays within bar + 5 and
settled UI ≤25 MiB on both compositors. Panels remain lazy; no new ceiling needed.
QML stays daily driver. Keep 6b niri pacing/strict idle and long-run chaos open.
CPU for small panels; GL for large animation (~20 MiB Mesa residue). Desktop plan
is HOLD; no live/install/config/unit/keybinding action, dashboard or Feature 11.
Further IME/reader work is deferred; the binary/control-script name clash remains.
