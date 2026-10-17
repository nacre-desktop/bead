# Status

## 2026-10-17 — Closed-panel audit: niri paired validation running

Main e8a02ae is accepted; CI38974187010 is green. The local cleanup releases the
launcher scalar mirror after the existing 2.2-second grace. Panel bundles,
workers, subscriptions and rasters already load lazily and release on idle.
The application-only 4 MiB mmap policy passes 193 tests, isolated trace allocator
proof, fmt, all-feature clippy and CPU-only build. It remains unshipped.
Both 20-cycle native proofs pass, including two unused preparations and quick
reopen; settled UI is 23.024/23.924 MiB H/N. All candidate close guards pass.
Five Hyprland memory pairs: warm third-open paired -9.562 MiB; settled +0.147
within the 0.300 MiB control spread. Five niri pairs are running. One H control
fails close budgets; data collection preserves valid=false/exit1, not a pass.
Five CPU pairs give paired UI -0.075/+1.364 ms H/N, inside spreads 4.153/3.033.
The current window is slower for both UI and daemon; no typing gain is claimed.
Calendar Source/date/reload and bar startup-state fixes are drafted separately,
as is a ranked-prefix typing candidate. Both are untested and outside captures.
Next: finish the memory matrix and verify all caps/guards before integration.
No new panels yet. QML stays daily driver; dashboard and live validation held.

## 2026-10-17 — Calendar/sound/routing cost accepted; idle audit next

Owner accepts the combined feature cost: five paired settled increases
+1.121/+0.753 MiB Hyprland/niri. The allowance is per explained feature, not
per commit. All 184 UI closes fit own bar+5; settled maxima 23.202/23.844 MiB
remain below25. Named views, native input/presentation and cleanup pass;
191 tests, fmt, all-feature clippy and CPU-only build pass on this exact source.
The implementation is integrated; the pinned preview still uses the earlier runtime.
Next, before more panels: make closed view data/state lazy and release after idle;
report settled before/after. Existing workers/modules/rasters release after2.2s;
a shared launcher-state map still persists. No cleanup saving claimed yet.
Calendar rollover-follow and matched QML remain; no new service contract assumed.
QML stays daily driver. STOP after service views, BEFORE dashboard; no live tests.
[Measurements](research/feature9-gate-d.md#calendar-and-named-views--memory-decision-2026-10-17).

## 2026-10-16 — Core switch active: recovery/static text accepted

Owner accepted the measured+0.339/+0.392MiB settled H/N feature cost.
All20 UI-close/35s-group guards pass; candidate settled maxima21.817/22.912MiB
fit the new25MiB UI ceiling. Exact173-test source integrated; fmt/clippy/
CPU-only/release checks pass. Feature CI: [latest main run](https://github.com/nacre-desktop/bead/actions/workflows/ci.yml).
New policy: pure optimizations preserve warm/settled spreads; explained features
may add≤0.5MiB settled each within close+5/25MiB limits. Record and continue.
Tools6350045 / [CI38947299625](https://github.com/nacre-desktop/bead/actions/runs/38947299625) green; pinned preview passes private smoke.
Native row-template WIP rejected: niri warm+0.559/settled+0.482MiB exceed
0.313/0.211 control spreads. Five-pair CPU8.445/8.507ms still misses8ms.
Preserved4db6682; runtime stays acceptedb0. Actual UI-provider duplex ACK test
also passes while menu.activate waits;176 tests/fmt/clippy/CPU-only pass.
Lua-only5043af7 also rejected:
one-pair CPU10.696→10.580 /11.214→11.345ms, no consistent win; runtime unchanged.
Next: daemon typing and activation/reconnect/provider contracts, then remaining
bar popouts/service views. STOP after service views, before dashboard.
Five-pair accepted-control typing UI10.792/10.736ms/key, p9924.705/33.462ms H/N;
targets open. Actual-widget exact pixel/action regressions pass with175 workspace
tests at the pixel checkpoint; now176 with activation duplex coverage. Runtime unchanged.
Persistent2,000-call daemon+provider CPU5.715/4.260ms H/N; query checks pass,
idle samples invalid under host paging. This is component CPU, not UI latency.
Official F3 actions/same-engine QML pending. Core estimate40–81 eight-hour days,
3–8calendarweeks at assumed16–20taskh/day;90–150MiB idle estimate, not measured.
[Evidence and forecast](research/feature9-gate-d.md#gate-e--core-switch-active-2026-10-16).
Desktop [validation plan](research/gate-e-validation-plan.md) HOLD, NOT RUN.
QML remains daily driver. IME/reader beyond current coverage deferred to release.

## 2026-10-16 — Gate D review: adjust

Renderer3bf5eff is pushed; CI38899237836 green. Bounded repair scratch passes
6,400 exact pixel cases and five memory pairs per compositor: first-open
+0.209/+0.120 MiB; warm/closed deltas fit control spreads; every close≤bar+5.
Scratch reuses capacity and releases after500ms quiet. The later lazy-mask
experiment fails warm/closed spreads and stays local; accepted code restored.

Client checkpoint8a15542 adds engine conversion/guidance views, shared daemon
menu/UI-provider connection, legacy UI forwarding and a private nested preview.
165 tests, fmt, all-target/all-feature clippy, CPU-only and release pass.
The early-link SIGTERM bug is fixed; strict H/N exit0+STOP and compositor-loss
cleanup tests pass. No input injection, install, live config or live bus takeover.

|Measured current launcher, H /N|Result|
| --- | ---: |
|Real engine UI CPU mean ms/key|10.045 /9.860|
|Real engine presented p99 ms|25.425 /35.507|
|UI-only fixture CPU Bead /QML ms/key|9.171 /113.911;9.040 /116.460|
|UI-only typed-list Bead /QML MiB|37.448 /574.955;42.336 /555.438|

Typing8ms/16.7ms targets remain unaccepted. Native200-key samples are one pair,
not a new optimization acceptance. Nested Hvirtual120/N60,scale1.5; feedback
refresh unknown, host contention uncontrolled, no own build overlap. Real group
UI+daemon/providers warm98.006/96.243→settled55.047/55.858MiB; UI alone returns
21.965/22.854. Every UI close and the35s-settled group pass own bar+5;
all warm reports show scratch released.
[Method, comparisons and forecast](research/feature9-gate-d.md#gate-d-review--2026-10-16).

Gate D owner chose this focused Gate E block. Typing/RPC and shared-engine QML/actions
before broad parity (roughly99–201 focused agent-days). Official F3 QML adapter,
daemon notification hosting, engine arithmetic/copy/non-app activation, reconnect
mark, static-text AccessKit/IME/hardware, dependency cache accounting and binary
name clash stay open. Preserve6b pacing/idle and historical29.61→32.66% items.
Backends frozen except fixes; Nacre/Orient read-only. No Feature11/live/release.
README has the one-command preview; its notifications use a transitional private
Bead host, while menu results and notification controls use the real daemon.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
