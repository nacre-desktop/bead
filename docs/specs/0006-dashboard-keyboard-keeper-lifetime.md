# B0006 — Keep input capabilities stable through acceptance windows

- ID: B0006
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready
- Depends on: B0005 Review at9f77da20dbb67d227737b63b39a82816bbab637a
- Date: 2026-10-18
- Size: one focused session, maximum90minutes

## Goal

Keep the existing private keyboard keeper alive through the longer acceptance
arm. A600s warmup cannot safely use a keeper that exits after600s and changes
input capabilities as baseline measurement begins.

## Context

Reviewed B0005 timing/warmup implementation46b2a99, hosted test stepbb98474,
final report9f77da2; exactCI39079511622/39079931459 verified success. Actual60s
observation, fixed budgets, prelaunch validation, legacy behavior and existing
rails are accepted as source/fake-clock evidence. No real capture performed.

spikes/launcher-proof.py starts wtype -s600000 before warmup and checks alive
only immediately.600000ms=600s; acceptance warmup600+baseline60+phase350 is
already1010s before small overhead. The keeper can expire before baseline ends.
Existing finally stops its exact owned keeper. No binary/provenance/policy gate
is resolved by this source finding.

## Scope

In: launcher-proof.py, dashboard-residency.py only for a small supplied lifetime
check callback, existing fake-clock test file/report docs. Out: UI/Rust/runtime,
protocol/dependencies, new keepalive process/framework, real wtype/compositor /
helper launch, sampling or builds. ExistingCItest step should pick up new tests.

## Steps

1. In acceptance mode only, set the existing wtype keeper sleep to1800000ms
   (30min), retaining600000ms in legacy mode. This bounds it above the expected
   per-arm17min warmup+observation schedule; do not make it indefinite. Keep exact
   owned-PID cleanup on success, assertion, error and cancellation. Never change
   system keyboard configuration or use a different capability setup per arm.
2. When a keeper is requested, assert it is still alive after warmup/before
   baseline, after baseline, before and after each phase60s observation, and
   before success cleanup. If it exits, invalidate the arm and run existing
   cleanup; no restart mid-arm, capability substitution or preferred-value rerun.
   A private callback from launcher-proof into residency timing helpers is fine;
   no generic process supervisor/continuous monitor/logging framework. If no
   keeper is requested, do not create one. Preserve other existing assertions.
3. Add deterministic tests in the existing hosted test file that exercise actual
   acceptance keeper command selection, pre/post-window lifetime checks and
   cleanup control flow. Mock Popen/poll/stop and fake clock; do not run wtype,
   sleep, read proc or attach display. Include: acceptance_keeper_outlives600warmup;
   legacy_keeper_duration_unchanged; early_keeper_exit_invalidates_and_cleans;
   keeper_not_requested_adds_no_process. Prove a keeper expiring at600s cannot
   produce a valid baseline/phase result, and cleanup targets only owned objects.
   A small helper around existing lifecycle glue is acceptable; no new framework.
4. Run full fake-clock tests, syntax and diff check, integrate normally and verify
   exact hostedCI executes them. Update harness/test hashes and report the30min
   bound/liveness checks and remaining missing source-linked binaries/0192/grant.
   Keep prior seven-test source evidence; distinguish added coverage, no runtime
   memory or physical/input proof. Do not rebuild UI/helper artifacts.

## Measurements

NONE. Mocked-clock/process objects only; no real sleeps/launches/remote probes /
previews/captures. No slot or host reservation is granted.

## Acceptance criteria

- [ ] Acceptance keeper30min vs legacy10min; no unwanted keeper when absent.
- [ ] Early death before/during baseline or phase invalidates; no mid-arm restart.
- [ ] All owned cleanup paths preserved, no external process/config changed.
- [ ] Mock tests/syntax and exact-headCI pass including added lifecycle cases.
- [ ] New hashes/report clear; no binary certification/policy/capture acceptance.

## Stop conditions

Need supervisor/framework/runtime change, actual process to make mocktest pass,
new duration override or altered acceptance rail, or90min exceeded: report exact
issue. Do not use the30min process limit as permission for any real run.

## Report format

B0006 Review: start/final commits, harness/test hashes, mocked tests/counts and
exactCI URL; command/liveness/cleanup evidence; no real launches/builds/capture;
remaining policy/provenance/grant blockers. B0005 remains Review until accepted.

## General rules

Apply the linked standing-rules.md in full. Author and committer use the approved
GitHub noreply identity; no private paths or inventory. Isolated worktree, direct
normal push to main, inspect outgoing history, check CI after every push; rebase
only own commits. Commands nice -n 10; Cargo -j2; private/nested tests only.
No live desktop/install/config/keybinding/autostart/unit changes, package/tag /
release publication, permission-policy changes, or paru skipreview changes.
Stop only owned PIDs; no host pressure/cache drops/kernel changes. Nacre commits
include [no-publish]. Framework remains permissive, publish=false, backends frozen
except fixes; no launcher engines in Bead. Preserve immutable fixtures. Extras
are report-only. Any new security advisory goes to the spec agent; vulnerabilities
go to the owner. Measurement requires an explicit exclusive slot grant.



