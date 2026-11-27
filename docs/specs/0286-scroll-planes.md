# 0286 Scroll planes (B0419)

Composited scrolling: every `ui.Scroll` becomes a plane whose content main can move as one, with
text rastered ahead of the viewport while it moves. Status: U1 (planes in the scene) landed; later
units (U2a GL shifts, U2b main stepping, U3 CPU fallback, U4 kits, U5a axis x/snap/motion/on_settle,
U5b `ui.scrolled`) append their contracts here.

## U1 contract: planes in the scene

- `bead_render::SceneLayer.scroll: Option<ScrollPlane>` is set on each `ui.Scroll`'s layer; the
  plane's content is the layer's `own.end..subtree_end`. Other layers carry `None`.
- `ScrollPlane` (layout px):
  - `offset`: the clamped scroll offset; `limit`: the largest offset.
  - `clip`: the viewport the content is clipped to (the outer clip ∩ the Scroll box).
  - `interest`: where the content rasters. At rest it is `clip`, bit for bit. While the plane
    moves it reaches `overscan` viewports above and below the clip, clamped to the content.
  - `ready`: the content span inside `interest`, in content px (0 is the content's top, at
    `bounds.y - offset` in the scene).
- `ui.Scroll{overscan=n}`: 0..3 viewports, default 1. On any other node it is a markup error.
  With `overscan=0` a moving plane keeps its interest at the clip (today's path).
- `bead.scrolling`: a framework store like `bead.hovered` (default `""`). It names the moving
  plane's view id, `""` at rest. Nothing in U1 sets it; U2b (main stepping) does.
- Interest rule: a plane is moving when `interest != clip`. Text and image rasters of a moving
  plane's content (innermost moving plane) are kept or dropped by `interest` instead of their own
  clip (`tree.rs` `raster_hidden_by_own_clip`, `prepared_rasters_match_visibility`). Layout needs no
  culling change: it already keeps every Scroll descendant in the scene (`retain_offscreen`).
- `ready` is what was rastered. The bake rasters every text in `interest` in the same pass
  (Dense images too). Images on the GL upload path keep today's async decode on the resource
  worker. The `ready={from,to}` Luau prop is deferred to U4, where the kits that materialize
  windows declare it.
- Patch path: a scroll-store or `bead.scrolling` change rewrites `offset`, `interest` and `ready`
  (`layout::plane`) with no Luau or layout. `Topology::place` moves a plane's `clip` (and a resting
  plane's `interest`) with its layer, so a plane under an animated ancestor stays placed.
- The scrollbar-thumb fast path (B8 fill tail) reuses a frame only with the same raster set, so a
  settle with no offset change drops the overscan rasters.

Memory: at rest, unchanged. While a plane moves, about (1 + 2 × overscan) × its visible rasters.
`shortcut:` there is no per-plane budget yet; the 68 MiB frame cap (`STEADY_RASTER_LIMIT`) and the
64 MiB GL texture cap bound it. Add a nearest-first budget if the rig shows > 8 MiB per plane while
moving.

Known limits (U1):
- Nested planes: an inner resting plane's content follows the outer moving plane's interest, so
  it can raster rows hidden by the inner viewport (over-admission, never a missing row).
- Clip boxes inside a moving plane raster by the plane's interest, also over-admission.
- Under a `scale` track the offset stays in layout px, so a moving plane's interest and ready are
  approximate there.
- Vertical only; the x axis is U5a.

Tests: bead-ui `materialized_tests` `scroll_plane_*` (geometry at rest and moving, overscan
validation, the text 150 px below the clip rasters only while `bead.scrolling` names the plane,
a scroll patch with no Luau equals a full layout, at rest equals today's scene and rasters, a
patch under a translating ancestor); bead-render `tree.rs`
`scroll_plane_settle_drops_overscan_rasters_on_the_thumb_path`.
