# B0102 — Dashboard owner fixes

The B0096 status-pill runtime sent bar-only state into companion surfaces on
pointer entry/exit. The dashboard rejected `status.hovered`, keeping its old UI.
Pill publishing now belongs only to bar sessions. A runtime regression loads all
13 dashboard pages and exercises pointer, timer, selection, dismissal and reload.

Today follows QML's wallpaper hero and three-column calendar, host/metrics and
media summary. Its Luau modules own geometry, typography and transport controls.
Published `dashboard.system` and `wallpaper.active` sources are leased only by
the open Today page; the existing descriptor-pinned wallpaper decoder produces
display-sized pixels in its short-lived worker. Closing releases sources and art.
No new framework primitives or temporary duplicate paths.

Validation and paired QML/Bead evidence: pending remote checks.
