# B0024 — Dashboard pages and extras parity

Status: **In progress** (Claude worker, branch `task/bead-dashboard-parity-20261022`). Number: B0023 is
reserved by the per-output-sessions worktree; this is the next free number.
Nacre protocol pin was `294d3f40` for this unit; B0027 moved it to `652d6c4f` and closed blockers B1-B5 (see the table below). Gap-list unit 6 of
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
| Focus page/extra (index 4) | Blocked here; done in B0027 (B2). |
| System gauges, GPU memory, host card, sparkline, network traffic | **Blocked** (B3). |
| Clipboard, Screenshots, AI usage, Brain extras | Blocked here; done in B0027 (B4). |
| Customization (tab order/enable, lyrics/palette/photo toggles) | Read side published (`settings.store.dashboard`, `settings.store.dashboard-extras`); not consumed here (size). Write side **blocked** (B5). |
| Weather photography/video, overview cards | Deferred optional; photos need a local-media contract (B1 art path). |

## Blockers: exact contract needs (nothing here is guessed in code)

Status after Nacre N0033/N0034 (consumed by [B0027](0027-dashboard-daemon-contracts.md)).

| Blocker | Contract | Status |
| --- | --- | --- |
| B1 player | `dashboard.player` + `player.play_pause\|next\|previous\|seek` (N0033) | **Done in B0027**: Media page playback card (title/artist/album, status, local position extrapolation, previous/play-pause/next). Seek, album art, halo, mini player and overview media card remain open. |
| B2 focus | `dashboard.focus` + `focus.start\|pause\|resume\|stop` (N0034) | **Done in B0027**: Focus page with a local countdown. |
| B3 metrics | extended `dashboard.system`, `AccessPoint.frequency_mhz` (N0033) | **Partly done in B0027**: CPU/memory/GPU gauges. Network traffic sparkline, frequency, host card and GPU memory card remain open. |
| B4 extras | `clipboard`, `screenshots`, `ai.usage`, `brain.recent` (N0034) | **Done in B0027**: four pages with gated copy/clear/open/refresh. Screenshot thumbnails remain open. |
| B5 customization | `settings.set` on `dashboard-extras` + `settings.store.dashboard-extras` (N0034) | **Done in B0027**: revision-fenced tab enable/order. Lyrics/palette/weather-photo toggles remain open. |

The original contract wishes are kept in the Nacre specs `0033` and `0034`; the field shapes above
were met as published, not as originally proposed here (for example `focus.start` takes `{minutes}`).

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
