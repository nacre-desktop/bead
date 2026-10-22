# B0019 — Launcher parity

Status: Review — implementation and private proofs complete; performance/memory acceptance open. Owner unit 4, 2026-10-22. No push or live switch.

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
  A failed post-check remains invalid; a two-key functional check may verify a
  harness correction without repeating its timing window.
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

Implementation and the full remote Arch CI equivalent passed on `c8b55b5`.
Native private input and three-cycle close/reopen proofs passed on both
compositors, including pointer pin/unpin and focused input after idle retirement. Query and preference work share one owned actor
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

A bounded heap diagnostic used the existing `trace` feature, without product
changes: after three Hyprland cycles, mallinfo2 live bytes rose 0.262 MiB,
while free arena bytes rose 18.210 MiB. Anonymous PSS rose 2.090 MiB, file PSS
fell 0.765 MiB and shared-memory PSS stayed flat. The process-wide counters
identify allocator residency rather than a matching increase in live heap;
they do not attribute every arena to the launcher. Query actor threads join,
rows/tickets/session retire, and reported closed cache bytes remain 171,708.
Normal-build closed PSS deltas were at most +0.989/+0.079 MiB (Hyprland/niri),
inside own-bar+5 MiB; SwapPss was zero. DRM was unavailable: no absolute laptop
or matched before/after memory verdict. The B2 idle-GC commit `7948c3c` is not
an ancestor of this task's base; integrating it is separate work.

Real-engine validation retained a failed Hyprland arm separately: all 200
serial key traces joined, but the post-window favourite waiter incorrectly
required UI_STATE logging, which clock-only captures do not enable. `c8b55b5`
checks authoritative preference readback and an identity-matched committed/
presented refresh instead. Native fixture input separately verifies the actual
star toggles. A fresh two-key Hyprland contract smoke passed catalog invalidation,
pin/unpin refresh and opaque group activation. It is not a replacement 200-key
performance arm. The earlier query-free reopen failure was also a harness bug:
`nacre.launcher` requires a query; `bead.panel open` is the correct preserved-
input route. Failed raw outputs remain invalid, not discarded or relabelled.

### Diagnostic receipt

| Nested diagnostic (ms) | Hyprland partial window | niri valid arm |
| --- | ---: | ---: |
| Keys / ordered-identity changes | 200 / 199 | 200 / 199 |
| Steady presented p50 / p99 | 9.380 / 9.871 | 5.857 / 6.269 |
| First-key presented / UI CPU | 25.214 / 13.905 | 25.016 / 14.777 |
| Steady UI CPU p50 / p99 | 4.438 / 5.119 | 4.929 / 5.351 |
| Steady daemon / worker CPU p99 | 1.700 / 0.786 | 1.671 / 0.760 |
| Steady RPC dispatch-start-to-reply p99 | 2.581 | 2.728 |

Steady excludes the first key (199 samples). Hyprland's overall arm is invalid
for the post-check failure above; its table column is partial trace evidence.
The corrected two-key Hyprland smoke and complete 200-key niri arm both passed
catalog refresh, authoritative pin/unpin plus UI refresh, and group navigation.
The serial parser rejects overlapping/burst traces instead of guessing joins;
actor unit tests cover overlapping/cancelled/stale completions. Both private
outputs were 2880×1800 at scale 1.5 (Hyprland synthetic 120 Hz / niri 60 Hz);
these are not physical scanout or matched QML comparisons. First-key UI CPU
remains above 8 ms: no Gate E or speedup acceptance is claimed.

End-of-typing niri UI PSS was 47.161 MiB (anonymous 23.266, file 16.437, shared
7.459), daemon 34.812 MiB and one worker 21.066 MiB, reported separately.
SwapPss was zero; DRM was unavailable. These are observations, not absolute or
matched memory clearance. No host swap reset was needed; host swap-in stayed
zero. Valid scopes completed with no OOM and cleaned their owned processes.
The desktop lock was free and no task transient service remained afterward.

Full CI included fmt, warnings-denied all-target Clippy, workspace tests
(nacre-shell 195 passed / 1 ignored), trace-memory and renderer feature checks,
headless/Luau schema, harness suites (53/11/12/10/6), private services/tray and
workspace plus every independent bead-* cargo-deny graph. Source `c8b55b5`
normal release SHA256 is
`c8eed1947b86b18c0452d495d4edd3b8b04a92f61aa17f2e7cb7d7b4d1c76085`;
the heap-only trace build used
`9e6284e7e75f9ca463b229f3090f551f0153b4aafeb111353034baad00b4ac4c`.
Task CI container and all remote source/build/target directories were removed;
logs, manifests and artifacts remain for review. No push, install or live run.
