# B0193: Widget scrim and pause

Status: candidate. Numbers are from private nested sessions (desktop test host, CPU renderer, 2880x1800 scale 1.5, release build); the lead's live audit is the acceptance.

## Adaptive contrast
- `crates/nacre-shell/src/wallpaper_luma.rs`: when a wallpaper is decoded for an output (the path that already scales it, once per change, cache hits included), a 16x9 grid of mean WCAG relative luminance is measured by sampling every 4th pixel (about 1 ms for 5 Mpx), together with the luminance of the palette roles `onSurface` and `surface`. It is published when the wallpaper is committed, and removed with the output.
- Store `bead.desktop.wallpaper.luma` (a one-record list `{cols,rows,on_surface,surface,cells}`; store lists hold objects), delivered per output to the bar session (poke model event, set again on `OutputName`) and projected to the canvas and drag ghost like the other `bead.desktop.*` keys. Declared in the bar, editor (canvas), app and drag behaviors.
- `shell/widgets/desktop-visuals.luau`: `tone(instance)` averages the cells under the widget slot (the canvas entry supplies the layout size), picks ink `onSurface` or `surface`, whichever contrasts more, and adds a translucent rounded scrim (opposite role, alpha .55) only when neither reaches 4.5:1 and the scrim helps. Shadow follows the ink. Roles only, no literals. Without luminance (previews, no daemon) the tone is the old look. Used by the digital and minimal clock, the frameless battery and the Wi-Fi card. No backdrop blur exists in the engine, so the scrim is plain translucent, not blurred.
- Nested proof: the harness paints the nested background with the same flat colour as the fixture wallpaper (the harness does not paint the wallpaper surface itself); the decode and luminance path is the real one. See `evidence/0193/`: `bright-wallpaper-after.png` (dark ink; compare `../0189/bright-wallpaper-before-scrim.png`), `dark-wallpaper-after.png`.

## pause_on (fullscreen, lock)
- `wallpaper_motion::output_hidden` reuses the motion-wallpaper derivation: a fullscreen window on the output's active workspace, or a locked or sleeping session. Editing and dragging never pause.
- The bar host (`reload.rs`, `sync_widgets_covered`) tells the canvas with `bead.desktop.covered`; while covered it sends no projections. The canvas ignores service and backend events while covered, drops its seconds lease (no 1 Hz ticks, no timers), and rebuilds once at each change. On uncover the host sends a full current projection, so state is correct (the minute clock reads the right time, see `resumed-after-fullscreen.png`).
- Granularity: the whole canvas of the output pauses; every in-tree widget declares fullscreen and lock, so per-widget `pause_on` opt-out is not read (the list is still only validated). `battery_saver` is not enforced.

## The 1.2 rebuilds per second
Cause: the Luau canvas entry has no view-read filter, so every changed store rebuilds it. `bead.desktop.clock.seconds` ticks once a second whenever the bar leases seconds (a bar clock in the seconds style, as in the harness layout) and reached the canvas by two routes: the host projections and the canvas session's own services subscription. Fix: the host projections drop the key unless a canvas clock uses a seconds style, and the canvas session drops it unless it holds the seconds lease. Verbose log `CANVAS_PROJECTION path= changed=` names the keys behind each projection.

## Idle numbers
`spikes/desktop-widgets-idle.py` (digital + analog + battery ring; bar clock in seconds style; 8 s warm-up, 40 s window; `widgets-covered` fullscreens an alacritty window). CPU per mille of one core for the shell, context switches per second.

| Run | CPU ‰ | ctx sw/s | canvas redraws/s |
|---|---|---|---|
| Before (a425b615, same harness, bright) | 36.3 | 24.3 | 1.33 |
| After, visible, bright | 17.3 | 19.3 | 0.375 (the one minute ease, 13 frames) |
| After, visible, dark | 16.8 | 19.3 | 0.35 |
| After, covered by a fullscreen window | 7.5 | 20.7 | 0 |

The no-widgets floor in B0189 was 16.5 ‰, so the visible cost is now at the floor.

## Tests
`desktop_clock_ink_follows_wallpaper_luminance` (real shell tree: theme ink without luminance, light ink on dark, dark ink on bright, scrim on mid, and back on a wallpaper change), `widgets_pause_on_fullscreen_or_lock_per_output` (per output, tiled/inactive not covered, lock, sleep), `wallpaper_luma::tests`. The reload-level cover/uncover wiring is proven by the nested run above, not by a unit test.

## Gaps
- The analog dial, battery ring and Wi-Fi icon glyph keep their own backing; sound/updates text widgets are not adapted.
- Luminance is per output at commit; the palette event and the luminance poke are separate, so a one-frame mismatch is possible at a wallpaper change.
- The canvas-needs-seconds check in `reload.rs` names the clock styles `seconds`/`analog-seconds` (Nacre edge knowledge).

No temporary duplicates were added.
