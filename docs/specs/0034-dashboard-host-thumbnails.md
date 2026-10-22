# B0034 — Dashboard host card and screenshot thumbnails

Status: **Review** (Claude worker, branch `task/b0034-host-thumbs-20261023`). Moves the nacre-protocol pin
from `652d6c4f` to `3499bcbe` (Nacre N0038, unpublished), which adds `dashboard.system.host` and
`screenshots[].thumb_path`. Finishes the two items B0031 left "not done". Reference: `NacreHostCard.qml`.

## Behavior

| Item | Behavior |
| --- | --- |
| Host card | System page, bottom right beside the updates row (which is now half width). Lines, in the QML order: user, OS (`Linux` when the daemon omits it), compositor (constant `Hyprland`, as in the QML), uptime, kernel. Each member is optional and omitted when unknown; no `host` block, no card. The update-count line is the existing updates row. |
| Uptime | `now − boot_ms` computed at projection (`Up 1d 2h 3m` / `Up 2h 3m`, the QML wording, i18n ids `bead.dashboard.host-*`), else the fixed `uptime_s`. It refreshes on the System page's existing daemon ticks; hidden panel or other page schedules nothing. |
| Thumbnails | Screenshots rows show a 72x48 thumbnail (placeholder icon on any failure) beside the file name. `thumb_path` is accepted only as an absolute path directly inside `…/nacre/shell/screenshot-thumbs`, a regular non-symlink file of at most 1 MiB and 4 MP (same checks as B0031 `player_art.rs`, now shared through a `Cache` descriptor). It is decoded once per (id, path) to 144x96 (twice the display size), only for the at most six listed rows. The view receives the pixels through the scene (`@icon:shot-<id>` swapped in `attach_art`); the path never enters the model. |
| Release | Thumbnails are dropped when the panel hides and with the page on retirement; ids no longer listed are dropped on each projection. |

## Memory and idle

Closed dashboard: no new cost (no host rows, no thumbnails, no wake). Open on Screenshots: at most six
144x96 PNGs (about 5–25 KB each as held bytes) plus the renderer's 144x96 RGBA pixmaps (55 KB each, at most
about 330 KB) and a transient decode buffer of at most 256x256x4 = 256 KB per file. The daemon makes
thumbnails only while the topic is subscribed (N0038). The relative memory gate (candidate minus control
on one host/backend) was not run; backend: CPU.

## Validation

`dashboard_more_tests.rs`: host rows and local uptime, absent host block, thumbnail decode-once /
outside-cache / symlink refusal / release on hide, goldens `dashboard-system-gauges-*` (host card) and
`dashboard-screenshots*` (placeholder) updated, `dashboard-screenshots-thumbs-*` new. `player_art.rs`
unit test unchanged in behavior.

## Temporary duplicates

None added. The QML host card's own file reads stay until this is trusted (Nacre N0038 rule: delete a week after).
