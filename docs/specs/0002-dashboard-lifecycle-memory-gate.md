# B0002 — Dashboard lifecycle and memory gate

- ID: B0002
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: In progress (owner-activated capture)
- Depends on: source-only preflight eligible after B0004 Done; captures require B0001 Done and exact slot grant
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours including capture after grant

## Early artifact/preflight request — authorized 2026-10-18

With B0004 Done and B0003 source phase accepted, prepare ONLY this request now,
maximum45minutes. B0001 policy blocker remains; no capture/build is authorized.

- Inspect already-existing private local artifacts/receipts without running the
  desktop, helper, compositor or benchmark. Supply available baseline/candidate
  full source and binary hashes, helper75157 exact existing hash/provenance,
  harness revision and whether runtime inputs are byte-identical across the
  tests/docs-only B0001/B0004 series. Never assume binary identity from a branch
  name or relabel an old artifact. If required artifact is absent/stale, report
  it missing; do not build or fetch dependencies to fill the gap.
- Propose private host scope, actual commands/flags and process cleanup plan from
  the existing harness; no remote probing of Nacre's host or live display.
  Calculate duration for5alternating pairs EACH compositor and full60s idle
  windows at every phase; include600s warmups per arm if noisy-baseline mode is
  needed. Identify if the total exceeds the current4h session; propose splitting
  into independently granted compositor windows, without silently reducing
  sample counts/warmups. Request review before changing the method/harness.
- Report the inherited0192policy dependency and absent exact grant as blockers.
  Do not start captures, CI-triggering source changes, builds, previews or product
  edits; docs/report only. No new hardware inventory/credential disclosure.
- This request does not complete B0002 or waive B0001. The spec agent will decide
  exact artifacts/method/window after dependencies are resolved. An idle host or
  a Ready spec is not a grant; neither implementer currently holds one.

## Goal

Prove the completed four-tab dashboard stays inside the existing memory budget
through repeated opens and closes. Attribute retained bytes before spending any
of the small remaining reserve.

## Context

Baseline83f5ecf plus advisory-only patch; candidate reviewed B0001 runtime plus
identical advisory policy. Record exact full hashes/binary hashes; docs-only
changes do not require rebuilding identical runtime. Read dashboard-core-coverage,
research/feature9-gate-d.md, spikes/dashboard-residency.py, spikes/menu-engine-proof.py,
and the linked standing rules. Last single-run settled23.756/24.258MiB is
historical evidence, not a matched baseline for a new candidate.

## Scope

In: private matched capture, report, a small dashboard-only lifetime/narrow-decode
fix if demonstrated. Out: renderer/allocator/backend changes, broad tooling,
new features, memory waivers, typing optimizations, physical tests.

## Steps

1. Before building, coordinate with the spec agent's exclusive schedule. Supply
   exact endpoints, prebuilt artifacts and expected duration. No build/test overlap
   with captures. Inspect the existing harness invocation from the four-tab report;
   preserve assertions and frozen helper75157 bytes, and label provider fixtures.
2. Use BEAD_DASHBOARD_PAGES=today,weather,media,system and residency slots=4 for
   both sides; legacy final phase enabled. Record never-opened, one full pass,
   three full passes and combined legacy navigation. Repeat three open/close
   cycles per tab, record each close after the2s retirement, and report group
   membership, source leases, retained buffers, SHM and DRM when observable.
3. Existing35/55/75/65-second phase markers are diagnostics, not a full60-second
   idle acceptance window. For this gate, collect full60s idle after each phase
   in both arms using the existing measurement helper. If harness adjustment is
   necessary, change only explicit wait/sample duration and add a small assertion
   that idle duration is>=60; never weaken close/presentation/quiet checks.
4. If a reproducible increase appears, test hypotheses in order: (a) lingering page
   subscriptions/module/state/raster ownership: close counts must reach zero;
   (b) broad parser code/file residency: inspect mapped file/PSS deltas, retain
   concrete decoding; (c) buffer/SHM/DRM: correlate bytes with surface geometry;
   (d) noisy host: invalidate paging runs and compare matched warmups. Time-box
   diagnosis to90min. Do not keep tuning unproven allocators or drivers.
5. Report per-phase per-run medians/spreads/peaks, all closes, file/anonymous/frame
   attribution, process audit and artifacts. If B0001 changed no runtime, state
   byte identity and use this session to establish the completed scope's gate;
   do not claim an optimization gain from identical code.

## Measurements

Five alternating A/B pairs EACH on Hyprland virtual120Hz and niri nested60Hz,
2880×1800/scale1.5. Same frozen helper/catalog/palette/fixtures and matched phases.
Use matched10-minute warmups if strict settling is noisy, full60s idle windows
always. Quiet: every measured process SwapPss0, host swap-in<1MiB with page size.
Report UI PSS+DRM and external daemon/helpers separately; unknown DRM remains
unknown. Preserve exact presented-page proof and cleanup. No physical claims.

## Acceptance criteria

- [ ] Every close<=own bar+5MiB; settled<=25MiB and below24.5MiB stop line.
- [ ] Every settled increase attributed; unexplained anonymous>1MiB stops.
- [ ] A pure optimization never raises memory. Repeatable regression blocks landing.
- [ ] No frozen reference/assertion altered; all ten pairs valid or gate NOT PASS.
- [ ] Source fixes, if any, pass normal full CI and new pairs use final exact bytes.
- [ ] Publish concise report and CI; raw/private artifacts stay outside Git.

## Stop conditions

Reaching24.5MiB, unattributed anonymous growth, reproducible regression, paging,
missing exact hashes or90min diagnosis exhausted: stop and report. A one-time
27MiB ceiling is only considered by spec-agent review after demonstrated lazy /
narrow-decoding pass with mostly file/shared remainder; this spec does not grant it.

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

## Report format

Send spec ID/status, start and final full commit IDs, changed files, each acceptance
criterion with PASS/FAIL/NOT RUN and command evidence, published-head CI URLs,
protocol/artifact hashes where relevant, deviations/blockers and raw private report
locations (never commit private paths). For captures include per-run numbers,
medians/min/max/peaks, quiet counters, attribution and owned-PID cleanup. Missing
is unknown. Ask the spec thread directly; do not improvise or ask the owner for
routine design. Submit Review, then start the next eligible Ready spec.

## Early preflight review — 2026-10-18

Accepted reportbb75590128cbf85b57a416ea881b3482ae802729 and exact green
CI39077628859. Correctly identifies runtime-input identity, uncertified cached
artifacts/helper pairing, historical harness/exit130 limits and absent actual60s
observation. Missing provenance is NOT resolved. Counts are10pairs/20cells total;
600s per-cell adds200min. Additional baseline60s must be included in schedule.
Next eligible assignment is B0005 harness correctness with mocked clocks only.
No artifact build/capture grant,0192exception approval or B0002completion.

## Current disposition — 2026-10-18

Preflight, B0005 and B0006 are accepted; no more harness/report polishing is
requested. Capture remains Blocked: B0001 inherited0192owner-policy ruling,
source-proven UI artifacts and certified helper pairing absent, no exact slot.
Do not use cached unproven binaries, repeat historical runs, install a toolchain,
borrow Nacre's host, or change dependency policy to bypass these blockers.
Provide existing provenance only if newly located; otherwise await the missing
prerequisite/authorized artifact-build spec. No new build or capture is granted.

## Active execution authorization — owner update2026-10-18

This section supersedes earlier policy/provenance/build holds. B0001 is Done;
B0004/B0005/B0006 and source preflight are accepted. Execute the existing gate,
folding small harness fixes into it rather than opening a tooling-only spec.

1. In deny.toml and deny-framework.toml retain0192's existing reason and append:
   revisit when cosmic-text/fontdb/usvg move off ttf-parser, before loading
   untrusted font files, or by2027-01-31. Include its RustSec link. Do not remove
   other exact IDs or broaden ignores; use the updated standing-rules delegation.
2. Prepare release artifacts in an existing isolated build environment. Owner's
   proportional provenance rule is commit+build profile+binarySHA256 in report;
   no extra attestation/archive chain required. Build only missing artifacts,
   nice10/Cargo-j2, and use existing pinned dependencies. No installed desktop,
   new perf-lab setup, credential copying or toolchain/config installation.
   Cached binaries still need honest source/profile identification, not relabelling.
3. Use current reviewed Bead runtime (75bd50b or later with only docs/harness /
   approved deny-reason edits) built as release. B0001/B0004 runtime inputs equal
   baseline83f5ecf, so a common exact release binary may serve both repeated arms;
   record runtime identity and make no feature-speedup claim from identical code.
   Use helper75157 if its existing record gives commit/profile/hash; otherwise
   explicitly use published Nacre5f4c0bf3 release helper4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa
   in BOTH arms as a separately named current-daemon lane. Original75157 artifacts
   and historical measurements remain immutable. Bead never edits Nacre source.
4. Exclusive private/nested measurement grant:2026-10-18 00:55–10:55UTC, existing
   Bead nested lab or existing private desktop lab, no physical output/live bus.
   Exact private route/identity is delivered in the agent message, never public
   inventory. Verify the selected already-trusted host, log commit/release/hash,
   and notify spec agent/Nacre CAPTURE_START before the first arm; no further
   permission prompt is required once those prerequisites pass. During build /
   preparation the capture host is not measured. No other build/test job may
   overlap an active capture. Use own isolated task worktree and private tmux.
5. B0002 first: five alternating pairs EACH compositor,20cells total, full600s
   matched warmup per arm and60s baseline/phase observation, accepted B0005/B0006
   harness,2880×1800/1.5 Hvirtual120/Nnested60. Plan separate sequential compositor
   windows, about3h15 each within the total grant; no truncation or reduced pairs.
   If remaining grant cannot fit a full cell, stop before it and request a revised
   spec-agent window. Quiet/memory/presentation/ownership/cleanup rules unchanged.
6. Send each compositor's concise result and final B0002 Review immediately; do
   not wait to write a separate ledger. On passing criteria, continue to B0003's
   measured diagnostic/paired steps in the remaining window, same host/artifact
   recording rules. No blanket product-optimization permission: send the measured
   bottleneck and exact proposed code change for an immediate product-fix spec.
   If a memory/quiet/measurement blocker stops this gate, report it promptly and
   continue independent existing typing source/test preparation, not report churn.
7. Release the measurement host as soon as ready so Nacre N0002 can take over.
   Nacre source work does not block this grant; avoid its unrelated checkout and
   installed helpers. Gate grants only private nested work, never desktop changes.

Report commit/profile/hash, exact method and per-run values/medians/spreads/peaks,
quiet counters, explained memory deltas and cleanup. No slot-only commit or extra
provenance report. Follow hostedCI after normal integration; advisory reason edits
are policy-only and need not block source-identical artifact preparation.

## Capture safety — restart rule, 2026-10-18

This section supersedes earlier capture windows. No capture starts until incident
review is reported to the owner and the spec agent issues the replacement grant.
An unexplained simultaneous agent shutdown is not proof of OOM or a safe retry.

- “Stop other jobs” means pausing your own builds and asking the other implementer
  to pause theirs. Never kill a process you did not start. Never signal or restart
  Codex/app-server processes. An expired window is not proof of host idleness;
  require explicit DONE/IDLE/RELEASED before another task starts.
- Before each arm: no builds running, peer pause acknowledged, and MemAvailable
  at least 4 GiB (4,194,304 KiB in /proc/meminfo). Record the value. During the
  entire warmup and capture sample MemAvailable at least once per second; abort
  and invalidate the arm if below 2 GiB (2,097,152 KiB), or if monitoring fails.
- Run the harness and all measured descendants (private compositor, UI, daemon,
  helpers and input driver) inside one new private systemd user scope. Set an
  MemoryMax=max(12 GiB, 1.5× the recorded baseline scope peak when known), plus
  MemorySwapMax=0. Supply a known peak as the strict positive integer byte value
  BEAD_CAPTURE_BASELINE_PEAK_BYTES; otherwise use the 12-GiB floor. Verify both
  effective systemd properties and cgroup memory.max/memory.swap.max before
  releasing the held child; no unlimited fallback. Scope setup is transient and
  task-private, not a live unit/configuration change.
- Keep the lightweight memory watchdog outside that measured scope, but task-owned.
  On low memory, scope OOM, unexpected exit or lost watchdog, invalidate the arm
  and stop only the recorded owned scope/descendants. Preserve partial logs,
  cgroup memory.events/peak and the reason; never retry automatically or count
  aborted samples as a valid cell. Do not include Codex/app-server in the scope.
- Verify containment and watchdog behavior with a bounded mocked check before
  measurement; do not deliberately exhaust RAM or induce host memory pressure.
  Fold the small guard and regression tests into the measurement spec, not a new
  tooling project. Report scope name, limit, start/minimum MemAvailable, guard
  result, exact artifact identities and owned cleanup with ordinary gate evidence.
- Before each acceptance arm, save IPC output metadata and assert the actual
  required profile: Hyprland bead-bench 2880×1800, scale 1.5, 120 Hz; nested
  niri 2880×1800, logical 1920×1200, scale 1.5, 60 Hz. A requested config alone
  does not establish geometry.
- Inspect kernel and user journals from 2026-10-18 02:30 with explicit timezone
  and retained-boot coverage. Report OOM/kills, agent/app-server exits and the last
  owned job without touching those processes. If evidence points to another
  cause, fix it and tell the owner before re-granting. Missing old journals mean
  unknown, not no OOM. Never change global OOM policy, swap, sysctl or caches.

## Replacement schedule — 2026-10-18 restart

The old00:55–10:55 window is withdrawn. Reserve2026-10-18 11:00–19:00UTC
for B0002 on the previously verified private nested desktop lab only. This is
a conditional reservation, not permission to bypass the missing wayland-info
prerequisite or incident review. Before CAPTURE_START send READY with existing
probe path, incident finding reported to owner, explicit Nacre pause, verified
4GiB available, sized scope with zero swap and tested low-memory guard, then obtain the
spec-agent activation of this reservation. No installation is authorized here.

Artifacts: reviewed source1cff9d17d61fb05451546bb0ea13c826c93cb4c9, release UI
b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b,
and5f4c0bf3 release helper
4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa
in both arms, using the accepted B0005/B0006 harness plus tested safety guard.
Record final harness commit and hashes; guard changes must not alter sampling,
presentation assertions or the five-pair-per-compositor/600s+60s method.
Failed pre-compositor cell is invalid. Do not reuse any historical partials.

Release early on completion/blocker. If readiness is delayed or full cells cannot
fit, request a revised actual window; never compress windows, overlap or start
from an expired reservation. B0003 follows only on B0002 PASS and its own safety
preflight. Nacre correctness work is scheduled separately after explicit release.

## Incident closed — owner decision, 2026-10-18

Owner closed the incident review: no OOM/systemd-oomd kills or relevant kernel
crash entries; likely lost Codex connection. This is the owner's disposition,
not proof from the unavailable earlier laptop journal. Retain all capture-safety
rules. Closure supersedes incident-review activation holds; it does not waive
containment, memory, artifact, quiet or explicit peer-pause checks.

## Active grant — supersedes earlier reservations and holds

Owner activates B0002 Oct18 11:00–19:00UTC, then B0003 19:00–21:00UTC after
B0002 PASS, on the previously verified private nested lab. No further incident
or spec-agent activation approval is required. Prepared runtime artifacts stay
unchanged: Bead source1cff9d17 release SHA
b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b;
Nacre5f4 release helper SHA
4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa.
Use guarded harness4d0861d or later reviewed guard-only fix, record its commit.

Perform required memory/scope/peer-pause checks, then announce CAPTURE_START.
Minimal private-lab setup of the missing wayland-info prerequisite is within
owner's go-now direction; do not install a new toolchain, upgrade the system,
change the harness probe or touch live desktop settings. Record that preparation
before matched arms. Actual privilege/dependency failure is a blocker to report,
not permission to weaken assertions. Continue independent typing source/tests
if blocked. Keep exact five-pair-per-compositor sampling and full windows.

Release early and transfer only by explicit DONE/IDLE/RELEASED. Nacre N0002
is next when its reviewed candidate is ready. If fewer full cells fit, request
an immediate spec-agent window adjustment; never truncate or silently overlap.
