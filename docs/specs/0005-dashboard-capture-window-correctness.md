# B0005 — Dashboard capture-window correctness

- ID: B0005
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: In progress
- Depends on: B0002 early preflight accepted atbb75590; NO capture grant
- Date: 2026-10-18
- Size: one focused session, maximum2hours

## Goal

Make the existing residency harness capable of observing the required full60s
idle windows and optional600s per-arm warmup. Verify timing logic with fake
clocks only, without launching a measured process or claiming performance.

## Context

Start bb75590128cbf85b57a416ea881b3482ae802729 plus this docs-only spec.
Preflight docs/research/b0002-artifact-preflight.md and exactCI39077628859
reviewed success. Source links for cached UI/helper binaries remain missing;
no artifact is selected/certified and inherited0192owner decision is unresolved.
Read B0002, standing rules, spikes/dashboard-residency.py,
spikes/launcher-proof.py (before bar baseline) and bar-matrix.py::idle.
Current capture() calls matrix.idle(pid,1); phase() sleeps remaining fixed budget
then captures. Sleeping60s and measuring1s would not provide a60s observation.

## Scope

In: two existing Python harness files, one stdlib fake-clock test file, one
minimal .github/workflows/ci.yml verification step and report/spec docs. Out: Rust/QML/runtime, deps/advisory policy, new framework/general
scheduler, source-linked binary builds, new remote/host setup, real benchmark /
preview/compositor/helper launches, measurement or live access.

## Steps

1. Add explicit acceptance mode BEAD_DASHBOARD_ACCEPTANCE=1. Existing unset legacy
   mode must retain original behavior for prior scripts; record mode in output.
   Acceptance requires BEAD_DASHBOARD_RESIDENCY=1, four current pages/four slots /
   legacy combined phase as B0002; validate mode options before any process launch.
   Optional BEAD_DASHBOARD_WARMUP_SECONDS accepts0or600 in acceptance mode,
   default0. Any other value fails visibly; never interpret32s engine settling
   as600s warmup. No reduced acceptance-idle value can be configured.
2. In launcher-proof.py after source/presentation readiness and before baseline,
   apply optional600s quiet warmup on the same owned UI/helper processes, then
   matrix.idle(pid,60) for the baseline in acceptance mode. No simulated warmup /
   extra process/caches policy. Preserve existing non-acceptance2s baseline.
   Record requested and measured elapsed warmup separately using monotonic time;
   assert measured>=requested and baseline idle seconds>=60. This executes only
   in a future granted real run; fake-clock tests exercise it now.
3. In dashboard-residency.py parameterize capture(label, idle_seconds=1) so every
   per-close diagnostic retains its existing1s check and each acceptance PHASE
   endpoint calls the existing matrix.idle(pid,60), asserting returned seconds>=60.
   Preserve all state/presentation/SwapPss/ownbar+5/25MiB and24.5stop assertions.
   Do not replace a60s measurement with sleep+1s sample. Memory remains endpoint
   PSS/DRM; CPU spans full interval. Do not claim continuously sampled memory peaks.
4. For matched-age acceptance endpoints use fixed phase budgets65/85/105/95s
   (never/once/several/combined): these are existing35/55/75/65+30s. After activity,
   assert elapsed<=budget-60, wait only budget-elapsed-60, then observe60s. Thus
   activity allowance is unchanged, idle time becomes a real observation, and
   control/candidate endpoints remain matched. If activity overruns, fail; never
   shorten idle or silently extend an arm. Keep old budgets/assertions when unset.
   Record phase budget, activity, pre-window wait and actual idle duration in raw
   rows/progress/method text. Preserve owned cleanup on assertion/error.
5. Add spikes/tests/test_dashboard_capture_windows.py using unittest/mock and
   fake monotonic/sleep plus stub matrix/state/source inputs. Exercise actual
   harness timing paths without importing/running a compositor or measuring PIDs.
   Tests: acceptance_full_idle_not_sleep_then_one_second; acceptance_warmup_precedes_bar;
   acceptance_phase_overrun_rejected; acceptance_both_arms_have_same_schedule;
   legacy_mode_preserves_existing_windows; invalid_duration_fails_before_launch.
   Assert exact call order/count/durations and unchanged close/swap/budget failure
   propagation; no real sleeps, subprocesses, display/bus or /proc observations.
   A small private helper for timing/config reuse is acceptable; no generic clock
   framework or copied algorithm that merely mirrors the implementation in tests.
6. Run the stdlib mocked tests and syntax compile only (no Rust build/capture).
   Normal source-harness/test/docs integration and required hosted CI are allowed.
   Preserve old immutable private harnesses/binaries/receipts; publish new tracked
   harness hashes and update preflight command with both acceptance options.
   Clarify count:5pairs per compositor=10pairs total=20cells; no20pair claim.
   Recompute schedule estimate including baseline60s in addition to phase windows.
   If3h/compositor budget no longer fits, report revised duration before any grant.

## Measurements

NONE. Fake-clock unit tests only; do not run measure.py, launcher-proof native
sample, native20, memory sampling, real sleeps, preview or benchmark subprocess.
No binary build/provenance repair or remote host probing authorized here.

## Acceptance criteria

- [ ] Acceptance baseline and each phase use actual matrix.idle60s; close checks
  remain1s and previous presentation/memory/quiet/stop rails unchanged.
- [ ] Optional600s warmup before baseline, fully recorded, validated before launch.
- [ ] Both arms matched phase schedules; overruns fail instead of shortening idle.
- [ ] Mocked tests and Python syntax pass; no real process/sampling/sleep occurred.
- [ ] Runtime/dependencies/goldens/fixtures untouched; old artifacts preserved.
- [ ] Hosted CI passes on published exact commit; new harness hashes/method/report
  are explicit. This does not complete B0002 or resolve0192/binary provenance.

## Stop conditions

Need runtime/dependency change, general harness rewrite, real probe to make a test
pass, changed validation rail, unknown timing semantics, or2h exceeded: report
exact gap. Never infer capture permission from this implementation assignment.

## Report format

B0005 Review; full start/final commits and new harness hashes; test commands /
PASS counts and CI URL; timing call-order proof; before/after schedule and total
cell/window calculation; explicit no real build/measurement/launch/host probe.
List unresolved provenance/policy/grant gates for B0002 separately.

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



## Hosted regression coverage clarification — 2026-10-18

The new timing tests must run in hosted CI; generic green jobs that omit them are
not evidence for this acceptance criterion. Authorize one minimal step in the
existing .github/workflows/ci.yml check job, using already-available Python:

```sh
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_dashboard_capture_windows.py
nice -n 10 python3 -m py_compile spikes/dashboard-residency.py spikes/launcher-proof.py spikes/tests/test_dashboard_capture_windows.py
```

No new packages, actions, toolchain, permissions, triggers, jobs or remote access.
Keep tests fake-clock/no subprocess/display/bus/proc/real sleep. Include this step
before the existing runtime service proof; do not weaken or replace other steps.
Do not commit bytecode. Report local seven-case result separately, and final
exact-head CI including the new step. No capture/build grant is implied.
