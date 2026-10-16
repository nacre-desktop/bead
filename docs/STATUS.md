# Status

## 2026-10-16 — Tight text rasters; Gate D pending

Main 59f00f1 / CI38828546515 is green. Active-view preparation fixes the
repeated markup guard failure; the 50 ms limit and 320-app fixture stay intact.
Cropped-text checkpoint: 136 tests, fmt, clippy and release pass. Both lifecycle
memory and native-GL smokes pass; fresh CI is checked after push.

Disjoint 160-row results, 200 keys: UI CPU mean H24.95/N25.69 →18.45/18.77
ms/key; presented p99 33.52/41.35 →30.43/29.02 ms. Text blending falls from
6.80/7.81 to0.56/0.62 ms; typed-list PSS+DRM falls by3.70/3.78 MiB.
Both typing targets still fail. Identical-row fixtures remain a separate best case.
H virtual120/N nested60, not physical; helper CPU/end-to-end unmeasured.
[Methods, comparisons and forecast](research/feature9-gate-d.md#tight-text-rasters--2026-10-16).

Source calls replace local catalog/filtering; Nacre owns engine/ranking/providers.
SDK pin6bdc22e and private standalone F1 proof pass. Menu schemas/engine follow
upstream F2. Additive protocol draft reviewed locally, held off main for migration.

Cropped lifecycle: H bar22.31 → third warm48.42/closed21.91 MiB; N23.72 →
46.50/19.40. All hover/preparation/three closes within bar+5, query RAM and cleanup
pass. Matched first/warm/closed show no regression. No allocator
policy change. Auto uses direct opaque composition; alpha fades stay cached.

Next: verify crop CI, measure avoiding full-model serialization for counts,
then canonical engine prefixes/activation and end-to-end comparison. Finalize
Gate D estimate/owner decision afterward. No Feature11 or live/install/release.
Backends frozen. Existing6b pacing/idle, input/IME/hardware and name clash stay open.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
