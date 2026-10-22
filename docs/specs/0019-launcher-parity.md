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
Category navigation means engine-returned menu groups and their activation
routes. The pinned protocol's `category` field belongs to software discovery;
it does not publish an application taxonomy. Do not invent one in Bead.

Read QML from Nacre origin/main (launcher modules, NacreMenuQuery rowsFit /
sameScope, NacreMenuContext). Keep product style in shell/theme; framework crates
receive no Nacre colours, dimensions or behavior assumptions (ADR 0004). Record
existing assumptions encountered in ROADMAP's Generalization list.
The existing Nacre protocol pin `961de0a3` documents `menu.preferences` with
`{action:"favorite",value:{id,enabled}}`; it is the favourite mutation path.

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
  on the remote build host, tmux, nice -n 10, Cargo -j2; no laptop tests/builds. Remove task containers
  and build directories afterwards. No Nacre desktop job exists in this repo.
- Nested desktop smoke under desktop-heavy.lock, private HOME/session only:
  focused first typing, grid/list/category/favourite behavior, burst-query
  fencing, keyboard activation, preserved reopen and joined motion. Respect
  swap/containment/watchdog/source/window gates for any captures. Measurements
  are reported only if actually run; old B0003 numbers remain historical.
- Evidence departure from the default five-pair method: at most one candidate
  200-key diagnostic per compositor may validate the new identity/RTT trace.
  Report first key and steady typing separately. These candidate-only runs
  cannot establish a speedup, matched memory verdict or Gate E acceptance.
- Run fmt, warnings-denied Clippy, tests, feature/schema/service/license proofs.
  Lead reviews diff and output before committing; no push. Author and committer
  use the owner's approved identity; messages carry [no-publish].

## Stop conditions and temporary duplicates

Stop for unpublished required contracts, unsafe host conditions or owner-only
decisions. Preserve lazy closed+idle release and standing memory gates; desktop
memory comparisons use the accepted relative rule, never claim laptop clearance.
No new parallel launcher implementation is planned. The existing fixture Source
is an intentional test input, not a second provider. The audited launcher path
has no legacy helper menu-backend lane. Any temporary duplicate introduced during
implementation must be named here with its trust condition and a concrete
"delete N days after the replacement is trusted" expiry before completion.

## Result

Implementation and focused remote checks are complete; full CI and nested
validation remain pending. Query and preference work share one owned actor
thread; scalar input survives retirement while rows/tickets release. No
temporary duplicate was introduced. Existing kit palette/icon-font defaults
and host action assumptions are already listed in ROADMAP's Generalization list;
this unit adds no framework look, dimensions, colours or launcher keymap.
Remote container additions: DejaVu, Noto and Noto emoji fonts for the existing
Fontconfig-dependent runtime tests; no desktop packages changed.

Bounded painting inspection: `bead-render/src/tree.rs::paint_with_scratch`
already skips invisible rounded items, narrows damage and handles opaque
interiors separately. Larger first-key damage is a plausible explanation for
the historical rounded span, not a measured cause. Source inspection identifies no
cheap improvement; renderer code stays unchanged, avoiding an unproven cache
and its memory cost. The historical 28 ms first key is not a candidate result.
