# Status

## 2026-10-16 — Core switch active: recovery/static text accepted

Owner accepted the measured+0.339/+0.392MiB settled H/N feature cost.
All20 UI-close/35s-group guards pass; candidate settled maxima21.817/22.912MiB
fit the new25MiB UI ceiling. Exact173-test source integrated; fmt/clippy/
CPU-only/release checks pass. Feature CI: [latest main run](https://github.com/nacre-desktop/bead/actions/workflows/ci.yml).
New policy: pure optimizations preserve warm/settled spreads; explained features
may add≤0.5MiB settled each within close+5/25MiB limits. Record and continue.
Next: daemon typing and activation/reconnect/provider contracts, then remaining
bar popouts/service views. STOP after service views, before dashboard.
Current typing UI11.041/10.988ms/key, p9924.287/34.679ms H/N; targets open.
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
