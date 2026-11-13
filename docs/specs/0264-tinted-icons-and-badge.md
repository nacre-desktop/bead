# 0264 — Tinted icons and badge

Two design-review asks from the Nacre bar: status icons drawn in a palette colour, and counts as small badges. Both are general framework pieces; the host's palette supplies the roles.

## `ui.Image { tint = ... }`

- `tint` takes any colour expression a `color` takes: a palette role, `ui.alpha(role, a)` or `ui.tint(base, over, a)`. It resolves at layout into `Primitive::Image { tint: Option<[u8; 4]> }`.
- The renderer applies the tint once, where every renderer decodes images (`decode_image` in `bead-render`): each premultiplied pixel becomes the tint colour at the pixel's own coverage (`alpha * tint_alpha`). The CPU path, the GL path and the GPU path all consume that raster (`load_image` and `ImagePayload::prepare`), so there is one implementation and no per-frame work.
- Coverage is the source alpha, not luminance: symbolic glyphs are usually dark ink on a transparent ground and a luminance mask would erase them. A full-colour app icon becomes its silhouette in the tint, which is what the review asks for.
- The raster cache key, the retained-raster fingerprint and the tree hash include the tint, so a tint change re-rasterises and an unchanged tint is a cache hit on the next frame (`tinted_image_is_the_alpha_mask_filled_with_the_tint_and_cached`).
- `prewarm_image` keeps its public signature and warms untinted rasters; tinted icons are small and go through the ordinary cache.
- The tree renderer's native cover decode (a `Cover` raster smaller than its target) bypasses `load_image`, so a tinted image never takes it (`tinted_cover_image_skips_the_native_decode`).
- Memory: no new cache. Tinted rasters are entries in the existing bounded small-image cache (<= 128 px), one per (icon, size, tint).

## Icon slots in folder apps

`@icon:NAME` paths are host slots. `bead check` no longer tries to read them from the bundle (the materialized path did, failing with the view file's name). The folder host (`FolderView`, so `bead check` and `bead run`) now fills a slot with `NAME.svg` (preferred) or `NAME.png` found under `icons/hicolor` or `pixmaps` of `XDG_DATA_HOME` and `XDG_DATA_DIRS`, memoised per name; there is no `index.theme` inheritance or size matching yet. A renderer that reaches an unfilled slot reports `icon "NAME": not supplied by the host or found in an icon theme` instead of a bare "No such file".

Fixed later by B0268 (`85086edf`): under `bead run`, a `[layer]` folder app's image failed with `UI_ERROR CPU worker preparation pending` because the CLI passed a plain `FolderView` instead of wrapping the bar's view in `bead_wayland::worker::Source`, as windows, panels and Nacre's bars do.

## `kit.Badge`

`kit.Badge { id, text, tone }` is a `ui.Text` with a fill: 15 px high, radius 7.5, width `max(15, ceil(text_width) + 8)` (padding 0 4), text 10 px weight 600, `position = "absolute"`, `top = -5`, `right = -7` relative to its parent (an icon box with a size). `tone` is the fill role; the ink is the matching on-colour for `primary`, `secondary`, `tertiary` and `error`, otherwise `surface`, which contrasts with any host attention colour (`warn`, `good`) by construction. `top`, `right`, `fill` and `color` override. The badge is hidden (`visible = false`) when `text` is empty or `"0"`; a count change animation is the motion unit's concern.

## Test font

`bead-test`'s `FixtureSymbols.ttf` subset gains the characters `0` and `q`, so the status glyphs `battery_android_0` and `battery_android_question` (and `question_mark`) resolve to one ligature glyph in goldens (`fixture_symbols_resolve_status_glyph_names`). Production fonts are unchanged.

## Evidence

Pixel and cache tests in `bead-render` (`lib.rs`), layout and tint-resolution test in `bead-ui` (`markup_tests.rs`), folder-app `bead check` test in `bead-cli`, plus a headless-sway screenshot of a bar with a tinted icon and a badge inspected during the unit (kept in the worktree's `.lead/`, not committed).

## Host changes

Hosts that construct `Primitive::Image` literals add `tint: None`. The Nacre bar replaces its stand-in badge with `kit.Badge` and passes `tint = "secondary"` on the title icon.
