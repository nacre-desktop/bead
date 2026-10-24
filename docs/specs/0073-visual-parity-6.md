# B0073 — Visual parity, round 6

Status: **Review, with remaining gaps**. Continues B0071. No launcher adapter,
launcher Luau, launcher goldens or Nacre files changed.

## Changes

- The optional updates item reads published `dashboard.updates.count`, rejecting
  unavailable/stale/disconnected data. The Luau `bar.updates.enabled` binding is
  driven by `desktop.barUpdatesEnabled` (default true); false skips its initial
  lease and removes its spacing. Lease configuration applies at bar construction;
  enabling an initially disabled item requires a bar reload. Zero/unknown counts
  hide as requested, while QML currently displays its standalone count/message.
- Kit switches use QML's 38×22 track, 16 px thumb, palette roles and disabled
  alpha. Action buttons use full pill radii. The renderer now multiplies glyph
  coverage by text alpha (cosmic-text's legacy raster callback drops base alpha).
- Wi-Fi has the disabled refresh pill, matching empty-group words, medium section
  labels and space for wrapped adapter/service notes. Bluetooth matches QML's
  connected-then-name order, per-device gaps, names and compact action pills.
- Audio volume/mute/device selection and battery profile choices reuse the control
  centre's capability checks, guarded calls and authoritative readback. Only the
  popout's topics are leased; extra fields stay scoped to Sound/Battery. UI stays
  in Luau; no ranking, provider or second daemon action engine was added.
- General primitive: `font_weight` (1–1000, default 400), included in font selection
  and raster/layer keys. OSD uses medium text, an unclipped icon line box, corrected
  vertical offsets and a palette-templated horizontal progress gradient.

## Evidence and measurements

Control `91a6227`; QML `f032c7b4`. Identical private nested niri, 2880×1800,
scale 1.5, CPU release builds without native GL, shared presentation and fixtures.
Accepted arms held the capture lock and passed containment/geometry, host swap-in and
process SwapPss gates. Swap stayed below 64 MiB; no reset or host change occurred.
The first control capture omitted its wallpaper decoder and was discarded;
the corrected control reproduces B0071's Wi-Fi/audio RMSE (10.248/7.200%).

| Whole-image RMSE % | Before | After |
| --- | ---: | ---: |
| Wi-Fi | 10.248 | 6.195 |
| Bluetooth | 5.531 | 5.236 |
| Battery | 4.846 | 4.854 |
| Audio | 7.200 | 5.482 |
| Brightness OSD | 4.226 | 4.080 |
| Keyboard OSD | 4.212 | 4.136 |
| Idle / bar | 3.867 | 3.870 |

[Numeric evidence](evidence/0073/metrics.json) also gives fixed-crop errors,
dispatch acknowledgements and memory samples. Paired JPEGs are QML left / Bead
right; the complete gallery and sequences remain private at `parity-gallery/b0073/`.
Clock changes and the missing upstream updates segment affect the bar error.

Three alternating bar-only pairs: median UI PSS+DRM+SwapPss delta
**+0.027 MiB** (file-backed -0.023, anonymous
+0.051, framebuffer/DRM/SwapPss +0). Closed idle delta
+0.027 MiB. This passes the remote +0.5 MiB relative gate. It measures
an unavailable-count lease, not a future live provider. No new timer/provider/cache;
closed panels retain the existing lazy release policy. This is not a laptop-cap
or populated-panel settled-memory acceptance claim.

Warm dashboard 10/50/90% image progress: QML 150/219/270 ms,
control 251/309/355 ms, candidate 251/310/355 ms. Bead acknowledges
both dispatches faster than QML, so the remaining delay follows admission.
A new panel worker is created on reopen; its logged first preparation is about
29 ms in the control, which does not explain the entire remaining offset.
The delay was not removed. These are nested capture timings, not physical scanout.

## Validation and remaining gaps

Full Arch-container CI-equivalent passed (Rust 1.99.1): fmt, clippy `-D warnings`, workspace and
trace memory tests, renderer feature configurations, facade/headless and generated
Luau schema/analyzer, spike/packaging suites, wallpaper smoke, private services/tray,
and root plus independent framework cargo-deny graphs. Goldens regenerated and
inspected. New regressions cover stale/disconnected counts, setting removal,
addressed audio selection, text-weight cache invalidation and text alpha.
One arm failed on private GVFS cleanup; setting its existing flags before D-Bus
activation and repeating the arm passed. No safety gate was relaxed.
Arch additions: DejaVu/IBM Plex fonts, swaybg and wlr-randr; Sway's file capability
was removed inside the disposable container so Docker can execute it. No host
packages, live UI, installed Nacre, settings, units or keybindings were changed.

Ask: publish/feed the daemon's `dashboard.updates` count; the private daemon leaves
it unavailable while QML's standalone worker shows 129 (N0019 still describes
standalone delivery). The bar consumer is ready; no provider was duplicated.
Other gaps: dashboard first-reveal offset; Qt/cosmic-text residual glyph/border
metrics and some service error wording; muted OSD fill is still flat; battery
profile/estimate availability differs from QML's defaults in this private session.
Wi-Fi Scan remains disabled. Audio volume uses the kit’s 5% steps.
Audio streams remain status rows; stream volume/mute and input peak are not ported.
Real-device activation was not attempted; fake-daemon tests verify addressed calls.
Temporary duplicates: none introduced.
