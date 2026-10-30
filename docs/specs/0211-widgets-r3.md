# B0211 Widgets round 3: widgets stay where dropped, quiet drags, cheap desktop canvas

Status: candidate on `task/claude-b0211-widgets-r3-20261030`; lead live gate pending.
Supersedes B0210's bar model (side-section gaps, centre row, insertion slot) and B0208's
re-centring centre section.

## Owner feedback on the live B0210 build (laptop, 120 Hz)
1. "Still a little laggy": `CANVAS_FRAME view=widgets-canvas w=320 h=300 draw_ms=7.7-11.9`,
   `MAIN_BUSY ui:Canvas` 8-12 ms per pass, in a 240 ms burst of ~15 frames.
2. "A weird blue border across when I drag".
3. A widget dropped in the middle pushed the title left; drops jumped left/right instead of
   landing where released.

## 1. Desktop canvas cost
Causes (release, dev-2; the laptop is ~3-4x slower, matching the live numbers):
- The 320x300 canvas was the battery big ring (that style's default size). A battery change
  eased the sweep for 240 ms (`ui.animate`, .24 s): each frame rebuilt the canvas in Luau and
  repainted the whole ring (the value arc's paint rect is the full circle): two 16 px arcs =
  2.0 of 2.4 ms per frame on dev-2, 7.7-11.9 ms live. The ring now sets its sweep: one repaint
  per change (`desktop-visuals.luau` `M.ring`).
- A digital/minimal clock re-rasters its glowing face on each minute tick: the text shadow was a
  direct 49-tap Gaussian, 13 ms for the 148 px face at 1.5x. Now three running-sum box passes
  per axis (`bead-render` `blur_text_shadow`), cost independent of the radius: 13 ms -> 1.7 ms.
- A steady second hand/static widgets were already cached (unchanged text and images keep their
  rasters; a sibling change does not re-raster them; measured).
- Drags: since B0210 the desktop canvas gets no pointer; since B0211 the bar does not rebuild per
  motion either (below). Nothing but the ghost surface position changes during a drag.

## 2. Blue border
Accent outlines drawn during a drag, all removed: B0210's bar insertion slot (a primary-outlined
box that slid across the bar with the pointer), the bar's 2 px primary outline while a drag
targets it, the desktop drop-zone outline (a primary rectangle around the whole desktop), and
the bar ghost's primary border and marker. The ghost keeps a hairline `outlineVariant` card; the
only zone left is the red Remove target for placed widgets. The drag surfaces are layer-shell
canvases (`widgets-drag` Overlay, `widgets-zones` Top), so niri's border, focus ring and shadow
(window rules) never apply to them; the Widgets app window keeps its normal focus ring.

## 3. Bar placement
- A top-level bar widget may carry `slot.offset` (logical px): `slot.region` is its anchor;
  left/right = px from that output edge to the widget's near edge, centre = px from the output
  centre to the widget's centre. A drop outside a pill picks the anchor by the third its centre
  falls in, so it stays put on resizes and other outputs. Normalisation keeps the offset (both
  VMs); validation allows it on top-level bar items only.
- Widgets without an offset keep the packed left/right rows: saved layouts load unchanged (the
  owner fixture renders the same rects and the same pixels as origin/main 6802f81e). Older
  centre neighbours of the title (B0208/B0210 drops) sit beside its widest band instead of in a
  re-centred row.
- The title stays at the true centre: placed neighbours only narrow it (it takes the general
  centred path with `available_width`) and only when they reach its widest band.
- Collisions: a drop overlapping a neighbour (measured from the bar's live geometry; the title
  counts as its minimum band) slides to the nearest free side, `spacing.xs` apart; nothing is
  pushed or reordered; with no free spot the drop is refused (the ghost returns). A packed
  widget leaving its row pins the widgets beyond it at their current positions.
- Pills stay groups placed the same way; dropping onto a pill still inserts by pointer order.
- Preview: the ghost (moved by surface position, B0210) is the preview; a bar drag keeps the bar
  layout frozen (the source dims in place) and writes once on drop. A snapped drop settles from
  the release point into its landing.

## Removed
`centre_shared`, the centre row (28-72 %) and `local_layout` sharing, B0210 `slot.gap`
(`bar_gap`, the spacer row, gap normalisation), the per-gesture geometry freeze
(`steady_context`, `widgets.drag.geometry`), the per-motion bar preview (`transient_layout`,
`same_slot`, `drag_placeholder` rendering, the spacer mode of `prepare_node`, `widgets.drag.preview`),
the insertion-slot box, the bar drag outline, the desktop zone (and `bead.widgets.zone-desktop`),
the ghost's accent border/marker, and the B0208/B0210 bar tests replaced below.

## Proof
- `widget_editor_tests`: `bar_drops_stay_exactly_where_released_and_the_title_stays_centred`
  (far left at an arbitrary offset, just left of the title, right; real bar geometry; title
  centre unchanged; offsets survive reload), `a_drop_over_a_neighbour_snaps_flush_beside_it_and_pushes_nothing`,
  `moving_a_packed_widget_pins_the_ones_beyond_it`, `bar_motion_changes_no_layout_until_the_drop`
  (60 motions: no write, frozen layout), `the_owner_layout_migrates_pixel_identically` (rects and
  pixel hash measured on origin/main), `drag_visuals_are_quiet_and_drag_surfaces_are_layer_canvases`.
- `widget_drag_preview_tests`: `motion_keeps_the_frozen_layout_without_write_and_drop_commits_once`,
  `real_bar_session_keeps_its_layout_until_the_drop`.
- `widget_canvas_cost_tests::a_clock_tick_redraws_the_desktop_canvas_at_a_bounded_cost`
  (glow <= 4x the plain redraw; was ~45x); `bead-render`
  `text_shadow_blur_cost_does_not_grow_with_its_radius`.

## Open
- Real-input captures on the desktop test host (bar drop left of the title, far-left drop,
  GL desktop drag frame rate) were not run in this unit's time box; the B0210 harness is at
  `~/nacre-lead/b0210/.lead/b0210-capture.py` on that host.
- A widget dropped over the tray area (not a widget, not in the geometry) is not snapped.
- `widget_app_tests` share one temp root per process and can race when run in parallel
  (pre-existing; seen once locally on dev-2).

## Memory
No new always-on state. The ring no longer keeps an animation state.
