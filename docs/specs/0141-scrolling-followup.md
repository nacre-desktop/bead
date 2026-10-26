# B0141 — scrolling follow-up

Status: investigation. B0135 is on main and remains uninstalled live.

## Scope and gates

Bound picker scroll/raster resources to visible rows plus a small margin. The
picker shares B0134's 40 MiB unreclaimable budget in total across its resources,
measured above its own never-opened shell idle. Report absolute process totals
as well as that picker cost, including active, reopened and closed phases; do not
assign separate40 MiB allowances to cache owners or exclude DRM. Capacity counters
alone do not establish a physical-memory pass. Rebase onto B0134 rather than duplicate its
resource retirement, thumbnail decoding or wallpaper activation paths.
Reproduce the allocator abort with remote sanitizer/debug-allocator evidence,
identify and fix its cause. A passing retry alone does not close this blocker.
AI reversal must reach one frame (8.33 ms at synthetic 120 Hz), using actual
input → store → matching commit → changed presentation joins.
History, popup and Clipboard must meet their previous gates or carry a measured,
explained budget of at most 2 MiB each under this unit's owner authorization.

Full remote Arch CI, source-pinned native timing, appropriate goldens and QML
pairs, and five alternating same-host memory pairs remain required. Unreclaimable
is Pss_Anon + Pss_Shmem + DRM + SwapPss; file PSS is informational. Missing
measurements are unavailable. Require zero aborts through a continuous 30-minute
instrumented remote scroll soak covering every reachable scroll container.
Desktop proof commands retain the shared lock for less than 15 minutes each;
remote headless sanitizer soak and native transport timing are distinct gates.
No live desktop, installation or release is authorized.

## Integration and ownership

Starting main is588d9506, including B0134 commits85f72cc2/588d9506. B0134's
source is integrated while its final five-pair report remains in flight. Preserve
its measured source pins; rebase changes without overwriting its worktree.
General cache/input primitives belong in framework crates; shell policy remains
in Luau and edge adapters. No duplicate renderer/provider or alternate scroller.

Allocator investigation owns remote sanitizer/CI setup; picker work owns render
cache bounds; latency work owns shared scheduling diagnosis. The coordinator
reviews changes, owns memory/soak planning, integration and final evidence.

## Initial evidence and design review

B0135 history/popup/Clipboard open medians were +1.4844/+1.2578/+2.1758 MiB
against its matched old control. The new owner allowance can justify the first
two only with measured explanation; Clipboard still exceeds2 MiB. Those frozen
measurements are not relabelled as current-main results. Five new pairs must
keep source, backend, geometry and fixture state matched.

Current B0134 already gives thumbnails stable IDs, so B0135's generation-suffixed
thumbnail accumulation is historical. The remaining generic cache design retains
untouched rows until large CPU/GL LRU thresholds. The candidate retires absent
lookup entries only after successful prepare/upload, preserves active image
previous-size reuse and leaves the last accepted frame valid on failure. Tests
cover bounded retention, held old-frame charges, fresh-render pixel equality and
reverse traversal. No new picker-specific framework policy or extra margin is
introduced before measured need.

The original AI trace is9.22 ms input→delivery and40.00 ms delivery→visible,
with callback/presentation intervals near16 ms. The first inverse logical step
repeats an already displayed offset; it is not yet a visible reversal. A private
direct-parent120 Hz proof will separate transport cadence from client scheduling.
No inverse-delta summing or violation of finger1:1 is used to improve a metric.

Forty fresh B0135 debug checks, including glibc checking/perturbation, have not
yet reproduced the allocator abort. Rust ASan and Luau C++ ASan are compiling
into a dedicated target; passing checks alone do not resolve the blocker.

## Allocator attribution under investigation

The historical workflow exit250 is consistent with Python propagating an
analyzer subprocess SIGABRT(-6), rather than a shell-process --check abort.
`tools/check-luau.py` starts type generation and the analyzer after CHECK_OK and
captures their output; the earlier standalone --check retry omitted that step.
Upstream [issue3020](https://github.com/luau-lang/luau/issues/3020) reports the
exact allocator message from parallel checks racing on the shared error reporter.
[Luau0.742](https://github.com/luau-lang/luau/releases/tag/0.742) lists the
upstream fix. Reproduction and sanitizer attribution are still required before
this unit calls the historical blocker resolved; the runtime Luau dependency is
not assumed faulty merely because the standalone analyzer has a race.
