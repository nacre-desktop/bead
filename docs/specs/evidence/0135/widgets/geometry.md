# Widgets gallery geometry probe

The Bead-only Widgets route is reachable through the published `widgets.open` method. The old harness command `nacre menu widgets` returned no window; after changing the probe to `ui.dispatch(target=widgets, method=open)`, the screenshot showed the 12-widget, two-column gallery. The first point `(960,600)` landed in the blank right gutter and left `widgets.scroll=0`. The corrected point `(800,600)` landed inside a second-column card and proved scrolling through the state trace: `widgets.scroll` went `0 → 27 → 0` during a matched finger reversal.

This is a one-repeat geometry/route check on the e2-based 8ad8eb0d candidate, not final performance acceptance. At 120 Hz requested, it rendered 83 frames with Bead `WINDOW_FRAME.render_ms` p95 8.983 ms / p99 14.382 ms. Presented p95 was 16.05 ms, with 70 missed-slot upper-bound samples and zero discarded feedback. `refresh_ns=0` means the nested presentation feedback did not confirm 120 Hz. Renderer cost exceeds the 8 ms target.

| Evidence | Value |
|---|---|
| Harness / tests | `1ae3dac417edac2f848b023dfafc3a769e7673e379cfe72b66753de046a42f15` / `ba9bbce9cc1cb89004dfb0106a9e51858917cd871fea44d3e88cd553a74f564b` |
| Candidate binary | `f61020a409568c05eb9cd0b1e145c1b8a4be411a888de4aab71dee9bd6636d74` |
| Input / output | 160 finger events, delivered interval p95 8.01 ms; 1920×1200 logical, 2880×1800 physical at1.5 |
| Direction reversal | 23.93 ms delivered-input to presented state change; 24.11 ms input-to-presented state change |
| Screenshot endpoints | [before](screenshots/candidate-widgets-gallery-before.png), [after](screenshots/candidate-widgets-gallery-after.png) |

The endpoint images return to the initial scroll offset because the gesture reverses halfway; the joined per-window state/presentation trace proves direction change. Both screenshots show the Widgets surface in the left half of the workspace with the same bounds; the gradient on the right is the wallpaper outside that surface. The trace’s full first-frame buffer is 1389×1662 physical pixels, or926×1108 logical at scale1.5, matching `widgets.canvas.width`/`height`; later frames damage only a subrectangle. The final source’s Widgets runtime and QML parity comparison remain outstanding.
