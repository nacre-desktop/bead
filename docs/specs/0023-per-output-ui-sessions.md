# B0023 — Per-output UI sessions

Status: **Resumed — matched desktop relative-memory gate and remaining private proofs**. Architecture unit consumed by
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
   is at most +0.5 MiB, with no unexplained anonymous growth above 1 MiB.
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

No temporary duplicate implementation is planned. The optional single-source
API remains supported compatibility, not a temporary replacement. Any temporary
duplicate introduced later must name its path and an expiry here first:
“delete N days after the replacement is trusted”.

## Initial result — 2026-10-22 (absolute desktop stop superseded)

Source `384b3de` implements independent sessions, output-scoped model/input/commit
routing, teardown, shared weak fonts/images and immutable palette snapshots.
Full `.github/workflows/ci.yml` replication passed in the isolated Arch container,
including all permissive dependency graphs; the three output runtime tests pass.
The Arch image needed `noto-fonts` and `ttf-dejavu` for existing renderer tests;
no host packages changed. CI used a Git bundle to retain fixture-tree identity.
CPU release SHA-256: `136ba835c15437f9a42bdf5dc020031b04a0c84299fc00f8c475bbca6ad0c7ef`.

Private Hyprland used 2880×1800/scale 1.5/virtual 120 Hz. One output had exactly
one session, including the late-name update; adding the second created exactly
one more session and both committed. The first attempt exposed a late-name race;
it collected no memory cells. The corrected run stopped during two-output sampling.

| Metric (MiB) | Result |
| --- | ---: |
| One-output PSS, five-sample median (range) | 23.978 (23.978–23.981) |
| One-output file / anonymous / SHM PSS | 17.841 / 5.465 / 0.672 |
| One-output SwapPss; DRM | 0; unavailable |
| First two-output memory screen | 26.043 |
| Provisional screen increase over one-output median | +2.065 |

The initial harness incorrectly applied the laptop 24.5 MiB stop to the desktop. Its component
breakdown was not retained before the guard raised; the increase cannot yet be
attributed by residency category, and the full settled delta is **not measured**.
No threshold was relaxed and no further capture or optimization was attempted.
Scale/removal/re-add/input/idle proofs and the niri run remain incomplete. Niri
multi-output hotplug is also unsupported by this harness. Both owned runs cleaned
up with zero scope OOM counters; host swap was zero before each arm. No push or
live change. The lead has authorized continuation under the relative gate above;
the historical incomplete screen is not an acceptance result.

Size: +1,011 / −130 physical product Rust lines, excluding test modules/files
and proof examples. No temporary implementation duplicates were introduced.
