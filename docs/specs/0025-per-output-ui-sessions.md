# B0025 — Per-output UI sessions

Status: **Implemented; matched memory and private lifecycle checks pass**. Architecture unit consumed by
[B0022](0022-bar-composition-lifecycle.md); its independent action-contract blockers remain.

## Scope and design

Give each announced output an independent Luau runtime, stable-ID stores,
committed scene, viewport, update/commit serial and input target. Removing an
output drops its source, subscriptions and view resources; callbacks from its
old generation cannot reach a replacement. Output geometry and scale affect
only that session. Existing callers without a session factory retain their
single-source behavior.

The permissive Wayland framework accepts a neutral output-session factory and
owns routing/lifetime, with no shell policy or compositor-specific semantics.
The GPL shell constructs sessions and projects workspace/title data using the
output name. Desktop/backend/daemon service owners and immutable font/image
resources remain shared; render transport stays process-wide. Closed companion
views keep the existing lazy open/closed+idle release policy. Services remain
outside view lifetime (ADR 0003); a script failure must not retire other outputs
or services. Reload validates and commits separately for each session and
preserves that session's stable-ID state on success and previous UI on failure.

No new daemon methods, ranking, provider, compositor adapter, live desktop
change or generalization project. ADR 0004 licensing and framework neutrality
apply. Existing unpublished bar actions stay blocked under B0022.

## Phases and acceptance

1. Commit this spec before product changes. Add neutral per-output ownership,
   update/commit/input/presentation routing and teardown with stale-generation
   fencing. Keep existing single-source tests green.
2. Construct shell sessions with shared service owners and output-scoped model
   projections. Test independent state/reload, failures, local status/input and
   output removal; retain existing lazy-panel and service recovery tests.
3. Run every command in `.github/workflows/ci.yml` in the isolated Arch container
   on the authorized CI host, including independent permissive dependency graphs.
   Run private nested virtual-output smoke on the authorized desktop under its
   heavy-job flock: one/two outputs, hotplug add/remove and scale change. Record
   source/profile/hash and exact commands, never claim physical scanout parity.
4. On the non-reference desktop, compare `origin/main` control and candidate
   with identical default-feature release builds, flags, CPU renderer, geometry
   and output count. PASS each phase if candidate median minus control median
   is at most +0.5 MiB for single-output phases, including scale changes.
   Owner ruling 2026-10-22: intentional extra-output session state is judged
   by the separate per-extra-output target (about 3 MiB), not the +0.5 MiB
   same-count gate for two outputs. Attribute anonymous growth with allocation
   counters or a profiler; share obvious immutable waste when cheap.
   This follows owner queue decision `f58558e`, reaffirmed by the lead on
   2026-10-22; the laptop absolute caps are not desktop stop conditions.
   Use five alternating control/candidate pairs per compositor. Each fresh
   private arm warms each phase for 10 seconds, then retains five 1-second
   samples; this unit uses a bounded bar diagnostic rather than B0002's long
   page-residency warmups. Hyprland phases are one/two outputs; niri phases are
   one output at the initial and changed scales. Report arm medians/spreads,
   anonymous/file/SHM PSS, SwapPss and DRM (unknown stays unknown). Separately
   report candidate two-output minus one-output cost, targeting about 3 MiB or
   less per extra bar. Preserve all swap, containment/watchdog and quiet gates.
   Stop on failed safety gates or unavailable authorized hosts; no laptop tests.
5. Update STATUS, this result and B0022's architecture blocker. Report added /
   removed product lines. Full CI precedes normal safe-push integration; never
   publish a package or alter the installed shell.

## Duplicates

Private calibration/profile/final binary and harness staging copies: delete 1
day after the replacement is trusted (normally at task cleanup); retain raw
measurement evidence, not executable staging. No product duplicate is planned.
The optional single-source
API remains supported compatibility, not a temporary replacement. Any temporary
duplicate introduced later must name its path and an expiry here first:
“delete N days after the replacement is trusted”.

Private capture receipts originally labeled this unit B0023 refer to B0025;
the number moved after the concurrent session-menu unit published B0023.

## Result — 2026-10-22

Captured source `515ecf7` is product/proof-identical to integrated `8b1f7ba`;
rebasing added only the main CI queue update. Control `b972a31` includes the
session menu and keybind/palette changes. Both binaries use Rust 1.99.0,
default features,
`cargo build --release -p nacre-shell --locked -j2`, and
`--bar --core-views --renderer cpu`. SHA-256 control:
`25457830762ae5f22507479900f240639084f8fdb85affce05ca1ac59b92e79c`;
candidate: `cd141af25e5acb7c69b295039d6b62ecad46c2b2e508a53e48002d5a60e409b4`.

Fresh CPU pools have exactly three aligned current-size slots at geometry
changes. Unused-scale raster/node/glyph/image caches retire without invalidating
already presented frames; shared fonts survive. Deferred workers commit the new
scale before painting. After callbacks are quiet for 250 ms, CPU surfaces retire
extra painted SHM slots, repaint one current frame and return allocator slack.
Single-frame generations also return transient heap pages once idle. The timer
is fenced by surface identity and bounded to three seconds; later frames can
rearm it. Importing a worker frame drops obsolete local fallback caches.

Five alternating control/candidate pairs per compositor, with 10 s warmup and
five 1 s samples per phase, give these medians of arm medians and min–max ranges.
All single-output phases pass +0.5 MiB; the intentional second session passes
its separate extra-output target. Geometry matches within every comparison.

| CPU bar phase | Control median [range], MiB | Candidate median [range], MiB | Delta | Gate |
| --- | ---: | ---: | ---: | --- |
| Hyprland one output, 1.5 | 24.688477 [24.371094–24.951172] | 23.696289 [23.502930–23.794922] | -0.992188 | PASS |
| Hyprland two outputs, 1.5 | 25.583008 [25.171875–25.812500] | 25.558594 [25.388672–25.638672] | -0.024414 | PASS (extra-output) |
| niri initial 1.5 | 26.152344 [25.979492–26.221680] | 24.750977 [24.720703–25.325195] | -1.401367 | PASS |
| niri changed 1.25 | 25.605469 [25.441406–26.312500] | 24.725586 [24.678711–25.302734] | -0.879883 | PASS |
| niri return 1.5 | 25.910156 [25.705078–26.644531] | 24.893555 [24.852539–25.834961] | -1.016602 | PASS |

| Candidate minus control, MiB | Anonymous | File | SHM |
| --- | ---: | ---: | ---: |
| Hyprland one output, 1.5 | -0.878906 | -0.154297 | +0.000000 |
| Hyprland two outputs, 1.5 | +0.179688 | -0.190430 | +0.000000 |
| niri initial 1.5 | -0.710938 | -0.172852 | -0.480469 |
| niri changed 1.25 | -0.664062 | -0.172852 | +0.000000 |
| niri return 1.5 | -0.789062 | -0.172852 | +0.000000 |

Median extra-output UI PSS is **+1.862305 MiB** (range 1.818359–1.885742), passing the
~3 MiB target. Anonymous +1.191406, file -0.000977, SHM +0.671875 MiB. Categories are
independent medians and need not sum exactly.

Original two-output anonymous excess versus control was +1.433594 MiB; final matched
excess is +0.179688 MiB.

SwapPss and host swap-in are zero in every final arm. DRM fdinfo is unavailable,
not zero. Raw rollups, full smaps, arm identities, timestamps and comparisons
remain in private evidence. The absolute reference-laptop caps were not used.
The original niri scale deltas +0.884/+0.981 MiB failed; fresh pools alone did
not cure residency. Final current-frame SHM is 0.671875 / 0.564453 / 0.671875 MiB
at initial 1.5 / changed 1.25 / return 1.5. Failed intermediate and profiler
attempts remain separate from acceptance evidence.

### Allocation attribution

In the first final one→two-output pair, glibc live heap grows 1,083,456 B
(1.033 MiB), free arena space grows 497,600 B (0.475 MiB), and total arena grows
1,581,056 B (1.508 MiB); mapped heap stays zero. These are allocator counters,
not additional PSS categories. Returning free pages reduces physical residency
without pretending that the arena's virtual capacity disappeared.

| Named second-session allocation/charge | Bytes |
| --- | ---: |
| Persistent behavior Luau VM | 487,712 |
| Compiled markup | 21,149 |
| Source vector capacities | 62,783 |
| Materialized view, including overlapping markup values | 62,705 |
| Computed values | 273 |
| Glyph images / CPU rasters | 13,533 / 38,054 |
| Retained nodes / prepared frame metadata | 15,848 / 8,419 |

These typed/cache estimates total 710,476 B (0.678 MiB), leaving about 0.356 MiB
of the live-heap increase unclassified among Rust scene/store/runtime ownership
and allocator overhead. This is partial attribution, not byte-perfect malloc
stack accounting. Do not add the 55,305 B markup-values charge again, or add
heap counters to PSS. The temporary markup VM's 913,000 B end-of-build reading
is dropped after construction; Taffy's layout arena is temporary too. The Luau
behavior VM dominates the named retained costs, rather than a glyph cache or
persistent layout arena. Weak font/asset indexes and font payloads stay shared.
An obsolete local fallback cache (~67 KiB charged) was removed. Heaptrack
preload/injection diagnostics did not complete guarded startup and contribute
no acceptance measurements or claimed allocation stacks.

### Proofs and checks

The final private Hyprland proof covers one/two outputs; calendar input prepared
and presented on each clicked output; closed+idle release; 1.5→1.25 scale;
addressed input at widths 1920/2304 with unknown outputs rejected; removal with
no stale commits; re-add with exactly one fresh session; and another removal.
Closed panels are +1.043/+0.328 MiB above their own two-output bar after two
seconds (bar+5 passes). Clean stop reports no surfaces.
Every niri arm proves initial/changed/returned geometry, commits and buffers.
Stock nested niri 26.04 (8ed0da4) provides one Winit output. Its action help has
no virtual-output create/remove action, and output help configures existing
outputs only; multi-output hotplug remains unproved on that backend. Hyprland
provides the private multi-output lifecycle proof. No live/DRM workaround ran.

Under the desktop heavy-job flock, the private driver invokes
`python3 spikes/measure.py --compositor hyprland --parent headless --bar-ui
--modes bead --runs 1 --virtual120`; niri substitutes `--compositor niri` and
omits `--virtual120`. The committed harness is `spikes/per-output-proof.py`.
Private HOMEs, buses and runtimes, 12 GiB/SwapMax=0 containment, verified scope
membership, memory watchdog, pre-arm swap <64 MiB, and quiet gates remain active.

The complete `.github/workflows/ci.yml` job passes in an isolated Arch container
on the first available authorized CI host: fmt, clippy `-D warnings`, workspace
and trace tests, feature checks, facade and Luau (75 modules), all five Python
suites (54/11/12/10/8), private services/tray lifecycle, and independent framework
cargo-deny graphs. Shell tests: 221 passed/1 ignored; renderer: 28 passed.
The ignored tray supervisor is run separately with a private bus. New regressions
cover presented-frame scale retirement, imported fallback-cache release and
independent output projection/reload/failure; existing single-output tests pass.
Product Rust size: +1,260 / −144 physical lines versus `b972a31`, excluding
`cfg(test)` items, test files and proof examples. No temporary product duplicate,
new system dependency, installed-shell change or live session was introduced.

## B0030 note — 2026-10-23

Notices and the OSD reuse these sessions rather than adding one: each bar session
registers as a driver of the daemon notice service, and the popup stack/OSD are panel
views offered through `UiEvent::CompanionOffer` on the leading session's output
([B0030](0030-notices-in-main-shell.md)). No per-output notice session exists; the
extra-output cost above is unchanged.
