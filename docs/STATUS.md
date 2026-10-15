# Status

## 2026-10-15 — cache checkpoint accepted; bounded investigation, then Gate D

Owner accepted215ea35: all close/lifecycle checks pass bar+5 MiB and later
opens became smaller. Integrating the tested cache checkpoint; CI checked per push.
[Cache implementation, measurements and limits](research/cache-budget.md).

Next: at most about one hour investigating first-open allocation, including
five alternating before/candidate samples per compositor with median/spread.
Fix only a simple demonstrated cause; otherwise record it and move on.
Then opaque composition measurement, Feature9 Source UI + matched QML,
Feature10 via Nacre's existing helper lane, protocol adapter when published,
and STOP at GateD. Nacre main5995d171 still lacks implemented protocol v1.

Nested H120/N60 first-warm delta+0.634/+0.405 MiB; third-open−1.294/−0.719.
111 tests/fmt/all-features clippy/CPU+release builds/deny/Luau pass.
First-open increase<1 MiB may continue if bar+5 passes and steady/closed memory
is not regressed. Greater first-open or steady/closed regressions still stop.
Backends frozen; no helper edits/catalog duplication. No live/install/release.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
