# B0024 — Dashboard pages and extras parity

Status: **In progress** (Claude worker, branch `task/bead-dashboard-parity-20261022`). Number: B0023 is
reserved by the per-output-sessions worktree; this is the next free number.
Nacre protocol pin unchanged (`294d3f40`). Gap-list unit 6 of
`docs/research/core-switch-gap-20261022.md`.

## Objective

Close as much of the dashboard gap as the published daemon contracts allow, and
write down exactly what the rest needs. Core Today/Weather/Media/System/Updates
(B0001–B0004) are unchanged. Nacre's QML registry order is
`today weather media system focus notifications clipboard devices network ai-usage screenshots brain`;
`nacre tab N` indexes that list, so extras keep those indices (4..11).

## Contract inventory (Nacre origin/main `nacre-protocol::core_services`)

Published topics: `network`, `bluetooth`, `power.profiles`, `battery`, `audio`,
`notifications`, `osd`, `tray`, `dashboard.{weather,updates,calendar,media,system}`;
published read-only `settings.store.<id>`. There is no MPRIS, focus, clipboard,
screenshot, AI, Brain or metric topic.

| QML page/card | Disposition |
| --- | --- |
| Notifications extra (index 5) | **Implement, read-only**: `notifications` topic (DND state, newest-first history/active rows). Dismiss/clear methods exist but are not wired here (B0020 owns notification actions). |
| Devices extra (index 7) | **Implement, read-only**: `bluetooth` paired/connected devices + `audio` output/input nodes. Connect/disconnect/"use" actions exist (B0010/B0015) and are not wired in this unit. |
| Network extra (index 8) | **Implement, read-only**: `network` primary connection, active signal, connections, access points. Traffic sparkline/frequency blocked (below). |
| Media player/MPRIS, mini player, album art, halo, overview media card | **Blocked** (B1). Existing enrichment-only Media page stays. |
| Focus page/extra (index 4) | **Blocked** (B2). |
| System gauges, GPU memory, host card, sparkline, network traffic | **Blocked** (B3). |
| Clipboard, Screenshots, AI usage, Brain extras | **Blocked** (B4). |
| Customization (tab order/enable, lyrics/palette/photo toggles) | Read side published (`settings.store.dashboard`, `settings.store.dashboard-extras`); not consumed here (size). Write side **blocked** (B5). |
| Weather photography/video, overview cards | Deferred optional; photos need a local-media contract (B1 art path). |

## Blockers: exact contract needs (nothing here is guessed in code)

- **B1 player**: topic `dashboard.player` `{health, active?: EntityId, players: Entities<{identity, status: playing|paused|stopped, title, artist, album, art_path?: local cached file, position_ms?, length_ms?, can_control, can_play_pause, can_next, can_previous, can_seek}>}`; methods `player.play_pause|next|previous {player}`, `player.seek {player, position_ms}`. Unknown position/length must be omitted, never zero.
- **B2 focus**: topic `dashboard.focus` `{health, status: idle|running|paused|done, total_s, deadline_ms?, remaining_s?, owns_dnd, previous_dnd}`; methods `focus.start {minutes}`, `focus.pause`, `focus.resume`, `focus.stop`. Today this lives behind the helper `dashboard-data` tool, not the daemon.
- **B3 metrics**: extend `dashboard.system` with optional `cpu_percent`, `memory {used,total}`, `gpu {percent?, memory_used?, memory_total?}`, and `network: [{down_bps, up_bps}]` samples (bounded count, unknown omitted); `AccessPoint.frequency_mhz?`.
- **B4 extras**: `clipboard` `{health, entries: Entities<{kind, preview, created_ms}>}` + `clipboard.copy {id}`/`clipboard.clear`; `screenshots` `{health, items: Entities<{path, created_ms}>}` + `screenshots.open {id}`; `ai.usage` and `brain.recent` read topics with per-provider/note rows. N0029 is a terminal helper, not a topic.
- **B5 customization writes**: `dashboard-extras` is QML-publisher-only; Bead needs either `dashboard-extras` in `SettingsProviderWrite` or `settings.set {store,key,value,rev}` accepting it.

## Implemented design

- `Page` keeps `ALL` = the four core pages (existing tests/goldens unchanged) and adds `EXTRAS`
  (`notifications`, `devices`, `network`); `Page::from_tab(index)` maps Nacre indices, so
  `nacre tab 5|7|8` opens them and 4/6/9..11 stay refused as unavailable.
- Same lazy model as core pages: one panel view per page, sources leased in the page factory
  (`notifications`; `bluetooth`+`audio`; `network`) and dropped with the page on idle retirement.
  Decoding only the needed `CoreSnapshot` variant; invalid or unavailable data stays "unavailable".
- Navigation shows the four core tabs; an extra page appends its own selected tab. No tab-enable
  customization yet (B5), so extras are reached by `nacre tab N` or a bound route only.
- No new actions: extras are read-only; the page action allow-list stays `view.open` (+ calendar).
- Preview fixtures `shell/tests/fixtures/dashboard-extras/{notifications,bluetooth,audio,network}.json` are
  artificial and labeled "Preview data".

## Acceptance

Existing dashboard/commands tests pass unchanged; new tests cover state defaults, unknown/invalid
data, stale-on-disconnect, source release on retirement, route mapping and deterministic pixels
(goldens at scale 1 and 1.5) for the three extras. Full Bead CI on a remote host. No live action.

## Temporary duplicates

None added.
