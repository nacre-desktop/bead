# Status

## 2026-10-16 — Stopped for memory decision, before Gate D

Published code: 4d9aa45, CI 38892906538 green. Kept Luau brush/bind/action
constructors: five-pair UI CPU medians H 11.243→10.727 / N 11.309→10.695 ms/key.
159 tests and full local checks pass; five memory pairs fit control spreads and
all close/hover guards pass bar + 5 MiB. Native one-pair CPU also falls, but the
8 ms UI / 16.7 ms presentation targets remain unaccepted.

Unpublished renderer WIP: 7271a86 on wip/gate-d-rounded-aa-memory-stop-20261016.
It fixes an exact partial/full rounded-corner pixel mismatch and passes 160 tests,
fmt, all-target clippy, CPU-only and release. It also regresses memory:

| First pair, MiB | Before H / N | Candidate H / N |
| --- | ---: | ---: |
| First open |50.909 /48.638|51.885 /50.109|
| Third warm |48.515 /46.538|49.639 /47.769|
| Third closed |22.310 /22.950|22.121 /23.075|

N first-open +1.471 MiB and warm +1.231 trigger the owner's standing stop rule.
A second H pair also has first +1.195 / warm +1.220. Most growth is anonymous,
not extra SHM; the full-canvas mask/allocator retention is the likely cause.
All measured closes still pass bar + 5; that does not waive the warm-memory rule.
The expanded run was interrupted, all evidence and exact source preserved locally.
Private test processes were stopped; accepted branch and shared binary restored.
No regressive renderer code was pushed. [Evidence](research/feature9-gate-d.md#rounded-repair-memory-stop--2026-10-16).

Owner decision: continue with a bounded-scratch repair under the same budgets,
or retain the published renderer with its exact-pixel gap recorded. Gate D is
not reached. F3 is already authorized upstream; its shared QML adapter is pending.
No duplicate client, ranking/provider or client arithmetic parser. Keep conversion/
guidance UI, UI-provider/reconnect, 6b pacing/idle, historical 29.61→32.66%,
cache/IME/hardware and binary-name gaps open. No Feature 11/live/install/release.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
