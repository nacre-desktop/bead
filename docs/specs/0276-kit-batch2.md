# 0276 — Kit batch 2

Second batch of review components. `EmptyState` and `NoticeRow` are pure kit Luau and take the call shapes of the Nacre stand-ins (`shell/ui/components/empty-state.luau`, `notice-row.luau`), so the Shell can delete the stand-ins at its next pin bump. Sizes come from `kit.metrics` and the theme tokens.

## Components

- `kit.EmptyState { glyph, title, detail?, chip?, chip_glyph?, on_chip?, width, height, id?, left?, top? }`: a `width` x `height` column, content centred. Hero outline glyph (`icons.hero`, `outline` ink), body-size title (500), caption `detail`, at most one `kit.Chip` (`on=false`). With `left`/`top` it is absolutely positioned, as the stand-in was.
- `kit.NoticeRow { id, icon, caption, title, body, on_dismiss, width, fill?, dismiss_label? }`: 72 px row, radius `medium`, 28 px icon tile (`icon` is a host icon name; a bell glyph when empty), caption, 600-weight title, one body line, 28 px round dismiss button (`on_dismiss`; nil disables it). The stand-in took a notification record and a props table; the kit takes one flat table, so the Shell passes `caption = app .. " · " .. when`. The dismiss label defaults to "Dismiss" (the Shell passes its translated string).

## Image radius (framework)

`ui.Image { radius = r }` (the existing `radius` property) now clips the image to an antialiased rounded rectangle of `r` logical px: `Primitive::Image` gains `radius`, applied where the pixels are baked (`apply_radius_clip`, a tiny-skia mask next to `apply_polygon_clip`). The dense CPU raster and the GPU upload path both bake through it, so every renderer path gets the same pixels; the decoded-native-cover shortcut is skipped for a rounded image. `radius` is part of the raster cache key and the retained/tree hashes, so a changed radius re-bakes and an unrounded image costs nothing extra. Hosts that construct `Primitive::Image` add `radius: 0.`. Test `image_radius_clips_corners_on_dense_and_upload_paths` (`bead-render`): corner transparent, interior and straight edge unchanged, upload bytes equal dense bytes.

## Label baseline (kit)

Text is top-anchored in its box and the glyph baseline sits one ascent (about 1.03 em) below the top, so the cap height centres in a box of about 1.35 em. Kit labels beside an icon used 20 or 24 px boxes, which left them 1.5 to 2.5 px above the icon's centre (the icon is ink-centred in its own box). `label_h(size) = ceil(1.35 * size)` now sizes the label box in `Chip`, `PillButton` and `ConfirmButton`; the row centres it. No framework change, no other text moves. Measured in the headless gallery (ink centre of the cap-height span against the icon's ink centre, px): `Chip` "Wi-Fi" 1.5 above before, 0.5 after; after the change `PillButton` "Open Brain" 0.5 below, `ConfirmButton` "Clear history" (descender-bearing) 1.0 above. No golden changed.

## Not built

- A framework-level vertical centring of single-line text (would move every label in a box taller than its line).
- Image radius for the GPU draw of non-baked textures: none exist (all image paths bake).

## Evidence

`examples/kit-gallery` shows all four items (`bead check`); a headless-sway screenshot was inspected and zoomed on the chip baselines (kept in the worktree's `.lead/`).
