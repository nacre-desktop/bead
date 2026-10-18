# B0002 — Dashboard lifecycle and memory gate

- ID: B0002
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Review (early artifact/preflight only; capture blocked)
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
