# B0027 — Dashboard consumes the N0033/N0034 daemon contracts

Status: **Review** (Claude worker, branch `task/b0027-dashboard-contracts-20261022`).
Protocol pin moved from `294d3f40` to Nacre `652d6c4f` (N0034 `9c9fc76e` plus N0033 `e20ab83b`;
both are additive, `cargo update -p nacre-protocol` locked no other package). Closes
B0024 blockers B1-B5 as far as listed below; Nacre QML (`nacre/shell/modules/dashboard/*`) is
the reference. Daemon-less behavior is unchanged: pages stay "unavailable" and nothing calls.

## Pages, indices and sources

Nacre registry order is kept (`nacre tab N`): 4 focus, 6 clipboard, 9 ai-usage, 10 screenshots,
11 brain join B0024's 5/7/8. Each page leases only its own topic in the page factory and drops it on
idle retirement (B0002 lifecycle): `dashboard.focus`, `clipboard`, `screenshots`, `ai.usage`,
`brain.recent`; Media also leases `dashboard.player`; every page leases
`settings.store.dashboard-extras` for tab visibility. `Page::Customize` (`view.open
dashboard-customize`, no slot index) is the customization page.

| Page | Shows | Calls (only while the method is advertised at v1) |
| --- | --- | --- |
| Focus | state, `mm:ss`, progress, DND note | `focus.start {minutes}` (15/25/50 min choice), `pause`, `resume`, `stop` |
| Clipboard | previews, kind | `clipboard.copy {id}`; `clipboard.clear {}` after an explicit confirm step |
| Screenshots | file names only (path never reaches the view) | `screenshots.open {id}` |
| AI usage | provider lines | `ai.usage.refresh {}` on the Refresh button only, never on a timer |
| Brain | recent notes, open tasks | none (no contract) |
| Media | adds the playback card: title/artist/album, status, position/length, progress | `player.play_pause\|next\|previous {player}`, player = the topic's `active`, gated by its `can_*` |
| System | CPU/memory/GPU gauges from the extended topic; unknown stays "—" | none |
| Customize | enabled/order of the eight optional pages | `settings.set {store:"dashboard-extras", key:["enabled"\|"order"], value, rev}` |

Rules shared by all calls (`dashboard_controls.rs`): one call in flight, never retried or replayed; row
ids are resolved against the current topic before a call (callers never invent ids); the daemon error
text is never shown (fixed "Request failed"/"Changed elsewhere; try again"); no optimistic update, the
topic is authoritative. The single host action is `dashboard.do` with `op[:id]`, authorized as
`dashboard.control`.

Tabs: four core tabs plus the enabled optional pages in `order`, limited to topics the daemon
advertises (preview fixtures show all enabled); the open page is always shown. Customize writes the
whole new array with the store's current `rev`, so a concurrent change fails as `stale_revision`
instead of overwriting; a store with a read error blocks writes. The footer "Customize pages" link
appears only when `settings.set` is advertised and the store is readable. Core pages keep the exact
old layout when nothing is enabled (existing goldens unchanged).

## Timing and idle behavior

The daemon owns the focus timer and publishes `deadline_ms` once. The view counts down locally: a
one-second wake thread exists only while a session runs in an open panel and is released when the
panel closes, the session stops, or the page retires. Media position is extrapolated the same way while
`playing` (the wake is shared). No topic polls and AI usage is read only on request.

## Not done (one line each)

Seek, album art, mini player and network sparkline moved to B0031. Halo/overview card; host card,
GPU memory card; screenshot thumbnails (copy/trash actions); weather photo, lyrics and palette toggles;
Focus dial ring and saved default minutes. A `--dashboard-fixtures` run shows artificial data only.

## Validation

`dashboard_more_tests.rs` (fixtures with the pinned protocol types, fake daemon over a private socket)
and the updated `dashboard_tests.rs`: state defaults, row projection, unknown/invalid/stale data,
source release on retirement, countdown wake lifecycle, tab model, exact call params, gating when the
method is absent, confirm step, revision fence, can-flag gating, goldens at scales 1 and 1.5
(`shell/tests/goldens/dashboard-{focus,clipboard,ai-usage,screenshots,brain,customize,media-player,
system-gauges}-*.png`). Full Bead CI job on a remote host; no nested desktop smoke (no daemon build with
these contracts on the desktop), so live behavior against the real daemon is unverified.

## Temporary duplicates

None added to Bead. Nacre's QML/helper duplicates (N0033/N0034) expire 7 days after this consumer is
trusted on the owner desktop.
