# Status

## 2026-10-16 — UI typing resumed; five-pair count study running

Main 3cf413c / CI38830981182 is green; renderer checkpoints347bd22 and
59f00f1 are also green. Owner resumed independent UI typing work.
Active-view construction and cropped text are kept. 136 tests, fmt, clippy,
release, exact pixel/counter checks and both native-GL smokes pass.

Matched crop pairs, 200 changed-result keys: UI CPU H24.95/N25.69 →18.45/18.77
ms/key; presented p99 33.52/41.35 →30.43/29.02 ms. Typed-list PSS+DRM falls
3.70/3.78 MiB. Both typing targets still fail; identical rows are a best case.
H virtual 120 / N nested 60, not physical; helper/end-to-end unmeasured.
[Measurements and provisional forecast](research/feature9-gate-d.md#tight-text-rasters--2026-10-16).

Count-only model API is saved locally as 95862ca, held off main. Three trial CPU
medians improve H 18.40→16.08/N 20.33→16.74 ms/key, but niri tails are mixed.
Closed PSS +0.18/+0.06 MiB needs confirmation; no tolerance waived. 137 local
tests/fmt/clippy/release/Luau pass. Default executable is the accepted crop build.

Crop lifecycle: H bar 22.31 → third warm 48.42 / closed 21.91 MiB; N 23.72 →
46.50/19.40. Every close stays within bar + 5 MiB; first/steady/closed controls show
no regression. Query RAM, hover expiry, feedback and cleanup pass. No allocator
policy change. CPU small panels/direct opaque paint; alpha groups stay cached.

Nacre owns catalog/ranking/providers; Bead uses Sources/exact fixtures. Published
SDK pin 6bdc22e and standalone F1 adapter proofs pass. Canonical menu schemas/
engine follow a time-boxed upstream cold-start investigation, before F2 finishes.
Additive protocol draft is reviewed and held off main until migration publishes
its schema/transcripts. End-to-end/prefix/activation acceptance still waits.

Owner resumed work: five alternating before/candidate pairs per compositor,
200 keys plus three lifecycle closes each. Keep count-only only if CPU median
wins and presentation/closed-memory deltas fit the measured spread. Continue
independent UI CPU work; adopt published menu schema then measure UI/daemon CPU
separately. STOP at Gate D. Keep 6b pacing/idle, historical typing, IME/hardware,
cache gaps and name clash open. No Feature 11 or live/install/release work.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
