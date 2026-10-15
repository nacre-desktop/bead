# Status

## 2026-10-15 — STOP: cache candidate needs a memory decision

Published main `5afb5a3` / CI `38788177900` green: OSD overflow corrected.
Recovered cache work preserved in the isolated gate-d-foundation checkpoint;
not integrated because the owner's memory-regression stop condition applies.
[Cache implementation, tests, measurements and decision](research/cache-budget.md).

Nested H120/N60, matched real bar+sound, three cycles/five-second idle:
first warm PSS+DRM MiB 27.848→28.482 / 30.854→31.259 (+0.634/+0.405).
Third warm 28.035→26.741 / 30.101→29.382. Closed 22.602→22.986 /
23.077→23.761; every close/lifecycle stays within its own bar+5 MiB.
Contended short samples; no physical, QML, CPU or idle-invariance acceptance.
Owner: accept the first-open tradeoff, or remove it before integration.

111 tests/fmt/all-features clippy/CPU and release builds/deny/Luau pass.
Shared 192 MiB cache admission, per-owner byte/LRU limits, lifetime charges,
report totals and stress tests implemented; dependency internals externally
bounded and documented. Interrupted empty measurement script recovered.
No task leftovers, live changes, installation or release.

Next after owner decision: opaque composition measurement, Feature9 Source UI
and matched QML, Feature10 via Nacre's existing helper lane, protocol adapter
when published, then GateD. Nacre main `5995d171` still lacks implemented v1.
Backends frozen; no helper edits or duplicate catalog/ranking/providers.
[Prior F5 report](research/foundation-f5.md). Typing targets remain unaccepted.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
