# 0286 Scroll planes (B0419)

Composited scrolling: every `ui.Scroll` becomes a plane whose content main can move as one, with
text rastered ahead of the viewport while it moves. Status: U1 (planes in the scene), U2a (GL
shifts) and U2b1 (main stepping, switched off) landed; later units (U2b2 the switch-on, U2b3 thumb
and windows, U3 CPU fallback, U4 kits, U5a axis x/snap/motion/on_settle, U5b `ui.scrolled`) append
their contracts here.

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
  worker. `ui.Scroll{ready={from,to}}` (U4) bounds it.
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

## U2a: GL composite of scroll shifts

`Renderer::scroll_shifts(&[(item range, clip, shift)]) -> bool` is a per-surface hint like
`presentation_motion`; the default returns true only for an empty set, so every renderer without
an override means "take the worker path". `GlRenderer` stores the latest set (an empty set clears
it) and `paint` applies it per item with the pure fns `shift_for` and `placement`:
- shift = the sum over every plane whose range contains the item (nested planes compose), added to
  the paint offset, so the device-px rounding of images and text is unchanged;
- scissor = the innermost plane's clip (unshifted) when the item's own clip is that clip or absent;
  an inner clip moves with the shift and is cut by the plane clip;
- cull: an item whose shifted bounds miss the plane clip is skipped. Nothing re-uploads.

Nothing calls it with a non-empty set until U2b, so an empty set paints exactly as before.
Known limit: an inner clip already cut by the viewport at bake time is shifted as cut (see the
`shortcut:` comment in gl.rs).

Tests (no GL context): gl.rs `shift_tests` (`nested_planes_compose_their_shifts`,
`items_outside_the_plane_clip_are_culled_and_partial_ones_are_cut`,
`the_viewport_clip_stays_and_an_inner_clip_moves`); lib.rs
`a_renderer_without_an_override_takes_the_worker_path`.

## U4: kit lists materialize in bands and declare `ready`

- `ui.Scroll{ready={from=,to=}}` (content px; numbers, `from <= to`, Scroll only; a markup error
  otherwise). `layout::plane` sets the plane's `ready` to the declared span within the interest span
  (declared ∩ interest); absent means the whole interest, as in U1. `materialized::Scroll` keeps it,
  so a store patch re-places the plane with the same declaration.
- `kit-virtual-list` (fixed and variable heights) and `kit.VirtualGrid` window the rows in bands of
  half the viewport: the one Luau dependency is `ui.window_index(scroll, band, prefix, max)`.
  The materialized span is `[k*band, k*band + viewport + 3*band)`, so at every offset in the band
  the viewport has at least half a viewport of rows on each side. A step inside a band re-runs no
  Luau; crossing a band edge re-runs once. Variable heights use the offsets table and the same band
  dependency (no `resolve_for_check` of the scroll store).
- Each kit declares `ready` as the span of rows that exist (rows past a lazy `total_count` prefix
  are not ready; the content's own start and end are included when the window reaches them).
- At rest the visible rows and positions are unchanged; the extra rows sit outside the clip and get
  no rasters (U1 interest rule). Cost: about 2 to 3 viewports of row nodes instead of 1 to 2.

Tests: bead-ui `materialized_tests` `kit_lists_rerun_luau_only_when_a_scroll_step_crosses_a_band`
(fixed, variable, grid), `kit_lists_at_rest_show_the_same_rows_at_the_same_positions`,
`scroll_plane_ready_is_the_declared_span_within_the_interest`, and the `ready` errors in
`scroll_plane_geometry_at_rest_and_while_moving`.

## U2b1: main stepping (switched off until U2b2)

Panels and layer surfaces (`window.rs` is U2b3). `scroll.rs` keeps a pure `Planes` map per
(target, plane id): `live` (shown), `baked` (last adopted scene), unsent motion, the running
totals of the syncs in flight.
- Step: a content wheel, fling or finger Δ goes to the innermost plane under the point that can
  take it (the `Session::scroll` rule). While the plane moves (`interest != clip`) and the
  viewport at `live + Δ` stays inside `ready`, main moves it and draws: no `UiEvent`. Otherwise
  the worker gets Δ plus the unsent motion as today's `UiEvent::Scroll`; for a moving plane that
  is a stall. `scroll_steps` nodes and the x axis keep today's path.
- Probe: the step asks the target's renderer `scroll_shifts(&[empty range])`. A renderer that
  refuses (CPU until U3, wgpu, frozen snapshots) takes today's path with no plane state at all;
  one that stops compositing mid-motion (a GL fallback) hands the unsent motion to the worker
  in that step and drops its planes, so nothing snaps back.
- Syncs (`UiEvent::Scroll` of the unsent motion at the last step's point; the worker's
  `ContentScroll` coalescing is unchanged): 50 ms after the last; within ½ overscan viewport of
  a `ready` edge that is not the content's end, with nothing in flight; before any other input
  (`view_event` flushes every tracked plane, on every surface, before delivering it, so hit
  tests and keys see the offset main shows); at motion end. They ride the motion frames and
  input: no timers, nothing at idle.
- Adopt (every step and draw): a plane offset equal to baked + some prefix of the syncs in
  flight (clamped; the longest prefix wins) keeps `live`, since the worker can bake or merge any
  prefix. Any other offset is a worker write (keys, Luau, clamp, reload): `live` takes it and,
  if main was moving the plane, the target's wheel/fling/finger motion stops.
- Draw (`rendering.rs`, after the drawn-ahead pass returns and before any paint): shift =
  baked − `live` clamped to `ready`, snapped to device px, passed with the content range and
  clip to `scroll_shifts` on every draw (empty without planes: one renderer draws every layer
  surface, so a set never outlives its surface's draw). Damage += the clip of each moving or
  last-shifted plane, so a main-only step always repaints, including the prepared GL snapshot
  path and never a callback-only commit; a pose drawn ahead (B0422) only prepared rasters and
  damage, so its present pass paints the current shift. A plane drops once its motion ended,
  its syncs are baked and it is drawn unshifted.
- Switched off: nothing sets `bead.scrolling` yet (U2b2), so no plane moves, every step is
  today's worker step and frames and events are unchanged.

Known limits (U2b1):
- A scroll-action node (`scroll=ui.action`) inside a moving plane is invisible to main: a wheel
  over it while the plane moves scrolls the plane. The first step of a burst always reaches the
  worker (at rest `ready` is the viewport), so only a later step at a new point is affected.
- Two-way scrolling that cancels out within one flight can match an older prefix; the plane then
  takes the scene's offset (as for a worker write) and its motion stops.
- The focus ring of a focused node inside a moving plane stays at its baked place until the next
  sync (≤ 50 ms); the thumb follows the syncs (its shift is U2b3, with windows).
- Nested planes: only the plane main moves is shifted; the U2a shortcut on cut inner clips holds.

Tests (no Wayland, `scroll.rs` `plane_tests`): `a_step_inside_ready_sends_no_ui_event`,
`a_step_outside_ready_falls_to_the_worker_and_counts_a_stall`,
`syncs_fire_at_50_ms_near_the_ready_edge_and_before_input`,
`a_worker_write_is_adopted_and_stops_the_motion`,
`a_scene_that_merged_two_of_three_syncs_is_no_worker_write`,
`the_settle_sends_the_final_sync_once_and_drops_the_plane_once_baked`,
`a_renderer_that_cannot_composite_gets_todays_events`,
`a_renderer_that_stops_compositing_mid_burst_sends_the_pending_motion_at_once`,
`one_surface_shifts_never_reach_another`; the SCROLL_FRAMES line in
`burst_lines_report_gaps_and_skip_single_steps`.

## U2b2: the switch-on

- Start: the first step of a burst that hits a plane on a compositing surface (the U2b1 probe)
  sends the worker `UiEvent::Model({"bead.scrolling": id})` once, before that step's
  `UiEvent::Scroll`. The worker re-bakes with interest = viewport ± overscan, so `ready` grows
  and the next steps stay on main. Not per step; not on CPU, wgpu or frozen surfaces (no
  plane state, today's events exactly).
- Settle: at motion end, after the final sync, one `UiEvent::ScrollSettled { id }` per named
  plane. It is a worker barrier like any non-scroll event (never merged into a `ContentScroll`).
  `Session::scroll_settled(id)` clears a matching `bead.scrolling` (a patch, no Luau, like
  `bead.hovered`) and ignores another id; the plane re-bakes at rest, so overscan rasters live
  only between the start and the settle.
- Close/park flush: a panel close (`close_panel`, before `PanelVisibility(false)`), a panel
  release, a removed output session, and a pointer press or key that interrupts the motion first
  deliver the target's final sync and ScrollSettled (`settle_scroll`), then drop its planes: a
  parked worker's last bake is at rest, a reopen starts at the store offset, and a press no
  longer drops unsent motion.
- A host's `UiEvent` match must add `UiEvent::ScrollSettled { id } => session.scroll_settled(&id)?`
  (bead's own host has it). The start signal is an ordinary `Model` store key.
- Tests: `the_start_signal_is_sent_once_per_burst_per_plane_before_the_first_step`,
  `the_settle_sends_one_scroll_settled_after_the_final_sync`,
  `a_close_flush_settles_only_its_own_target`, `a_renderer_that_cannot_composite_gets_todays_events`
  (no start, no settle), bead-ui `scroll_settled_clears_the_named_plane_with_no_luau_and_ignores_another`.
