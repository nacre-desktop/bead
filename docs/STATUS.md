# Status

## 2026-10-16 — Launcher changed-result checkpoint; Gate D pending

Code e2ba01d CI38827038558 passed. Docs-only 5a7f548 CI38827618562 repeated
its 50 ms markup failure. Active-view preparation removes hidden grid work;
135 tests/fmt/clippy/Luau pass. Fresh CI follows; execution limit unchanged.

320 fixture apps, 200 keys: identical rows were a best-case reuse workload.
Disjoint160-row results expose CPU mean H 36.57 / N 35.60 ms/key; active-view
candidate 23.90 / 26.06. Presented p99 improves 48.05/50.52 → 38.97/40.49 ms.
Both typing targets remain unaccepted. Composition and markup are still costly.
Matched unchanged QML gets the same rows; CPU 133.78/135.56 ms/key. Extra
QML controls remain; no full-parity claim. H virtual 120 / N nested 60, not physical.
[Measurements and provisional forecast](research/feature9-gate-d.md#changed-result-typing-and-active-view-preparation--2026-10-16).

Source calls/exact fixtures replace local catalog/filtering; Nacre owns engine,
ranking and providers. Published SDK pin 6bdc22e and private standalone F1 proof
pass. Canonical menu schemas/engine follow upstream F2; end-to-end/helper CPU wait.
Additive protocol draft reviewed locally, held off main for migration integration.

Last accepted default lifecycle: bar/first-warm/third-closed MiB H 22.17/53.07/23.45,
N 23.53/51.25/24.39; every close within bar + 5. Sound guard is below published-Rust
control. No allocator policy change. Auto uses direct opaque composition; alpha
fades stay cached. Active-view lifecycle also passes both: preparation/query preservation, three
closes within bar + 5 MiB, real feedback and cleanup.

Next: publish validated active-view fix, profile remaining changed-result typing,
then canonical engine prefixes/activation and end-to-end comparison. Finalize
Gate D estimate/owner decision afterward. No Feature 11 or live/install/release.
Backends frozen. Existing 6b pacing/idle, input/IME/hardware and name clash stay open.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.
