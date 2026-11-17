# 0280 — Kit batch 3: interaction poses and surface shadows

The design review's parked framework asks (0274 "Not built"): hover-only children, shadows on any surface, a host shadow prewarm, animated list reflow and a per-node press with scale. Reference: [luau-ui.md, Interaction poses](../reference/luau-ui.md#interaction-poses).

## Framework

- One paint-stage pass (`materialized::Poses`, bead-ui) poses the laid-out or patched scene on every frame from `bead.hovered`, `bead.focused`, `bead.focus_visible` and `bead.pressed`: no Luau run, no layout. Layout records the views that ask (`visible_on_hover`, `press_scale`/`pressed_fill`, `reflow`); a view with none of them adds nothing. Motions use `bead_core::animation::Animations` (retargets from the drawn value, reduced motion jumps), keep their state across rebuilds, and pace frames through the generation's animation deadline.
- `visible_on_hover="<ancestor id>"`: the subtree fades (`fade` .16 s) with the group's pointer or keyboard focus. The group's plain background is a hover target (`bead.hovered` = its id) when no control is under the pointer; hover colours apply to focusable views only, so a group id paints nothing. Hidden children stay nodes (pointer, keyboard, accesskit).
- `press_scale` / `pressed_fill`: the press origin is the node hovered at the press; a view holding it is pressed while the pointer stays inside it, until the release. Scale about the view's centre, box fill mixed into `pressed_fill`, `press` .06 s down, `release` .12 s up. Node boxes keep the laid-out geometry.
- `reflow=<seconds>`: a container's direct children slide from their drawn position relative to the container (so scrolling is no reflow); new children appear in place.
- `shadow=true` on `Panel`, `Box`, `Row`, `Column`, `Grid`, `Button` (was Panel and Box): the same `Primitive::Shadow` (margin and blur now `bead_render::SURFACE_SHADOW`), nine-patch raster. A hover box a transparent shadowed button inserts goes behind its label and in front of its shadow (was in front of the shadow's index).
- `bead_render::prewarm_shadows(&[(Rect, radius)], scale)` pins those shadows; the hard-coded Nacre OSD shape (`prewarm_osd_shadows`, the `osd_shape` branch of `shadow_pixmap`) is deleted.

## Kit

`kit.Button` defaults to `press_scale=.97`; `Chip`, `Segment` options, `PillButton` and tile buttons get `pressed_fill` (the hover tint at .12) instead of reading `bead.pressed` in Luau (each press re-ran the view); tiles scale as a whole (inner buttons `press_scale=1`). `NoticeRow`'s dismiss is `visible_on_hover` its row. `Card{shadow=true}` passes through. `examples/kit-gallery` shows all of them plus a reflowing toast stack.

## Shell changes (next pin bump)

- Replace `prewarm_osd_shadows()` with `bead::render::prewarm_shadows(&[(Rect::new(0., 0., 360., 64.), 32.)], 1.5)` (the review's OSD pill; one call with every shape) and drop the 396 px stand-in shape.
- Toast cards: `shadow=true` instead of the outline stand-in; expand/close icons `visible_on_hover="<card id>"`; the stack `reflow=ui.motion.duration.switch`; chips get the press for free.
- Remove any view-side `ui.bind("bead.pressed")` reads used only for press fills (use `pressed_fill`).

## Limits

- Untinted images, shadows and graphs inside a hover-only child do not fade (no primitive opacity).
- Clips inside a sliding reflow child stay at its new place; press scale keeps clips (the content only shrinks).
- Only pinned shadows are cached as whole pixmaps. Any other radius/blur/scale (a status peek's shadow while it moves) is still
  composited per frame from the per-axis template cache (0270); a general whole-shadow cache is not part of this unit.
- An immediate-action button (action on press) does not track leave while held (`drag` updates the hover only for deferred gestures), so it stays pressed until the release.

## Evidence

Press: the kit no longer reads `bead.pressed` in Luau, so a press and its release evaluate no view (B0282 measured 78 view
evaluations for 38 gallery clicks before, about 2 ms each); the press test asserts 0 evaluations over press and release.
Tests (`bead-ui` materialized_tests): `hover_only_child_fades_in_with_its_group_and_keyboard_focus_without_luau_or_layout`, `pressed_view_scales_and_deepens_its_fill_until_release_without_luau`, `reflow_children_slide_from_their_drawn_position_without_luau_or_layout_per_frame`, `shadow_on_any_surface_and_a_hover_box_inserted_behind_it_paints_like_a_rebuild`; `prewarmed_shadows_are_pinned_and_replaced_per_call` (`bead-render`). `bead check examples/kit-gallery`. No golden changed.

Headless sway capture of the gallery (dev-1, 1000x900, scale 1, pixman; screenshots kept in the worktree's `.lead/`): at rest no
toast shows its dismiss button; hovering the second toast's background shows only its dismiss glyph; moving off hides it again.
"Add toast" chip ink box 127 x 36 px hovered, 123 x 36 pressed (.97), deeper tint, back to 127 after the release; the card shows
the panel shadow halo. The press inserts a toast at the top (the chip acts on press): 0.12 s after it the new toast is drawn in
place (y 188) while the others are on their way from their old rows (y 268, 348) to their new ones (348, 428), at 337 and 417;
after the slide the rows sit at 188, 268, 348, 428.

## Shadow parameters (B0320)

`shadow` is `true`, `false` or `{blur, spread, dy, alpha}` (defaults 10, 0, 0, 1). The old 30 px margin was exactly `3 * blur` (the Gaussian's 3 sigma at blur 10) with spread 0, so `true` emits the same primitive and pixels as before.
`Primitive::Shadow` gains `spread`, `dy` and `alpha` (serde defaults 0, 0, 1). Its rect is the box grown by `3 * blur + spread`, then moved down by `dy`; its radius is the box radius plus `spread`; alpha scales the `.32` opacity and is part of every raster and cache key.
Reveal, retained edge bands and clip margins read the margin and offset from the primitive, so a reveal panel with a table shadow follows its real extent.

## Key repeat schedule (B0322)

- `Panel{key_repeat={delay, interval, min_interval, ramp_ms}}` (root only) lands in `Scene::key_repeat`. While a surface of that panel has keyboard focus the app ignores SCTK's repeats and compositor-sent ones (`KeyboardHandler::repeat_key` drops them while a run is active) and runs one calloop timer for the last pressed key that the keymap repeats (the keymap's repeat flag is read from `Keymap::as_string` at `update_keymap`).
- The first repeat comes at press + `delay`; each next gap is `interval` eased to `min_interval` by time since the press, reaching it at press + `ramp_ms` (the first gap is exactly `interval`; a `ramp_ms` not past `delay` gives `min_interval` at once) (`RepeatPlan::gap_ms`, pure). Each repeat goes through the same `deliver_key` path as a press. Another repeatable key's press restarts it; release of the key, `leave` and a dead or panel-less surface stop it.
- Without the prop nothing changes: SCTK's timer and `repeat_key` are untouched. The bar, other panels and windows keep the system repeat.

## Font features (B0326)

`font_features = "tnum ss01, liga=0"` on any element that takes `font_weight`: space- or comma-separated OpenType tags of 4 printable ASCII characters, each with an optional `=0..65535` (a bare tag means 1), at most 16; anything else is a markup error, absent is none and keeps the output unchanged. `Primitive::Text` gains `features` (serde default empty) and `Primitive::text(rect, text, family, size, color)` builds a run with every other field at its default.
Features reach `Attrs::font_features` on paint and in `bead_render::text_width_styled` / `text_lines_styled`, which take `(family, weight, features)` (`text_width` / `text_lines` keep their signatures at weight 400, no features); the raster key, the tree hash and the retained layer hash include them. `ui.text_width(text, family, size[, weight[, features]])` measures at the node's weight and features (3-argument calls stay the 400-weight default). A numeric `font_weight` is a continuous `wght` on variable fonts, so 450 is a real weight.

## Bake size (B0328)

`bake_size = {w=96, h=96}` on `ui.Image` (logical px, 1..4096 each, else a markup error; absent changes nothing) becomes `Primitive::Image::bake_size` (serde default none; `Primitive::image` leaves it none). On the upload (GL) path the payload key, its width/height and the bake use `bake_size` x scale instead of the rect size, with fit, `clip_polygon`, `radius` and `tint` baked in that rect, so a rect that only moves or resizes reuses one payload and texture; the GL draw already maps the whole raster onto the rect (bilinear), so `radius` and `clip_polygon` scale with it (a circle stays a circle when the aspect matches). Opacity stays draw-time. Budgets charge the baked size.
`native_cover` is decided against the bake size and still covers the rect at draw time. A bake_size payload skips the `compact` atlas layout, whose coordinates assume payload px = rect px. The CPU (Dense) path ignores `bake_size` and rasters exactly per rect (`shortcut: no scaled CPU draw, add one if a CPU surface animates image sizes`). Damage uses the rect.
