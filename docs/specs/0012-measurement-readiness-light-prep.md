# B0012 — Light B2/B3 measurement readiness preparation

- ID: B0012
- Repo: nacre-desktop/bead
- Implementer: fresh built-in Luna worker
- Status: Ready
- Depends on: accepted B0001/B0004/B0003-S and owner-installed wayland-utils
- Date: 2026-10-19
- Size: one focused light session, <=90min

## Goal

Prepare a concrete B2/B3 execution handoff while Nacre fixes F4. Verify the now
installed wayland-info prerequisite and audit existing commands/artifact bindings;
this task has no measurement slot and must not start a compositor or full CI.

## Context

Start current published main5ad2551 (or its docs-only descendants), fresh own
worktree. B0011 sourceaf61d18/CI39147491406 is Done; newer shell product features
must not replace B2 frozen runtime1cff9d17 silently. B2 historical release SHA
b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b,
Nacre5f4 baseline helper4a65f4ec. Read B0002/B0003 specs, existing artifact-preflight
and gate-D source reports, capture_safety and window tests, standing rules.
Owner installed wayland-utils. Missing-executable/sudo blocker is superseded;
prior failed cells remain invalid. F4 publication/switch-on precede B2/B3 captures.

## Scope and steps

1. Create an isolated task worktree from published main. Do not edit Nacre or any
   existing worker branch. Read exact current source and parked artifact report.
2. One bounded READ-ONLY trusted-runner command is authorized: resolve installed
   wayland-info path, package/version information without opening a Wayland
   connection, executable hash, and the existing preserved runtime/helper hashes.
   Invoke the existing private wrapper with shell stdin; don't inspect/copy its
   credentials or alter SSH keys. Do not query the live display, /proc memory,
   idle processes, compositor sockets or desktop configs. No package install.
   If remote access is unavailable, record exact blocker and continue source work.
3. Audit current B2/B3 entry commands and sources for actual2880×1800/scale1.5,
   H120/N60, windows/pairs, keeper, artifact identity, source-proven release build
   profile, and existing guard capability. State each field as source-proven,
   historical or unknown; config requests alone are not actual geometry proof.
   Identify the existing actual geometry proof mechanism or its precise gap.
   Don't create another harness, silently change dimensions, or claim readiness
   from an unknown binary. Owner cap rule is max(baseline peak×1.5,12GiB), with
   host>=4GiB start/abort<2GiB, swap0/verified scope/independent watchdog. Document
   any existing fixed2GiB assumption as a future task requirement, not acceptance.
4. Produce one concise revision of docs/research/b0002-artifact-preflight.md with
   exact source/profile/binary/helper hashes, dependency result, ready-to-run
   command templates, expected output paths, per-compositor time budget, required
   first preflight assertions and remaining blockers. Preserve historical evidence
   labels. Include B3 measured-phase dependency after B2 PASS, existing serial
   query-identity limitations and exact trace-parser tests; don't repeat B3-S work.
5. Report source-only acceptance checklist to lead. Commit locally approved
   identity; NO push/hosted CI without lead review. Only docs/report corrections
   authorized here; if source/tool fixes are needed, give exact proposed changes
   instead of implementing. No product/renderer/fixtures/protocol edits.

## Checks and acceptance

- [ ] Installed dependency verified read-only, or exact access blocker recorded.
- [ ] Frozen runtime/helper profiles and hashes retained, unknown provenance explicit.
- [ ] Existing commands target correct geometry/rates/windows with actual proof
  requirements and owner-sized safety gates; any gap has an exact follow-up.
- [ ] B2/B3 durations/ordering and no-active-slot status are clear.
- [ ] Links and git diff --check pass; no capture/preview/benchmark/build/test suite,
  compositor/helper/wtype launch, package operation or hosted CI occurred.
- [ ] Local docs commit/report ready for lead review; no misleading memory claim.

## Stop conditions and report

Live-session access, missing credential/host identity, artifact mismatch, required
product/tool change or capture request: stop that dependent step and report;
continue independent source prep. Status report: done/blocked, commits/files,
exact read-only commands/results, dependency/artifact fields, command templates,
unknowns and next bounded unit. No performance acceptance inferred.

## Short rules

Commit as siverteh <92440237+Siverteh@users.noreply.github.com>, both identities.
Own worktree; never force/rebase others. nice10/Cargo-j2 if later authorized;
NO heavy slot in this task. No live desktop/installed Nacre/configs/keys/units,
publication, AI permissions/paru changes, other agents' PIDs, pressure/cache/sysctl.
No separate terminal agents. Keep personal paths/raw artifacts out of commits.
