# Status

## 2026-10-16 — Keep measured UI typing improvements; targets still open

Last green measurement checkpoint: e4145f5 / CI 38856389288. This checkpoint
keeps ordered pixel damage, separate SHM repair regions, Luau node constructors,
shared immutable result models and the redundant-caret-shape fix. Count-only a3ab6dc
and canonical Source 4fbf629 remain kept. CI 38863863203 caught test-only redundant
borrows; the follow-up fixes them. Local clippy now includes all targets with
warnings denied. Production behavior and measured binaries are unchanged;
fresh CI follows this repair push.

Five alternating clock-only pairs per compositor: CPU medians H 15.47→10.10 /
N 16.96→10.83 ms/key; presented p99 medians 24.72→22.19 / 27.02→21.34 ms.
Ten lifecycle pairs each: third warm H 48.51→47.95 / N 46.20→45.60 MiB;
closed 21.87→21.64 / 22.29→22.03. First-open also falls. Every current close/
hover check stays within its own bar + 5 MiB; no memory threshold was waived.
[Method, spreads and limits](research/feature9-gate-d.md#clocked-ui-typing-improvements--2026-10-16).

4000 exact typing proofs and 160 successful lifecycle/hover guards. One initial
niri IPC state call failed; original evidence is retained, its exact diagnostic
retest and 20 extra lifecycle sessions pass. Cause remains unclassified; failure
reply/client-status capture is added. Do not describe the original series as flawless.
147 tests/fmt/clippy/CPU-only/release pass; native GL functional smoke passes (6 keys each compositor).

Both paths use scale 1.5 and 1746×981 CPU buffers. Nested H virtual120 / N60 are
configured rates; feedback refresh prediction is unknown, not physical validation.
Both 8 ms CPU / 16.7 ms presentation targets remain unaccepted. Next measure the
font-system reuse prototype and validate numeric real-engine result readiness.
Those drafts are separate from this checkpoint; no catalog/ranking/providers here.

Upstream 83a9a9a4 / CI 38847478920 is green. SDK3042f3f2/menu goldens are pinned;
migration owns engine/provider/host work before F2 finishes. Then compare real
engine UI and owned-daemon CPU separately against matched QML, and STOP at Gate D.
Keep 6b pacing/idle, historical 29.61→32.66% typing CPU, IME/hardware/cache gaps
and the binary name clash open. No Feature 11/live/install/release work.
CPU small panels / GL large animation; Mesa residue and idle-release policy remain.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
