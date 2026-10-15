# Status

## 2026-10-16 — Source launcher fixture checkpoint; Gate D pending

Main684b922 / CI38823402035 green. Source adapter/F9/framework fixes are landed.
Launcher now uses Source calls/exact fixtures; local catalog/filter removed.
Nacre owns engine/schema/ranking/providers. Engine follows F2; real typing waits.
[Methods, measurements and limits](research/feature9-gate-d.md#feature-10-source-fixture-checkpoint--2026-10-16).

Default CPU,320 fixture rows,200 keys: UI CPU mean H5.09/N5.01 ms/key;
presented p99 H13.96/N19.06 ms. Steady p99 H13.88/N16.49; first presented
39.06/28.93 ms. Overall N/first-transition targets and end-to-end are unaccepted.
Immutable model snapshots halve steady diagnostics; JSON/rollback remain intact.
The50ms cumulative behaviour budget now excludes native/other-VM idle work;
actual small Lua calls still exhaust it. No execution limit raised.

Output-based viewport removes recursive percentage resizing. Launcher stays
1164×654; hover timeout/query RAM/not-disk/three lifecycle closes pass both.
Bar/first-warm/third-closed MiB H22.17/53.07/23.45,N23.53/51.25/24.39.
Sound guard bar/warm/closed5s H22.07/28.89/22.78,N23.62/30.92/23.68;
all closes/lifecycles within bar+5. Published-Rust control is larger on both.
No allocator policy change. New paint benchmark selects direct opaque motion;
alpha groups remain cached.134 tests/fmt/clippy/CPU+release and policy checks pass.

Matched fixture UI comparison complete: Bead/QML warm MiB H37.80/253.52,
N41.81/208.51; CPU mean5.27/5.69 and5.72/5.55ms/key. Presented p99
15.09/15.74 and15.83/12.70ms. QML extra details/controls retained; no parity claim.
[Comparison and provisional forecast](research/feature9-gate-d.md#matched-launcher-view-comparison--2026-10-16).
Reference checkpoint:135 workspace tests/fmt/clippy plus C++ input/reload proofs
pass. Next: canonical menu schemas/engine, real prefixes/activation and end-to-end,
then finalize Gate D forecast/owner decision. First transition remains open.
Backends frozen; no live desktop, installation or release permission.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
