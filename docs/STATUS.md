# Status

## 2026-10-16 — Count-only accepted; canonical menu contract published

Main 351fc54 / CI38842700729 is green. Five alternating pairs per compositor
accept count-only95862ca: CPU median H 17.46→16.16/N 19.97→17.78 ms/key;
presented p99 median 30.30→25.13/29.82→27.20 ms. Closed deltas +0.24/+0.05 MiB
fit baseline spreads 0.55/0.64; steady deltas also fit. All 4,000 key presentations
and 80 closed/unused-hover guards pass. 137 tests/fmt/clippy/Luau pass; fresh CI
is checked after push. Both 8 ms CPU /16.7 ms presentation targets still fail.
[Methods and full comparison](research/feature9-gate-d.md#count-only-decision-five-alternating-pairs--2026-10-16).

Active-view construction and cropped text stay kept; previous 136-test code and
native-GL/pixel/counter proofs are green. Crop typed-list memory falls 3.7–3.8 MiB;
every lifecycle close remains within bar + 5 MiB. No allocator/font-engine rewrite.
H virtual 120/N nested 60, not physical. Helper/end-to-end CPU unmeasured.

Reviewed menu payload/schema/transcripts are on Nacre main 3042f3f2; its eleven
protocol tests/export/fmt/clippy pass, old wire artifacts byte-identical. Source
CI is checked after push. Next pin this canonical revision and switch the fixture
Source to keyed items/order and opaque activation. Nacre owns catalog/ranking/
providers; no helper edits. Engine follows time-boxed startup work before F2 ends.

Continue independent UI CPU work: ordered pixel-damage reuse is saved locally,
not tested/measured yet. Then real engine/UI-provider/prefix/actions, separate UI
and daemon CPU, matched QML comparison and final Gate D forecast. STOP at Gate D.
Keep 6b pacing/idle, historical typing CPU, IME/hardware/cache gaps and name clash
open. No Feature 11/live/install/release work. CPU small panels/GL large animation;
Mesa residue finding remains. No persistent warm launcher memory.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
