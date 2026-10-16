# Status

## 2026-10-16 — Launcher input parity; typing targets still open

Published constructor checkpoint3a9dd0f /CI38869424491 is green. This checkpoint
adds first-enabled selection on query edits, geometry-based arrow navigation,
scroll containment and visible activation errors over the unchanged engine order.
153 tests/fmt/clippy ALL targets/CPU-only/release pass. Both private nested input
proofs pass13 states:12 exact presentations plus one no-change acknowledgment.
The reconnect test now synchronizes event-socket readiness; backend code stays frozen.

Five alternating lifecycle pairs each: warm H48.17→48.57/N46.15→46.37MiB;
closed21.92→22.19/22.58→22.77. Deltas fit control ranges1.48/0.95MiB;
all60 cycle checks and hover guards pass bar+5. First-open+0.33MiB each fits
control spreads too. Mixed typing checks show no optimization; CPU8ms/key and
p99≤16.7ms remain unaccepted. Configured Hvirtual120/N60,scale1.5; feedback
refresh prediction unknown. External contention is uncontrolled, not physical proof.
[Method and limits](research/feature9-gate-d.md#launcher-input-parity--2026-10-16).

Kept: count-onlya3ab6dc, canonical Source4fbf629, measured pixel/SHM/Luau/model
changes3e3bb4e+clippy repaircdf157e, font reusee12b17f, constructor3a9dd0f.
Font loads/key1→0; index owns no font data. Prior4000-key/ten-lifecycle series
and one unclassified niri IPC failure remain recorded in the research report.
SDK-only optlevel2 is rejected; level1 stays. Bake-copy experiment is separate.

Upstream83a9a9a4 /CI38847478920 green; SDK3042f3f2/menu goldens pinned.
Next: reduce fixture UI CPU; migration owns engine/provider/host work before F2
finishes. Once hosted, compare real UI/owned-daemon CPU separately vs matched QML.
STOP at Gate D. Keep6b pacing/idle, historical29.61→32.66% typing CPU,
IME/hardware/cache gaps and binary name clash open. No live/install/release work.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
