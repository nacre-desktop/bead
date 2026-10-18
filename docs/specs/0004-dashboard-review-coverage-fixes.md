# B0004 — Dashboard review coverage fixes

- ID: B0004
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Done
- Depends on: B0001 submitted Review at989bfbea4e70cd6b875d3b417f3922717d984b55
- Date: 2026-10-17
- Size: one focused session, maximum90minutes

## Goal

Close two concrete evidence gaps in B0001 without changing dashboard behavior.
Every covered page needs accurate proof for ready-state defaults and disconnects.

## Context

Starting commit989bfbea4e70cd6b875d3b417f3922717d984b55 plus this docs-only spec.
Reviewed implementationcec1ef6 and published-head CI39072672938 success.
The41-row inventory exactly matches Nacre5f4c0bf3 and source release proof is
accepted as scoped: callback/Arc ownership, not elapsed timer or GPU memory.
Review found defaults test only projects unavailable sources; disconnect test
only checks Weather, yet inventory Media row cites it. Read B0001 and its linked
standing rules. B0001 stays Review until this follow-up is accepted.

## Scope

In: dashboard_tests.rs and accurate coverage/review docs. Out: runtime product
code, golden changes, providers, new framework/hooks, measurements or new deps.
Preserve existing drafts, all prior timing/memory references and advisory policy.
Any required advisory-policy decision stays owner-only; this test assignment
neither extends an exception nor certifies broader policy compliance.

## Steps

1. Extend dashboard_four_tabs_have_complete_state_defaults to check BOTH the
   existing no-data unavailable state AND ready synthetic fixture state for each
   Page::ALL. Today has its calendar primary/weather secondary; System includes
   updates; Weather/Media use their concrete fixtures. Validate fixture shapes
   through existing checked() before projection. Assert every emitted dashboard
   key exists in that page's defaults; non-Today cannot emit dashboard.clock.
   Include Today's existing clock/store projection path using available fake
   source/Stores setup; do not invent a clock API or widen declarations to hide a
   faulty projection. Preserve existing ready/preview assertions where applicable.
2. Parameterize dashboard_disconnect_keeps_last_good_stale over all four primary
   page sources. For each: fresh fixture is ready and not stale; disconnect keeps
   its last-good user-visible fields/rows while setting stale. Retain no-data
   unknown/not-stale case. Supply existing secondary fixtures for Today/System
   so their unrelated missing provider does not confuse primary disconnect proof.
   No product fix here: if assertions expose a real defect, report exact failing
   page/field and stop for a separately specified runtime fix.
3. Correct inventory proof links to assertions that actually exercise the named
   behavior. In particular Media disconnect must point to the newly parameterized
   test, not Weather-only evidence. Do not infer numeric system metrics from
   state-key/default tests. Preserve the41 source rows and explicit optional /
   provider-blocked distinctions; do not call current core full QML parity.
4. Update the B0001 report with review fixes and exact checked commit/results.
   Keep B0001 Review and submit B0004 Review; spec agent marks Done. Do not start
   B0002 capture until its separate grant. Normal tests-only integration/CI is
   authorized under existing rules, no product behavior publication implied.

## Measurements

None. No native20, residency, typing, preview or runtime memory/latency capture.

## Acceptance criteria

- [ ] Both ready/unavailable projections for four pages have declared defaults.
- [ ] Four primary disconnect cases retain last-good data and visibly mark stale.
- [ ]41-row inventory still exact; Media/other proof claims match actual asserts.
- [ ] Runtime files/fixtures/PNG goldens unchanged; no weakened assertions.
- [ ] Focused tests, fmt/Clippy and published-head required CI pass. Existing ignored
  benchmark cases remain honestly ignored; no speed/memory claim.

## Stop conditions

Runtime defect, unsupported projection/clock API, a required schema/provider /
policy change, or90min exceeded: report precise gap; do not expand scope.

## Report format

Spec B0004 Review; starting/final full commits, modified files, test commands /
results and exact published-head CI URL; list each page/state covered and any
remaining failing behavior. State no runtime/fixture/golden/measurement change.

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


## Spec-agent review — 2026-10-18

Done. Reviewed full3da4856→ef5406f diff, including197fe51 and56210a6 code;
all four ready/unknown default checks, Today clock/stores and all four primary
last-good/stale disconnect cases meet the requested fixes. Inventory proof links
now match assertions. Only test/docs files changed; fixtures/goldens/runtime
unchanged. Independently verified code CI39074426606 and final-docs
CI39075073594 success at their exact commits. Implementer clarified validation
was CI-only (local Cargo absent); no local-test claim is accepted for B0004.
No new memory/timer/GPU proof or measurement grant.


## Restart safety

No measurements are authorized by this spec. Any later measurement must follow
the standing capture-safety rules: peer pause, MemAvailable >=4GiB before start,
abort below2GiB, private memory-limited scope, and explicit spec-agent grant.
