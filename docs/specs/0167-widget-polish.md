# B0167 — widget polish follow-up

B0161 review found frameless clock and battery text hard to read over pale wallpaper, light glass cards, copied design notes, and a slow analog clock. This change strengthens text and glass, uses daemon battery values, and removes copied notes. The 60/120 Hz pacing target did not pass.

| Check | Evidence | Result |
| --- | --- | --- |
| Contrast and glass | `evidence/0167/` | Captures refreshed on forest and dark wallpapers; paired reference HTML is a visual proxy, not QML parity |
| Battery | `shell/widgets/battery/widget.luau`, `crates/nacre-shell/src/reload/bar_daemon.rs` | Real reported percent, charge status and estimate; unknown values stay unknown |
| Analog cadence | Desktop `captures/pacing/{60,120}/pacing-summary-*.json` | FAIL: 2 commits per five repeats; presentation p99 gap 1256/1597 ms at 60 Hz and 1583/1244 ms at 120 Hz; dropped presentation rates 97.9–99.0% |
| Copied notes | `shell/widgets/**/*.luau` | Removed the battery fill annotation and power confirmation design note |
| Full CI | Remote Ubuntu 24.04 full recipe | See worker report for exact final result |

The common glass tint is rgba(10,22,20,158). Frameless Outfit text uses #eef6f3 with a 24 px soft shadow at 0.60 alpha. Battery status and remaining time come from the daemon projection. The face remains retained, but active hands are not reliably rescheduled at refresh rate; this remains an acceptance gap. The renderer has no radial-gradient surface and the widget host receives no wallpaper raster sample, so adaptive luminance scrimming also remains open. QML has no matching wallpaper widget/editor surface; the paired HTML references do not establish QML parity.

No temporary duplicate code or new build dependencies.
