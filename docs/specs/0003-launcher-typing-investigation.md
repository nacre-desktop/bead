# B0003 — Launcher typing: isolate the dominant cost

- ID: B0003
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready (source phase accepted; measured steps after B0002 pass)
- Depends on: source-only preparation eligible now after B0004 Done; captures require B0001/B0002 Done and exclusive grant
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

## Early source-only phase — authorized 2026-10-18

B0004 is Done and B0001 functional evidence accepted; B0001's inherited0192
policy criterion and B0002 capture gate remain open. To keep the queue useful,
start ONLY Steps1–2 and the static portions of Step5 now, maximum3hours.
Read existing source and immutable historical traces; no builds, profiling runs,
input injection, previews, benchmarks, memory captures or product edits.

Write one concise report section mapping each typing stage to exact source
file/function and clock/counter provenance, ownership/queue boundary, and existing
trace evidence or UNKNOWN. Rank the four hypotheses from existing evidence;
explicitly distinguish serial populated-result edits from bursts/stale queries,
and changed-result presentation from key receipt/commit. End with one proposed
bounded next experiment and the missing evidence that would change that choice.
Historical timing values stay historical, not a fresh baseline or gain claim.
If existing evidence cannot rank causes, explain the gap rather than inventing
CPU attribution. Do not implement an optimization or relax a target.

Submit a source-preparation checkpoint for review; this does not complete B0003.
Capture Steps3–4 remain prohibited without a separate exact-hash/host/window
spec-agent grant and the listed capture prerequisites. No active grant exists to
either implementer; Nacre is doing correctness-only N0004 work, not holding a
capture slot. The owner grants physical/live tests, the spec agent grants private
nested slots. No automatic prerequisite/measurement waiver is implied here.

## Goal

Identify the dominant cost preventing8ms UI CPU/key and16.7ms presented p99.
Produce one measured explanation and an exact small follow-up recommendation,
without stacking speculative optimizations.

## Context

Start from reviewed B0002 runtime commit; record full hash. Read Gate D/E and
latest typing sections of research/feature9-gate-d.md and spikes/typing-proof.py,
menu-engine-proof.py, process-clock.py. Recent ~10ms UI CPU /25–35ms p99 values
are historical and do not satisfy the targets. Keep engine/helper75157 and frozen
parity sources immutable; use a separately named current-daemon lane if needed.

## Scope

In: source review, existing stage instrumentation and controlled private captures,
one diagnosis report. Out: product optimizations, renderer/backend/allocator
changes, launcher catalog/ranking/providers, new queue layers, physical tests.
The next implementation spec will choose the exact fix from this evidence.

## Steps

1. Map input receipt→query dispatch→reply/decode→projection→Luau evaluation→layout /
   render→commit→presentation. Identify each timestamp clock/source; CPU is whole
   UI-process user+system across the input-to-presentation window, not elapsed
   latency. Daemon CPU and transport wait must remain separate.
2. Rank hypotheses: (a) projection/Luau/layout rebuild dominates UI CPU; (b) reply
   decode or repeated string/allocation dominates; (c) daemon/transport queues
   dominate wall time without corresponding UI CPU; (d) missed frame deadline /
   compositor pacing creates33ms tails. Use existing stage traces to distinguish;
   a long presentation wait alone is not slow render or proof of RPC cost.
3. Request slot with source/binary hashes and120min maximum capture window.
   First run one200-key diagnostic/compositor with stages enabled,320 synthetic
   apps and alternating populated-result edits. Preserve changed-result IDs,
   exact update→commit→presentation linkage. Select the dominant stage only if
   the traces support it; conflicting traces require reporting uncertainty.
4. Run five alternating pairs/compositor comparing clock-only and stage-enabled
   observation of IDENTICAL runtime to quantify instrumentation overhead. Both
   arms same real menu.query daemon and query order. Include first key separately
   and in all-key totals; no cache-warm/no-result substitution. This is a profiling
   overhead comparison, not a speedup claim. Time-box source attribution90min.
5. Add a concise report section with whole-path diagram, per-stage CPU/wall and
   missing spans, cold/steady numbers, alternative explanations and proposed file /
   function change for one next spec. No product fix in this assignment. Keep
   stale-result/burst handling and serial typing evidence distinct.

## Measurements

Private Hyprland virtual120Hz/niri nested60Hz,2880×1800/1.5; five alternating
200-key pairs each, prebuilt identical artifacts. Exact presented-update proof,
UI CPU/key median/mean/p99/max, presented p50/p99/max, helper CPU and RTT reported
separately; never add percentile components. Quiet SwapPss0/host swap-in<1MiB.
Full60s idle memory screen each arm; matched10min warmups if noisy. Source and
binary hashes, min/max spreads, peaks and cleanup required. No physical claim.

## Acceptance criteria

- [ ] Complete path and clock provenance; instrumentation overhead quantified.
- [ ] Ten valid pairs and exact changed-result proofs, or explicitly incomplete.
- [ ] Ranked evidence supports one bounded next change or explains inconclusiveness.
- [ ] Memory/quiet rules unchanged; no product patch or claim targets are accepted.
- [ ] Docs-only report commit passes published-head CI. If a future fix reaches
  UI CPU<=8ms and p99<=33ms after fair effort but misses16.7ms, escalate as owner
  candidate with options/costs; do not accept it on the owner's behalf.

## Stop conditions

Stop after4h, a quiet/memory failure, missing clock provenance, new capability /
protocol need or exhausted capture slot. Do not extend the slot unilaterally.

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

## Source-only phase review — 2026-10-18

Accepted source-preparation checkpoint at214b0f0bda7107192a304669fa9de8b61623f832.
Exact-head CI39076370494 independently verified success; source clock/span sites
spot-checked. Report correctly distinguishes process/thread CPU, nested overlapping
spans, unmeasured enqueue/transport/decode attribution, serial-versus-burst proof,
historical data and nested presentation. Ranking is provisional, no dominant-cost
or target acceptance. Docs-only delta; no build/capture/product change.
B0003 remains incomplete/Blocked for capture dependencies and explicit grant.
No further static report polishing requested. Next eligible work is B0002's
limited artifact/preflight request phase, not a measurement.

## Measured execution — owner throughput update2026-10-18

Source analysis is accepted; do not rewrite it. After B0002 passes, measured
Steps3–4 are authorized in the remaining Bead-exclusive00:55–10:55UTC window
on the same existing private nested lab. Commit/release profile/binaryhash suffice
for provenance. Use the B0002-selected helper consistently in all diagnostic /
clock-only arms; label any new5f4lane without altering frozen75157 references.
Existing method/quiet/memory rules remain. Capture only a complete cell within
remaining time, request extension from spec agent if needed, and release early.

Fold any small observation/harness correction into this spec with its relevant
test; no standalone tooling spec or extra paper trail. Send first useful evidence
promptly so spec agent can specify one bounded launcher PRODUCT fix while the
measured work continues. Do not spend the whole slot polishing the diagnosis or
repeat source audits. Optimization acceptance still needs matched before/after
and existing memory/typing gates; owner typing candidate boundary unchanged.

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
  explicit MemoryMax well above expected peak: 2 GiB for the existing nested
  lanes, expected below 1 GiB. Verify the effective cgroup limit before any arm;
  no unlimited fallback. If expected peak cannot fit with generous headroom,
  stop for spec-agent sizing instead of silently raising it. Scope setup is
  transient and task-private, not a live unit/configuration change.
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
- Inspect kernel and user journals from 2026-10-18 02:30 with explicit timezone
  and retained-boot coverage. Report OOM/kills, agent/app-server exits and the last
  owned job without touching those processes. If evidence points to another
  cause, fix it and tell the owner before re-granting. Missing old journals mean
  unknown, not no OOM. Never change global OOM policy, swap, sysctl or caches.

## Replacement schedule — 2026-10-18 restart

Reserve2026-10-18 19:00–21:00UTC on the same private nested lab, after B0002
PASS and explicit host release. The former window is withdrawn. This reservation
requires spec-agent activation after incident review and the capture-safety
preflight above; no automatic start on clock expiry or incomplete B0002.
Use the B0002 reviewed source/release/helper with recorded exact hashes and
final tested guard/harness commit. Preserve the existing ten-pair diagnostic
method and targets. If B0002 ends early, ask to move this window earlier; if
prerequisites fail, keep source preparation and no captures.
