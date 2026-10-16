# Status

## 2026-10-16 — Typing measurement mode; engine integration pending

Last green Source checkpoint: 4fbf629 / CI 38849169061. It pins Nacre 3042f3f2;
keyed items/order, opaque activation/revision in host, engine-owned navigation.
Four menu transcript families plus wire/peer/reconnect/boot proofs pass.
No Bead catalog, ranking or providers. Upstream 83a9a9a4 / CI 38847478920 is green.
Migration's Rust core matches 394 references; provider/host work remains upstream.

Count-only a3ab6dc stays kept after five alternating pairs per compositor:
CPU medians H 17.46→16.16/N 19.97→17.78 ms/key; p99 30.30→25.13/29.82→27.20.
Closed deltas +0.24/+0.05 MiB fit baseline spreads 0.55/0.64; 4000 keys/80 guards pass.
Those historical measurements use full stage tracing; typing targets are unaccepted.

Timestamp-only mode keeps exact input/commit/presentation and process-CPU clocks,
without stage spans. Same-binary, two-run medians: full→clocks H 12.10→10.19/
N 13.25→11.75 ms/key; H variation is wide. This calibrates observation overhead,
not a UI optimization. 512 private key proofs pass. Refresh prediction is unknown
in both nested compositors; preserve null rather than claim a measured refresh rate.
[Method](research/feature9-gate-d.md#typing-clocks-and-tracing--2026-10-16).

UI/Luau/source/SHM repair changes remain local and unaccepted pending matched
clock-only before/candidate and memory repeats. 147 local tests/fmt/clippy/CPU-only/
release pass. Disjoint SHM repair cuts paint area to 3.4% of the old bounding box;
full-trace CPU H 13.39→11.65/N 14.52→12.50 ms/key, closed memory falls in that pair.
Earlier ID-map variants were rejected for closed/steady memory. No memory waiver.

Measurement mode is this checkpoint; fresh CI follows the push. Next decide
measured UI changes against the matching clock-only control. Real
engine readiness must use completed request ID/revision/count, then exact presented
update; report UI and owned-daemon CPU separately against matched QML. Both 8 ms CPU/
16.7 ms presentation targets remain open. STOP at Gate D after the measured forecast.
Keep 6b pacing/idle, historical typing CPU, IME/hardware/cache gaps and binary name
clash open. No Feature 11/live/install/release work. CPU small panels/GL large
animation; Mesa residue and release-on-idle policy remain.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
