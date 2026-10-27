# B0167 — widget polish follow-up

B0161 review found the frameless clock and battery text hard to read over pale wallpaper, glass cards too light, and the analog clock under-scheduled. The battery card also contained design-note copy and a fabricated time estimate. This slice strengthens text and glass, reads battery status and estimate from the daemon contract, and schedules active analog layouts at the display refresh interval.

| Check | Evidence | Result |
| --- | --- | --- |
| Contrast and glass | `evidence/0167/` | Pending forest and dark-wallpaper captures |
| Battery | `shell/widgets/battery/widget.luau`, `crates/nacre-shell/src/reload/bar_daemon.rs` | Uses reported percent, charge state, and time estimate; unknown values remain unknown |
| Analog cadence | `crates/nacre-shell/src/reload.rs`, `crates/bead-wayland/src/canvas.rs` | Active analog canvas layouts use the generic `SceneSource.wakeup_at()` frame deadline; face image remains retained |
| Copied notes | all `shell/widgets/**/*.luau` | Removed the battery fill annotation and power confirmation design note |
| Full CI | remote Ubuntu 24.04 recipe | Pending |

The common glass tint is rgba(10,22,20,158). Frameless Outfit text uses #eef6f3 with a 24 px soft shadow at 0.60 alpha. The analog wakeup follows the output's reported presentation interval; it does not claim physical scanout timing. The existing UI has no radial-gradient surface and the widget host does not receive a wallpaper raster sample, so an adaptive luminance scrim remains a known gap pending a general renderer primitive and a wallpaper sample API. QML has no equivalent wallpaper widget/editor surface, so the required direct QML comparison is unavailable.

No temporary duplicate code or new build dependencies.
