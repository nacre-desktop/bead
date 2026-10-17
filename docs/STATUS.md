# Status

## 2026-10-17 — Frame cost needs owner decision; dashboard draft preserved

Owner authorizes first dashboard C pages after placement parity; device controls
use published schemas, with hosting/implementation after Nacre F4. Stop at the
first pages or a needed decision. QML stays the daily driver; no live
permission.

Placement candidate d88e94e is UNPUBLISHED. Eleven exact real-Qt geometry/rim
cases and paired 2880×1800/1.5 placement masks pass, with exact solid interiors.
220 candidate workspace checks plus trace/fmt/clippy/CPU-only/release pass. The
mapped-close guard fixes a real hover/close presentation race; proofs stay
strict.

Five alternating pairs/compositor plus native20: all 424 close/settled samples
(244 candidate) fit own bar + 5 MiB. Maximum candidate settled UI is 23.096 /
23.648 MiB H / N, below 25. Paired settled increases +0.729 / +0.861 MiB exceed
the 0.5 MiB feature allowance and control spreads 0.216 / 0.440 MiB. The
allowance gate FAILS; no COMPLETE marker or runtime integration. About 0.260 MiB
is shared frame buffers; most of the remainder is file-backed, with smaller
anonymous growth. No saving/noise claim. [Measured
report](research/feature9-gate-d.md#dashboard-c-authorized-placement-candidate--2026-10-17).

Only this report/scope update is published. The accepted preview stays
unchanged. Dashboard WIP 911688e (bab527a/77b0a51 checkpoints) is preserved on
its isolated branch, NOT built/tested: page-scoped subscriptions on one existing
daemon link, three separate lazy page bundles, typed fixtures and drafted tests;
goldens pending. Protocol pin/lock remains 1e2d6755; adopt green c2b26d98 before
compiling the draft. Reserved media/system/weather/updates/device schemas are
FULL GREEN in Nacre [CI
39010730141](https://github.com/nacre-desktop/nacre/actions/runs/39010730141),
not hosted capabilities. Media is lyrics/palette enrichment, not playback. Real
data stays unavailable without an advertised topic; explicit preview fixtures
never simulate actions. Protocol reader capacity trimming is requested upstream
and remains open.

## Accepted runtime cbd7031

Read-only network/Bluetooth/battery Sources are lazy; workers retire after 2.2 s
closed, unused hover after 2 s. All 48 checked closes release module/view
owners. Source f1d66af [CI
39003597141](https://github.com/nacre-desktop/bead/actions/runs/39003597141) and
docs cbd7031 [CI
39004077385](https://github.com/nacre-desktop/bead/actions/runs/39004077385) are
green. 213 baseline workspace checks pass. Prior five pairs/native20: all 394
close and settled samples fit own bar + 5 MiB; maximum settled UI 23.831 /
24.482 MiB H / N. Measurements are from separate nested cohorts, not directly
interchangeable absolute baselines. Frozen helper 75157 remains the A/B control.
No private preview artifact changed.

## Open gates and forecast

Typing remains unaccepted: last compatibility pair UI 9.963 / 10.922 ms/key and
presented p99 30.886 / 31.562 ms H / N. Keep 8 ms / 16.7 ms targets and 29.61 →
32.66% regression. No typing gain was measured in this placement block. Actual
executor/shared QML and service hosting remain upstream; public ui.menu.activate
stays denied.

Core forecast 40–81 eight-hour agent-days (320–648 task hours), practical 3–8
calendar weeks including waits; full parity 102–207 days, 7–18 weeks. No
measured C-page pace justifies reducing it. Complete-core idle 90–150 MiB is an
estimate, not a combined measurement. Keep 6b niri pacing/strict idle, long
chaos and binary-name clash open. Memory rule remains 0.5 MiB per feature, own
bar + 5 MiB, settled UI ≤25 MiB both compositors. CPU for small panels; GL for
large animation (~20 MiB Mesa residue). Desktop validation plan HOLD. No live
UI/install/config/unit/keybinding changes. Optional preview system-bus reads are
unimplemented; IME/reader work stays deferred.
