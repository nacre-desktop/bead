# Status

## 2026-10-17 — Closed-panel cleanup verified

The bar already loads only its own bundle. Panel modules, icons, stores,
subscriptions and rasters release with the worker after the 2.2-second close
grace; unused hover preparation expires after 2 seconds. The launcher scalar
mirror now clears too. The application-only 4 MiB mmap policy fixes an older
large-raster residency failure; no framework allocator/font rewrite.

| UI MiB, five alternating pairs | Hyprland before → cleanup | niri before → cleanup |
| --- | ---: | ---: |
| Third warm median |48.102 → 38.350|47.032 → 45.603|
| Settled median |22.901 → 22.954|23.398 → 23.490|
| Paired settled delta / control spread |+0.147 / 0.300|+0.097 / 0.735|
| Maximum candidate settled |23.120|23.924|

Both 20-cycle native proofs pass, including two unused preparations and quick
reopen. All 158 candidate closes fit own bar+5; cache/group/cleanup guards pass.
One paired control fails six close budgets and remains valid=false/exit1 in the
data. Candidate warm ranges vary; the paired-median gate passes, not a uniform
peak reduction. No settled saving is claimed. Nested CPU only, scale 1.5,
H virtual 120/N 60 Hz; refresh unknown, host uncontrolled, no own build overlap.

193 tests, isolated trace allocator proof, fmt, all-feature clippy and CPU-only
build pass; [CI workflow](https://github.com/nacre-desktop/bead/actions/workflows/ci.yml).
Five CPU pairs give UI paired −0.075/+1.364 ms H/N, within control spreads
4.153/3.033. Both UI and daemon are slower in this window. No typing gain or
acceptance: targets remain UI 8 ms/key and presented p99 16.7 ms. The older
29.61→32.66% typing regression remains on the optimization list.
[Full measurements, ranges and method](research/feature9-gate-d.md#closed-panel-residency-audit--2026-10-17).

Next: test/fix calendar Source wakes and date-follow behavior, drop unused bar
startup-state copies, then measure the ranked-prefix typing draft. These drafts
are untested and outside the frozen cleanup captures. Check this checkpoint’s CI
before further integration. Dashboard has not started.

## Core-switch scope and standing rules

Owner accepted recovery/static-text (~0.4 MiB settled) and calendar/sound/routing
(+1.121/+0.753 MiB H/N). Feature allowance is 0.5 MiB per explained feature,
not per commit. Pure optimizations preserve warm/settled control spreads;
every closed-idle UI sample fits own bar+5 and settled UI bar stays ≤25 MiB.
Closed panels load on first use and release after idle. Propose a reasoned ceiling
if 25 MiB becomes a blocker after cleanup; no new ceiling is requested now.

Continue daemon typing and activation/reconnect/UI-provider contracts, then
remaining bar popouts/service views. STOP for review after those, BEFORE dashboard.
Nacre's additive typed action contract is published but its CI repair and actual
executor remain pending. Keep the validated protocol and frozen measured helper;
never build launcher ranking/providers or invent service payloads. Network watch
is only an availability/change trigger; full AP, audio, Bluetooth, power and tray
contracts/hosting remain unavailable. Calendar/sound/routing are not full parity.

Core estimate remains 40–81 eight-hour agent-days (320–648 hours), roughly
3–8 calendar weeks at an assumed 16–20 task-hours/day including waits. Always-on
90–150 MiB is a forecast, not a measured full core. On-demand QML Settings, Colors,
Welcome, AI and software windows stay in that plan. Update it after service views.
[Remaining parity and forecast](research/feature9-gate-d.md#gate-e--core-switch-active-2026-10-16).

## Accepted gates and boundaries

Gate C/optimization and Gate D are accepted for continued work. Bounded partial
repair retains the accepted exact-pixel behavior. Preserve 6b's open nested-niri
pacing and strict idle-invariance items. CPU for bars/small panels; GL only for
large animated surfaces, with the measured ~20 MiB Mesa residue finding.

QML stays the daily driver. The private preview still pins the earlier accepted
runtime until updated and smoked. The desktop [validation plan](research/gate-e-validation-plan.md)
is HOLD, not run; earlier physical permission is consumed. Further IME/reader
work is deferred to public release. No live desktop/install/config/keybinding,
notification-service takeover, Feature11 or package/release publication.
The nacre-shell installed-name clash remains open for packaging.
