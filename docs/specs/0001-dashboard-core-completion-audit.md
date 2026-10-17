# B0001 — Continue the core dashboard

- ID: B0001
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Review
- Depends on: exact scoped zbus advisory exception committed; published-head CI green
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

## Goal

Finish the read-only behavior of the four current core dashboard tabs and make
remaining QML coverage explicit. A tab existing is not proof that its content,
navigation, unavailable states and resource retirement match the core switch.

## Context

Start at83f5ecf plus the agent's exact-ID advisory exception and docs-only setup;
record full hashes. Nacre reference is5f4c0bf3; read it without editing.
Read docs/STATUS.md, ROADMAP.md, research/feature9-gate-d.md (Gate D, Gate E,
four-core-tabs and green-compatibility sections), and
[standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md).
Existing shell/dashboard/{main,behavior,components,weather,media,system}.luau,
today/{page,behavior}.luau and crates/nacre-shell/src/dashboard.rs already supply
Today/calendar, Weather, Media enrichment and System/Updates. Protocol pin5f4 is
schema compatibility, not implemented daemon providers.

## Scope

In: traceable inventory of the41 QML components, deterministic completeness tests,
read-only empty/stale/disconnect and navigation fixes within existing four tabs.
Out: new tabs/extras/customization, focus mutation, Brain/AI, player/MPRIS,
provider hosting, refresh calls, device controls, frameworks/renderers/backends,
new protocols, measurements. Do not port all41 as41 separate Bead components.

## Steps

1. Add docs/research/dashboard-core-coverage.md, with one row for every QML file
   under Nacre's nacre/shell/modules/dashboard at reference5f4. Record exact count
   and names (brief's41 is a reported count; explain any difference). Columns:
   source component, user behavior, current Bead equivalent, test proof, disposition.
   Dispositions: current-core covered / current-core gap / deferred optional /
   blocked by published runtime provider. Group shared widgets by behavior without
   losing source rows. Add no additional ledger or time estimate.
2. Current core means Today clock/date/calendar/agenda + weather summary; Weather
   current/hourly/daily; Media existing enrichment/lyrics/palette only; System
   process/filesystem + Updates. Keep four tab IDs and legacy routing unchanged.
   Explicitly defer extras (AI/Brain/clipboard/screenshots/notifications/devices /
   network extra cards), Focus mutations, customization, weather photography/video,
   player controls/art decoder. Existing network/device panels remain untouched.
3. In crates/nacre-shell/src/dashboard_tests.rs add
   dashboard_four_tabs_have_complete_state_defaults (every projected key belongs
   to that page's declared defaults; non-Today never emits dashboard.clock),
   dashboard_navigation_routes_every_core_tab (tab IDs/legacy routes reach same
   page and close returns to bar), dashboard_disconnect_keeps_last_good_stale
   (unavailable without data stays unknown; disconnect with data marks stale),
   dashboard_long_rows_scroll_without_clipping (bounded long event/process/forecast
   fixtures have reachable last row), dashboard_closed_page_releases_sources
   (on explicit Runtime retirement/drop, no page subscriptions/state/raster leases).
   Reuse existing bead-test Harness/Source; no fake-clock or generic harness API.
4. Fix failures only in dashboard.rs, dashboard_tests.rs and shell/dashboard
   modules; nacre_source source-lease/reload integration only when a named failing
   lifecycle test proves necessity. Keep per-page concrete decoding and selective
   module loading; no all-CoreSnapshot deserialization. Update explicit labeled
   synthetic fixtures/goldens only for intended UI fixes, with before/after images.
5. Update STATUS and feature9 report with bounded results, coverage and unresolved
   providers. If tests reveal no product defect, tests + coverage is sufficient:
   do not add ornament, new optional pages or invented values to fill scope.

## Clarification: separate resource release from retirement timing

The2200ms close grace belongs to crates/bead-wayland/src/panel.rs close_panel(),
not Runtime or bead-test::Harness. B0001 does not require a fake clock, worker
scheduler hook, wall-clock sleep or framework change.

- Implement dashboard_closed_page_releases_sources by creating the actual page
  Runtime with counting/fake sources, observing acquired subscriptions/leases,
  then explicitly dropping/retiring it and checking they return to baseline.
  Cover Today secondary weather/clock and System secondary updates as well as
  Weather/Media. Weak-reference/drop counters may prove page state/source release;
  keep the bar/shared client alive so a shared owner is not falsely called a leak.
  Do not assert driver-memory reclamation from Rust object drop.
- Existing service_views_tests::panel_retirement_releases_launcher_mirror_but_bar_does_not
  remains the analogous launcher/bar resource-ownership test. It is NOT an
  integration test of elapsed2200ms. Cite it accurately in the coverage report.
- Cite close_panel()'s2200ms Timer and epoch/token/reopen guards as source wiring,
  with the exact source revision. If an already-recorded native lifecycle proof
  covers matching source bytes, link it as historical end-to-end evidence; do
  not claim a new timing run. Fresh actual close-delay/lifecycle acceptance is
  B0002 under its own measurement grant, not a prerequisite for this unit test.
- No bead-test/framework/backend edits are authorized just to expose time. If
  dashboard Runtime resources fail release, fix that demonstrated dashboard /
  reload ownership path only, preserving the current worker grace.

## Measurements

None authorized by B0001. Run deterministic headless tests and image assertions;
request a later slot only if product changes require runtime memory validation.
Do not run native20, residency, typing or a preview against a live display.
Runtime fixes cannot be declared memory-accepted until B0002's matched gate.

## Acceptance criteria

- [ ] All source dashboard components have a disposition and concrete proof/gap.
- [ ] Named tests pass; unknown values remain unknown, fixture data labeled.
- [ ] Four current tabs/routes and joined-rim placement retained; selective decode
  and first-open/2s-close retirement preserved.
- [ ] No fabricated provider capability, new mutation, optional feature or new dep.
- [ ] fmt, warning-denied Clippy, workspace/trace tests, CPU-only/all-features,
  service boundary, Luau and cargo-deny CI pass on published main. Only the exact
  three approved advisory IDs are excepted; review trigger is2027-01-31 or earlier
  consumer zbus6 support, not the older proposed October review date.

## Stop conditions

A current-core gap needs an unpublished protocol/provider, optional-scope decision,
framework redesign, a >4h implementation, or memory acceptance: document exact
blocked behavior and ask spec agent. Do not expand core-switch scope yourself.

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
