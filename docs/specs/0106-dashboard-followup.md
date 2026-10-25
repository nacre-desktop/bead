# B0106 — Dashboard follow-up

Status: in progress; no timing, memory or parity acceptance yet.
Baseline: B0102 integrated at `b41d636`. Its pre-redesign memory control is
`44a2004` with the identical companion host-routing fix from `18dc581`.

Today's wallpaper decode and PNG encoding previously blocked the panel worker's
model projection. A single background job now prepares the bounded poster;
the first projection returns the layout immediately. The existing descriptor
identity checks remain in the decoder. Results for superseded wallpaper/size
pairs are rejected. Closing hides the poster; retirement joins the job and
releases its result. QML's Today image also uses asynchronous loading.

Shell Luau owns the visual changes: Media separates player/artist/album,
Weather restores units, metrics and compact weekday/range rows, and System
uses a shared four-gauge band above network/process cards. Storage reads the
published filesystem data. No frame pacing, launcher, daemon or backend change.
No new framework primitive or temporary duplicate is introduced.

## Validation pending

Private desktop captures use the blocking shared lock per command, swap
preflight, containment/watchdog and full 2880×1800 output at scale 1.5. The same
wallpaper/palette record and playing-media/system/weather fixtures apply to
both shells. CPU debug builds retain the B0102 measurement method; a matched
release pair separately checks B0074's first-reveal target. Heaptrack runs are
diagnostic and excluded from timing and memory acceptance. Clean-file PSS is
informational; the gate is anonymous + SHM + DRM + SwapPss.

Remaining proofs: stage breakdown and ±20 ms reveal target, per-page closed
memory attribution/recovery, before/after RGB RMSE, full remote CI, and visual
inspection of regenerated goldens. No live session or laptop-cap claim.
