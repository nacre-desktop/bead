# B0025 — Per-output UI sessions

Status: **Blocked on authorized CI-host capacity; fixes and diagnostics await validation**. Architecture unit consumed by
[B0022](0022-bar-composition-lifecycle.md); action-contract blockers there remain.

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

## Owner continuation — 2026-10-22

The measured +2.044922 MiB per extra output passes the owner's two-output
acceptance rule; the former two-output +0.5 MiB verdict below is historical
and superseded. Attribute the +1.433594 MiB anonymous difference using actual
per-session allocation counters or heap profiling (behavior/markup Luau heaps,
layout/scene storage, text/image caches and shared-memory pools). Avoid retaining
immutable resources per session when a small sharing fix suffices.

Niri scale-phase increases are a product defect to fix, not a host exception.
Trace old-scale allocations and release them safely; repeat five alternating
control/candidate niri pairs, requiring at most +0.5 MiB median in every phase.
Try nested niri multi-output hotplug and document an unavailable backend route.
Use the authorized CI host for builds/full Bead CI; desktop captures retain the same private HOME,
heavy-job flock and all swap/watchdog/containment gates. After passing checks,
update this result/B0022/queue and integrate through safe-push. No live change.
No new temporary product implementation duplicate is planned.

## Continuation checkpoint — 2026-10-22

Candidate `05e0986` (integrated with session-menu main `c72d217`) replaces each
CPU geometry pool with three aligned current frames, retires unused-scale
raster/node/frame and glyph/image caches, and preserves the shared FontSystem.
The new regression checks that presented frames survive retirement and that
returning to the original scale preserves raster pixels. Read-only review found
no remaining rendering hazard, but **Rust compilation/tests have not run**.

Allocation diagnostics now report behavior Luau bytes, compiled markup and value
charges, temporary markup-sandbox end-of-build bytes, source capacities, global
cache admission and glibc live/free/mapped heap bytes at settled phases. The
markup sandbox is dropped after building; the previous suggestion of a retained
markup VM was incorrect. Layout's Taffy arena is temporary too. No new allocation
values are claimed: the +1.433594 MiB anonymous attribution remains pending a
capture, as does any further cheap sharing optimization justified by those values.

The installed nested niri reports version 26.04 (8ed0da4). Read-only
`niri msg action --help` has no virtual-output create/remove action;
`niri msg output --help` configures only existing outputs. This harness produces
one Winit output and no supported private hotplug route was found. Niri scale
proof remains runnable; Hyprland supplies private multi-output lifecycle proof.
No DRM/EDID or live-output workaround is authorized.

The authorized CI host remained at its task-container cap while the build worker
waited about 28 minutes. No container/build/test/capture started in this
continuation. Single-file proof syntax and `git diff --check` passed; fresh Rust
fmt/CI, allocation attribution, niri matched scale medians and safe-push remain
blocked externally. Task staging is removed; previous evidence stays private. Current candidate
size is +1,078 / −145 physical product Rust lines versus main `c72d217`,
excluding test modules/files and proof examples.

## Previous result — 2026-10-22

Source `60896a0` implements session ownership, scoped model/input/commit routing,
shared weak fonts/assets and immutable palette snapshots. The late output-name
race preserves the original session; opening a companion now also preserves the
in-flight owning session instead of creating a duplicate. Proof revision
`8f4aad5` fixes newline handling in the final log parser. These failures remain
in private evidence; corrected runs use fresh directories. No temporary product
implementation duplicate was introduced.

The original absolute desktop stop was incorrect and is superseded by the
matched relative gate. Control `b83adee` has identical product source to current
main `d1f46c7` (intervening changes are CI/docs only). Both binaries use Rust
1.99.0, default features, `cargo build --release -p nacre-shell --locked -j2`
and `--bar --core-views --renderer cpu`. Control SHA-256:
`048a497d3d002d08c18290e3626f9d4ac074ce6866fa0e8810fa249f6789eb03`;
candidate: `14511e6db0e6c34bab35efb2cd992d0d888a2731090c07cdc48a130e872cc642`.

Final matched UI-process PSS (MiB): each cell is the median of five arm
medians, with their min–max range. Every arm uses 10 s warmup and five 1 s
samples per phase; geometry matches between roles. The complete arm medians
and raw rollups remain in private `comparison-final.json` and capture evidence.

| Phase | Control median [range] | Candidate median [range] | Delta | Gate |
| --- | ---: | ---: | ---: | --- |
| Hyprland one output, 1.5 | 23.945312 [23.721680–24.031250] | 24.214844 [24.136719–24.502930] | +0.269531 | PASS |
| Hyprland two outputs, 1.5 | 24.836914 [24.725586–24.980469] | 26.256836 [26.193359–26.540039] | +1.419922 | FAIL |
| niri one output, initial 1.5 | 25.269531 [25.082031–25.461914] | 25.712891 [25.690430–25.832031] | +0.443359 | PASS |
| niri one output, 1.25 | 24.767578 [24.574219–25.206055] | 25.651367 [25.628906–25.766602] | +0.883789 | FAIL |
| niri one output, return 1.5 | 25.014648 [24.817383–25.665039] | 25.996094 [25.972656–26.111328] | +0.981445 | FAIL |

| Candidate minus control residency (MiB) | Anonymous | File | SHM |
| --- | ---: | ---: | ---: |
| Hyprland one output, 1.5 | +0.308594 | +0.016602 | +0.000000 |
| Hyprland two outputs, 1.5 | +1.433594 | -0.016602 | +0.000000 |
| niri one output, initial 1.5 | +0.269531 | -0.020508 | +0.187500 |
| niri one output, 1.25 | +0.265625 | -0.023438 | +0.691406 |
| niri one output, return 1.5 | +0.230469 | -0.024414 | +0.824219 |

**Relative gate: FAIL.** Two-output anonymous growth is +1.433594 MiB versus
control and is not fully attributed. Extra session VM/view/layout ownership
is expected, but the existing Luau counter omits the markup VM and cannot
account for every anonymous byte. Niri scale phases retain more SHM than the
control; these are measured residency differences, not proof of buffer causality.

Separately, candidate per-extra-output PSS costs across the five arms are
2.044922 / 2.056641 / 2.037109 / 2.041992 / 2.072266 MiB:
**median +2.044922 MiB** (target below about 3 MiB passes). Residency delta:
anonymous +1.375000, file −0.001953, SHM +0.671875, SwapPss 0 MiB.
The control extra-output median is +0.880859 MiB. Category medians are reduced
independently and need not add exactly to the median total delta. SHM is
measured shared-mapping residency, not the total allocated buffer byte count.

The private Hyprland functional proof passes at 2880×1800/virtual 120 Hz:
one/two outputs, prepared and presented calendar input on each clicked output,
closed+idle release after two seconds, extra-output scale 1.5→1.25, addressed
input widths 1920/2304 with unknown outputs rejected, removal with no stale
commits, re-add with exactly one fresh session, and a second removal. Closed
PSS deltas from the two-output bar are −0.291/+0.260 MiB (own-bar+5 passes).
The re-added settled endpoint is 28.126 MiB; this lifecycle run includes warmed
panels and is separate from the matched bar-only memory cells. Stop reports
zero surfaces. The nested niri backend supports one window/output here; its
1.5→1.25→1.5 geometry/commit/buffer proof runs in every matched arm. Niri
multi-output hotplug remains unproved; no live session was used.

Commands: under the desktop heavy-job flock, `python3 relative-batch.py proof`
invokes `python3 spikes/measure.py --compositor hyprland --parent headless
--bar-ui --modes bead --runs 1 --virtual120`; `python3 relative-batch.py memory`
runs five alternating control/candidate pairs per compositor (niri omits
`--virtual120`). The orchestrator is private evidence; the committed proof is
`spikes/per-output-proof.py`. Each arm uses a private HOME/D-Bus/runtime and the
existing 12 GiB/SwapMax=0 guarded capture. All 20 final matched arms and the
completed lifecycle proof have swap below 64 MiB before launch, zero process
SwapPss, host swap-in and OOM deltas;
DRM residency is unavailable, never treated as zero.

Full `.github/workflows/ci.yml` replication on `8f4aad5` passed in the isolated
Arch CI container: `CARGO_BUILD_JOBS=2 nice -n 10 bash /task/ci-prod-run.sh`
(exit 0). This includes fmt, denied-warning clippy, 204 passed/1 ignored workspace
tests, trace memory-policy tests, feature checks, facade, 58 Luau modules,
Python suites (54/11/12/10/8), private service/tray-bus recovery and all independent
permissive cargo-deny graphs. The three output runtime tests cover scoped
projection, moved-window titles and independent state/reload failure; existing
single-output tests remain green. Build prerequisites add `noto-fonts` and
`ttf-dejavu` inside the CI container for existing renderer tests, with no host
package changes. CI log SHA-256:
`9c0d6f39f1005035153a6c1597b3990edc90b59b064c783265363b86411c99ed`.

B0022's runtime implementation blocker is replaced by failed memory acceptance;
its unpublished daemon action contracts remain independently blocked. No push,
live change or package publication. Size: +1,014 / −132 physical product Rust
lines versus the control, excluding test modules/files and proof examples.
