# B0157 — Wallpaper picker errors

4K activation could reject an optional CPU composite after the live view had already consumed its retained-byte budget.
Spotlight slide work scanned the complete catalogue for each animation update.
The fix keeps the prepared CPU frame on admission pressure and limits slide traversal to visible neighbors.
Private 2880×1800, scale 1.5 QML/Bead captures used the owner's wallpaper library in isolated HOMEs; no owner imagery is committed.

| Check | Result |
|---|---|
| CPU snapshot budget fallback | Regression test fills the 40 MiB view budget; acceptance keeps the prepared frame and does not return the cache-budget error |
| QML + Bead CPU/GL, 60/120 Hz | Open, hover, pressed, slide×20, apply×10 and close: zero UI errors; cache peak 23,040,372 bytes (22.0 MiB) |
| Bead CPU 60 Hz render time | p50 28.25 ms, p99 50.03 ms, max 61.95 ms; fails the 33.3 ms two-interval ceiling |
| Bead CPU 120 Hz render time | p50 28.33 ms, p99 54.46 ms, max 67.57 ms |
| Bead GL 60 / 120 Hz render time | p50/p99/max 0.33/6.72/9.30 ms; 0.29/6.43/16.40 ms |
| Full Bead CI | Passed on the task commit in the repository CI container (`FULL_CI_OK`) |

The GL cadence and picker cache budget pass. CPU repaint cadence remains the open gate; B0147's reported cold/expired 60 Hz p99 was 53–57 ms, so these CPU p99 values improved, but individual frames still exceed two intervals. Paired screenshots were inspected locally and removed with the private capture set. Known visual gaps remain: QML shows a full-output selected-wallpaper backdrop, and its hero card is larger than Bead's.

Temporary duplicates: none. No new kit primitive or dependency.
