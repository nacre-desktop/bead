# B0141 — scrolling follow-up

Status: allocator fix (Luau 0.742 analyzer), Chrome scroll physics, bounded GL/CPU picker caches and the fresh-update replacement are landed with full Arch CI (all steps pass except the sway-based wallpaper-video smoke, which cannot exec sway in the dev-2 container). Native memory pairs, the 30-minute soak, AI-reversal and history/popup/Clipboard final-source measurements remain open. B0135 is on main; no live installation.

## Scope and gates

Picker scroll/raster resources share one40 MiB ceiling across encoded assets,
CPU rasters, GL textures, retained metadata and composites. B0134's40 MiB
physical retention allowance applies after close, above the same shell's
never-opened idle. Report full active and closed unreclaimable memory, including
DRM; payload counters alone cannot establish the physical retention pass.
B0134 source is integrated; rebase onto current main before final artifact freeze.

Reproduce and fix the historical allocator abort using remote instrumentation.
AI reversal must reach one frame at synthetic120 Hz (8.33 ms), joining actual
input, store, matching commit and changed presentation. History, popup and
Clipboard need measured justification at≤2 MiB each or a fix.
Require full remote Arch CI, exact pushed-head CI, source-pinned native timing,
QML pairs, five alternating same-host memory pairs and a continuous30-minute
instrumented scroll soak with zero aborts across every reachable scroll container.
Headless soak and native presentation are separate evidence. Private desktop
commands hold the shared lock for less than15 minutes and retain swap/source/
geometry/containment gates. No live desktop, install or release authorization.

## Changes

Active prepare/upload retires absent cache entries only after replacement succeeds.
GL panels use worker-prepared upload resources, original bitmap cover textures and
lossless tiled SVG atlases; exact dense CPU rendering remains the fallback.
A shared budget charges encoded assets, rasters, textures and opacity groups.
Bounded decode scratch is separate from retained cache residency and must be
reported. Resource completion wakes the renderer without rerunning shell logic.
Generic image preparation and compaction belong in framework crates; picker policy
and presentation remain in Luau. No provider or wallpaper activation replacement.

Fresh CPU UI updates may replace a pending callback's obsolete commit using the
existing buffer set. This needs final memory and presentation verification.
Wheel movement is100 logical pixels per notch, scaled once by the output.
Finger movement remains1:1 without contact damping. Release motion follows
Chromium's Linux-default PhysicsBasedFlingCurve:96 DIP/inch,2645.664 DIP/s²,
duration=min(abs(v)/2300,2 s), endpoint=min(v²/(2a),3 client viewports), with
the release-speed-preserving cubic Bézier. This is an independent per-axis,
unboosted implementation; diagonal coupling and repeated-fling boost are excluded.
[Chromium default](https://chromium.googlesource.com/chromium/src/+/c11ad0021df3ea0608b099ad8b52ad1c0665d47e/ui/base/ui_base_features.cc),
[physics source](https://chromium.googlesource.com/chromium/src/+/3e1a26c44c024d97dc9a4c09bbc6a2365398ca2c/ui/events/gestures/physics_based_fling_curve.cc).

## Evidence and remaining work

The allocator failure is reproduced in standalone Luau0.740 after successful
Bead CHECK_OK: run12 produces the original tcache/SIGABRT failure; explicit
glibc debug-allocator preload crashes on run28. Upstream identifies a shared
parallel error-reporter race, fixed in analyzer0.742. The fixed tool passes
100/100 original-environment checks and100/100 with verified debug preload.
CI pins its official release digest; signal failures are labelled explicitly.
Embedded runtime Luau remains unchanged. [Attribution and limits](evidence/0141/allocator.md).

Remote renderer/native/default checks pass for the current pipeline; final
lifecycle review, exact-source release, native memory/parity, CI and soak remain.
An earlier callback-only AI diagnostic measures reversal11.09/12.89/13.03 ms
before and12.04/7.53/4.69 ms after: median7.53 ms, maximum12.04 ms. Thus it
passes a median-one-frame criterion but does not establish every-reversal≤8.33 ms.
Its direct-parent route is different from B0135's49.22 ms nested-niri result.

Old B0135 history/popup/Clipboard medians (+1.484/+1.258/+2.176 MiB) are
historical, not current-source passes. An initial GL80 active-cache retirement
candidate still consumed147.4 MB in reported caches; this failed the40 MiB gate
and motivated native uploads/compaction. Final-source measurements are pending.
No temporary alternative product implementation is retained.
