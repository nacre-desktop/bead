# B0209 - Detach ghost: cover blur

Superseded by B0210 (2026-10-30): the ghost was niri's session-wide window blur behind the
invisible prewarm; the fix is `background-effect { blur false; }` in the pearl rule, and the
cover-blur release below was reverted.

Live evidence (laptop, 2026-10-29, 23:10): after `PEARL_DETACHED` the dragged window is
already visible while an opaque, stretched, blurred wallpaper rectangle sits at the docked
spot over the terminals for ~1.4 s. niri's layer list shows the panel's `bead-dev` Overlay
layer mapped from +0.00 s to +1.41 s, then unmapped; `bead-panel-dismiss` was gone at +0.00
(B0200). No close slide runs (journal has no `PANEL_SLIDE dir=close` for the panel).

## Cause

The cover layer (kept mapped until the native pearl's first content frame, B0131/B0200) still
owns its `ext_background_effect` blur region over the whole panel surface. The handoff moved
the effect object into `Handoff` and kept it alive. A blur region shows the compositor's
blurred background even when the layer's own pixels are not the content (xray blur = the
wallpaper), so the cover painted a blurred wallpaper over the docked spot until retire.

## Fix

`release_cover_effect` destroys the layer's blur effect and commits the layer at the start of
both handoff paths (`promote_native_pearl`, `activate_prepared_native`). The cover buffer is
unchanged, so no blank frame; the native window creates its own effect on its first draw.
Failure restore recreates the effect on the next draw. Unit test: `cover_effect_tests`.

## Gaps

GL nested panels do not render in the harness, so the ghost was not reproduced in frames; the
cause is derived from the live layer list and frames. The lead's live audit is the acceptance.
Why the cover retires ~1.4 s after detach (first native frame timing) is unchanged and not
investigated; with the blur gone the cover shows only its own last frame.
