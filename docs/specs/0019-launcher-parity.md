# B0019 — Launcher parity

Status: In progress. Owner unit 4, 2026-10-22. No push or live switch.

## Outcome and boundaries

Match the current Nacre QML launcher: focused search on open, catalogue grid
and search list, categories, visible favourite stars with published pin/unpin,
daemon ordering, keyboard navigation/activation, and preserved input on reopen.
Retain previous rows while a same-scope query is pending; never show another
category's rows or a transient empty-state message. Open/close moves the search
and body as one container. Use published menu.query/menu.activate contracts only.
Calculator/software/theme/wallpaper modes and live integration are out of scope.

Read QML from Nacre origin/main (launcher modules, NacreMenuQuery rowsFit /
sameScope, NacreMenuContext). Keep product style in shell/theme; framework crates
receive no Nacre colours, dimensions or behavior assumptions (ADR 0004). Record
existing assumptions encountered in ROADMAP's Generalization list.

## Implementation and evidence

- Own launcher Source/projection, shell launcher UI and action wiring; preserve
  engine ranking and opaque activation/revision fields. No local catalogue or
  providers. Add focused tests for pending same-scope retention, cross-scope
  clearing, stale replies, favourites, navigation, activation and reopen state.
- Close B0003 trace identity/RTT gaps where the existing path permits: correlate
  query, item IDs and timed RPC send/return with commits/presentation. Keep CPU
  and elapsed wait distinct. Rounded-row painting gets only a bounded inspection;
  no optimization claim without matched A/B and memory evidence.
- Full Bead .github/workflows/ci.yml equivalent in a task-owned Arch container
  on dev-1, tmux, nice10, Cargo -j2; no laptop tests/builds. Remove task containers
  and build directories afterwards. No Nacre desktop job exists in this repo.
- Nested desktop smoke under desktop-heavy.lock, private HOME/session only:
  focused first typing, grid/list/category/favourite behavior, burst-query
  fencing, keyboard activation, preserved reopen and joined motion. Respect
  swap/containment/watchdog/source/window gates for any captures. Measurements
  are reported only if actually run; old B0003 numbers remain historical.
- Run fmt, warnings-denied Clippy, tests, feature/schema/service/license proofs.
  Lead reviews diff and output before committing; no push. Author and committer
  use the owner's approved identity; messages carry [no-publish].

## Stop conditions and temporary duplicates

Stop for unpublished required contracts, unsafe host conditions or owner-only
decisions. Preserve lazy closed+idle release and standing memory gates; desktop
memory comparisons use the accepted relative rule, never claim laptop clearance.
No new parallel launcher implementation is planned. Existing fixture/daemon
Source lanes remain for tests: delete the legacy helper menu-backend lane 7 days
after published daemon mode is trusted at the core switch, subject to owner
removal approval. Measurement fixtures remain tests, not replacement providers.

## Result

Pending implementation and remote validation.
