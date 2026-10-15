# Optimization pass

Gate C is accepted after the supervised 2026-10-15 physical test. All further
experiments are nested. Stop before Feature 9.

## Baseline and policy

The physical bar + CPU sound panel passed opening/closing p99 16.667/8.337 ms;
longest opening 58.330 ms and command→ready p50/p95 45.25/64.64 ms still
justify bounded hover preparation. Nested Gate C ready p50 was 48.63 ms
(Hyprland virtual 120 Hz) / 74.88 ms (niri nested 60 Hz).

Use CPU for small panels and GL for large animated surfaces. The earlier
explicit small-GL experiment retained about 20 MiB above the bar even after
EGL shutdown and allocator trimming, including Mesa/LLVM mappings. Dropping
Bead's caches cannot safely discard driver-owned process mappings. The CPU
sound panel passes the close/idle ≤5 MiB gate. Feature 6's ~105 MiB used a
larger GL stress panel, not this 400×286 content panel.

Typing 29.61→32.66% remains an optimization candidate. Nested niri/shared-GL
pacing and strict idle invariance from 6b step 0a remain open; the physical
small-CPU-panel result does not close those distinct experiments.

## Bounded hover preparation — initial checkpoint

Entering the bar's audio icon starts the existing script/bake worker with
closed initial state, the hovered output and its scale. It creates no panel
or dismissal surface. Opening consumes this preparation; otherwise the
worker/VM/rasters expire after two seconds, with idle trimming. Moving within
the icon does not renew the deadline; leave/re-entry can start a new attempt.
No persistent warm actor or cache is introduced.

Three pixel-verified cycles after 150 ms hover lead: command→ready p50/p95
9.99/10.73 ms (Hyprland), 7.39/7.72 ms (niri). Unused expiry PSS+DRM delta
−0.49/−0.80 MiB; closed +2.07/+0.10 MiB. These are exploratory trials during
the pass; matched longer A/B and final lifecycle checks are still pending.

## Typing: immutable bytecode, fresh evaluation

A/B/B/A trials (240-row launcher stress workload, two8s samples per version)
reduced CPU35.26→32.05% on Hyprland and36.89→36.26% on niri. CPU per
operation22.69→20.45/125.83→124.47ms; memory116.40→116.38/117.33→117.18MiB
PSS+DRM, swap0. Same background hour-soaks affected both versions; final quiet
measurements remain pending. This is separate from the small sound-panel scene.

The generation owns compiled markup/kit/SDK bytecode (8MiB budget), not a VM.
Each evaluation still creates a fresh sandbox with the existing50ms/8MiB limits.
A test verifies module-local state resets and edited imports replace bytecode
without losing text stores. All71 workspace tests and fmt/clippy pass.

## Retained: share one scoped file watcher

Identical bundle/palette subscriptions share one notification thread. Revisions
and source validation remain per view; the last subscriber drops all ownership.
Warm PSS+DRM+SwapPss29.97→27.59MiB (Hyprland),29.85→27.48(niri), one
thread and three FDs fewer on both. Initial full controls/clipboard/dismissal/
reopen/crash/reload proofs pass. Recovery now matches a non-error companion
commit to its exact presentation update; generic bar commits cannot pass it.

## Rejected exploratory changes

- Fewer scene copies: no typing improvement; patch reverted.
- Two SHM buffers: nested niri exhausted both compositor-owned slots; retain3.
- 512 KiB stacks and16MiB raster cap: no consistent warm-memory improvement.
  Keep default stack headroom and64MiB raster/GL-image limits; idle drops them.
- GL0/0/0 attachments: renders a square slider thumb; reject despite lower memory.
  Zero depth with required stencil selected24/8/0 on both; no saving. Existing
  config is already non-MSAA. Several cold small-GL runs also failed the
  strict feedback proof. Keep CPU-small/GL-large policy and the prior~20MiB
  driver-residue finding. Full panel pixels matched for the depth-only trial.
- Display-size decoding: SVG path already does this; actual matched sound
  scene has no large raster-image decode bottleneck. No decoder dependency added.

## Allocator aging methodology

Initial concurrent soaks terminated early on animation-feedback assertions and
are not one-hour evidence. Restarted memory-only aging still requires each
opening to present and each close to release the actor; timing failures are
recorded without stopping the memory workload. Its pacing field is N/A.
Dedicated sequential pacing acceptance retains the strict feedback requirement.
All variants use the same baseline executable, private compositor/services,
one hour of open/release cycles, and60s endpoint idle. Shared executable/driver
PSS differs from a single-client desktop measurement; include SwapPss.
