# B0003 — Launcher typing: isolate the dominant cost

- ID: B0003
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready
- Depends on: B0002 Done; exclusive measurement grant for profiling/captures
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

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
