# B0031 — Dashboard media and system cards, finish

Status: **Review** (Claude worker, branch `task/b0031-dashboard-media-finish-20261023`). Builds on B0027
(protocol pin `652d6c4f`, no change). It implements the items B0027 listed under "Not done" that the
published Nacre N0033 contracts can carry. Nacre QML (`NacreMediaCard`, `NacreDashboardNetworkCard`,
`NacreNetworkExtra`) is the reference.

## Done

| Item | Behavior |
| --- | --- |
| Seek | Media page "Back 10 s" / "Forward 10 s" buttons send `player.seek {player, position_ms}` from the extrapolated position, clamped to `0..length`. Enabled only when the active player reports `can_seek` and the daemon advertises `player.seek` v1. A slider/drag has no host-action shape in `dashboard.do`, so jumps are the control. |
| Album art | `art_path` decodes once per path to a 440 px square PNG thumbnail (`player_art.rs`: absolute path inside `…/nacre/shell/player-art`, regular file, no symlink, at most 4 MiB, 4096 px side and 4 MP, otherwise the placeholder note). The scene receives only the thumbnail (`@icon:player-art` is swapped for it in `finish_scene`, as tray pixmaps are). It is dropped when the panel is hidden and with the page on idle retirement. |
| Mini player | Today hero shows art, title, artist and prev/play-pause/next while the daemon reports a player (same `can_*` gates, same `dashboard.do` ops). Today leases `dashboard.player` only while the page is open; the one-second wake is **not** started for it (no position shown). |
| Network card | System page replaces the placeholder Disk gauge (it only said "See drives below"; the Drives list stays) with Down/Up rates, two 32-bar sparklines on one scale and the peak, from `dashboard.system.network` (bytes per second). No samples shows "—" and no bars. |
| Wi-Fi `frequency_mhz` | Network page: active access point frequency in the details line (replaces the false "Traffic rates are not published yet") and `Secured · 5180 MHz` in each access point row; omitted when unreported. |

## Not done, with reasons

- **Host card** (user, OS, compositor, uptime, kernel, update count): `dashboard.system` does not publish
  these (N0033 lists the host card as not done). The QML reads `/etc/os-release` and `/proc` itself; Bead
  must not. The System page already shows every published reading (gauges, drives, processes, updates).
- **Screenshot thumbnails**: the `screenshots` topic carries only id and file name, and B0027 keeps the file
  path out of the view by design. No thumbnail path is published; unchanged.
- Spectrum/halo and the QML manual player override (needs the `audio` topic) remain out of scope.

## Memory and idle

Nothing new is resident with the dashboard closed. Art is one thumbnail (about 100–300 KB PNG while a
player with art is shown) plus the renderer's display-size pixmap, released with the page; the in-process
decode is bounded to 4 MP (a transient buffer of at most 16 MB, mmap-backed and returned on free). The
`dashboard.player` lease on Today lives and dies with the Today page factory; the daemon producer is lazy.
The relative memory gate (candidate minus control, same host and backend) was not run: the daemon build
with these producers is not on the desktop. Backend: CPU.

## Validation

`dashboard_more_tests.rs`: seek math and clamping, can_seek/advertisement gating, art decode/reuse/release
and refusals, Today mini player lease without a wake, traffic bars/rates/unknown, frequency text, goldens
(`dashboard-{media-player,system-gauges,system,network}-*` updated, `dashboard-media-art-seek-*` and
`dashboard-today-mini-player-*` new, scales 1 and 1.5). `player_art.rs` unit test covers the path/size
refusals. Goldens do not render enabled `dashboard.do` buttons (the headless harness has no host actions).

## Temporary duplicates

None added.
