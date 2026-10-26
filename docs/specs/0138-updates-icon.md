# B0138 + N0083 — updates remain visible after an update

The enabled Bead updates widget stays in the bar at count zero, displays `0`
like QML, and retains the updates popout action. Positive counts keep their
existing badge. Nacre refreshes the read-only count when the launched updater
exits and after pacman local-database changes settle for two seconds; it uses
the existing inotify watcher and makes no periodic filesystem scan.

## Validation

Bead's full CI and Nacre's full `desktop` CI passed on the published source
trees. The Bead golden matrix covers zero, positive count, refreshing and
unavailable at 1× and 1.5×. The bar regression confirms the real widget tree
shows zero and keeps `status.pill.open updates` clickable. Nacre tests use fake
commands and a temporary database directory; no real update was run.

The private nested zero-count pair uses identical 2880×1800 output, 1.5 scale,
canyon wallpaper and the Bead palette. Both arms show the package icon and `0`.
The visual comparison reports 1.600% RMSE, 0.179% changed pixels and 7.018% bar
RMSE; both arms reported no UI errors. Positive-count idle, hover, pressed and
open states remain covered by the paired captures in [B0124](0124-pill-click-updates.md).

| QML | Bead CPU |
| --- | --- |
| ![QML updates icon at zero](evidence/0138/bar-idle-qml.jpg) | ![Bead updates icon at zero](evidence/0138/bar-idle-bead.jpg) |

Source revisions, fixture and capture safety results are in
[`validation.json`](evidence/0138/validation.json); the pixel comparison is
[`results.json`](evidence/0138/results.json).
