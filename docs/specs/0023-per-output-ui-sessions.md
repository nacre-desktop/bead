# B0023 — Per-output UI sessions

Status: **In progress**. Architecture unit consumed by
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
4. Compare settled one/two-output bar PSS, reporting delta (target well under
   +3 MiB/output), anonymous/file-backed, SwapPss and DRM (unknown stays unknown).
   Use matched geometry/renderer and report individual samples. This unit's
   incremental one/two-output comparison is a bounded diagnostic, not the
   default five-pair core-switch acceptance gate. Preserve all capture swap,
   containment/watchdog/quiet gates and the 24.5 MiB settled stop line.
   Stop on failed safety gates or unavailable authorized hosts; no laptop tests.
5. Update STATUS, this result and B0022's architecture blocker. Report added /
   removed product lines. Full CI precedes normal safe-push integration; never
   publish a package or alter the installed shell.

## Duplicates

No temporary duplicate implementation is planned. The optional single-source
API remains supported compatibility, not a temporary replacement. Any temporary
duplicate introduced later must name its path and an expiry here first:
“delete N days after the replacement is trusted”.
