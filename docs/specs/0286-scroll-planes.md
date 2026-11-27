# 0286 Scroll planes (B0419)

Composited scrolling: every `ui.Scroll` becomes a plane whose content main can move as one, with
text rastered ahead of the viewport while it moves. Status: U1 (planes in the scene), U2a (GL
shifts), U4 (kits), U2b1 and U2b2 (main stepping and its switch-on), U5a1 (`axis="x"`) and U5a2 (`on_settle`)
landed; later units (U2b3 thumb and windows, U3 CPU fallback, U5a3 snap and motion, U5b
`ui.scrolled`) append their contracts here.

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
- Vertical only: an `axis="x"` Scroll carries no plane (U5a1).

Tests: bead-ui `materialized_tests` `scroll_plane_*` (geometry at rest and moving, overscan
validation, the text 150 px below the clip rasters only while `bead.scrolling` names the plane,
a scroll patch with no Luau equals a full layout, at rest equals today's scene and rasters, a
patch under a translating ancestor); bead-render `tree.rs`
`scroll_plane_settle_drops_overscan_rasters_on_the_thumb_path`.

## U2a: GL composite of scroll shifts

`Renderer::scroll_shifts(&[(item range, clip, shift)]) -> bool` is a per-surface hint like
`presentation_motion`; the default returns true only for an empty set, so every renderer without
an override means "take the worker path". `GlRenderer` stores the latest set (an empty set clears
it) and `paint` applies it per item with the pure fns `shift_for` and `placement` (tree.rs since
U3, shared with the CPU presenter):
- shift = the sum over every plane whose range contains the item (nested planes compose), added to
  the paint offset, so the device-px rounding of images and text is unchanged;
- scissor = the innermost plane's clip (unshifted) when the item's own clip is that clip or absent;
  an inner clip moves with the shift and is cut by the plane clip;
- cull: an item whose shifted bounds miss the plane clip is skipped. Nothing re-uploads.

Nothing calls it with a non-empty set until U2b, so an empty set paints exactly as before.
Known limit: an inner clip already cut by the viewport at bake time is shifted as cut (see the
`shortcut:` comment on `placement`).

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

## U5a1: `axis="x"`

- `ui.Scroll{axis="x"|"y"}` (default `"y"`; a markup error on another node or value). An x
  Scroll's offset runs along x: children are placed at `x - offset`, the limit is the content
  width minus the viewport (Taffy `scroll_width`), and a store patch moves them with no Luau
  (`Topology::place` takes a dx and a dy scroll per layer).
- Nodes carry `scroll_horizontal` (their own or inherited viewport). `Session::scroll(x, y,
  delta, horizontal)`: a viewport takes only deltas along its axis; scroll actions and graphs
  take y only, as before. Bead's host dispatches `UiEvent::HorizontalScroll` to it (it was
  dropped).
- An x Scroll draws no scrollbar (the overlay bar is vertical); the vertical keys (Up/Down,
  PgUp/PgDn, Home/End) skip it for the nearest y viewport; AccessKit reports `scroll_x`.
- No x plane yet: an x Scroll's layer has `scroll: None`, so main never steps it and a vertical
  wheel can never move an x plane on main; its `overscan` and `ready` are inert. x deltas keep
  today's worker path. Main-side x stepping needs `ScrollPlane.horizontal` (bead-render).
- A host's pin bump: `session.scroll(x, y, delta)` becomes `session.scroll(x, y, delta, false)`,
  and `UiEvent::HorizontalScroll { x, y, delta }` goes to `session.scroll(x, y, delta, true)`.
- Known limits: no Left/Right key scrolling; a vertical wheel over an x Scroll scrolls its y
  ancestor; a Scroll lays out as a column, so the content is a Row (or `direction="row"`).
- Tests: bead-ui `materialized_tests` `axis_x_places_content_by_the_x_offset_and_takes_only_x_deltas`
  (placement, limit, no plane, the patch equals a full layout, each axis moves only its viewport,
  keys, AccessKit, validation); bead `host` `folder_source_dispatches_horizontal_scroll_to_an_x_scroll`.

## U5a2: scroll viewports on main and `on_settle`

- `bead_render::ScrollViewport{id, rect, horizontal, offset, settle}` is one scroll viewport as
  main's wheel sees it: the clipped rect, the axis, the store offset when read, and whether it has
  `on_settle`. U5a3 adds `snap` and `motion` with their props.
- `SceneSource::scroll_viewports()` (default: none) lists them; `Session::scroll_viewports()` fills
  it, the worker mirrors it per packet, and Bead's host forwards it. A host with its own
  `SceneSource` must forward it too, or its worker-path bursts get no settle (as today).
  Main reads it through each surface's source (window and overlay expose `source()`).
- `ui.Scroll{on_settle=ui.action(name)}` (Scroll only; a markup error otherwise). The action runs
  once per settle of a viewport that moved since its last settle, with the final offset as its
  argument: `Session::scroll` notes the store it moves, and `Session::scroll_settled(id)` runs the
  action and clears a matching `bead.scrolling` in one stage. The worker commits a burst's last
  scroll before it stages the settle, so the store already holds the final offset.
- Main: each content wheel or finger event notes the viewports with `on_settle` under the point
  along its axis; at the burst's end (`settle`, after the final syncs) it sends one
  `ScrollSettled { id }` per noted or named id, once each. A main-stepped plane is named anyway; a
  worker-path burst (CPU, x, a source without planes) now settles too, but only over a viewport with
  `on_settle`, so every other burst keeps today's events exactly.
- Not covered: keyboard scrolling, scrollbar drags and programmatic writes run no `on_settle`.
- Tests: bead-wayland `plane_tests`
  `a_burst_over_on_settle_sends_one_scroll_settled_on_the_worker_and_the_compositing_path`,
  `a_renderer_that_cannot_composite_gets_todays_events` (now with viewports without `on_settle`,
  on the other axis and elsewhere); `worker` `a_source_without_scroll_viewports_lists_none`;
  bead-ui `materialized_tests` `on_settle_runs_once_with_the_final_offset_after_a_burst`.

## U3: CPU presenter shifts

The host's panels draw with `TreeCpuRenderer`, so before U3 the probe refused and every scroll
frame was a worker round trip (`plane=- main_steps=0`).
- `TreeCpuRenderer::scroll_shifts` stores the set with the content key of the frame it was placed
  against and returns true for an empty set, or when the last render painted its frame directly
  with one item per scene primitive. It returns false, so main takes today's worker path, after a
  frozen snapshot (a suspended GL panel: only its group moves), an opacity group (a fade or
  `OpaqueComposition::Cached`), a reveal slide, a frame not one item per primitive (an imported
  posed frame), or before any render. The probe (`0..0`) gets the same answer from the last
  render, so it is true exactly when a real set would be painted. A state change at the next
  draw falls back at the next probe, as for a GL fallback (U2b1).
- Paint: the direct paint (`paint_items`) places each item with U2a's `placement`: shift = the
  sum of the containing planes, added to the paint offset (main snaps it to device px, so the
  rounding of rasters and fills is unchanged); scissor = the innermost plane clip, or an inner
  clip moved with the shift and cut by the plane clip; items whose moved bounds miss the plane
  clip are skipped. `opaque_start` places items the same way, so a fill that covers the damage
  only at its baked position never hides what lies below it. Rasters are reused as baked;
  vectors inside the damage are rastered as today. An empty set paints exactly as before.
- Shifts apply only to the frame whose content key they were placed against. A close's
  layer-motion atlas renders a newer scene through the same renderer without the draw hook,
  and paints it as baked. The settled-opening copy (B0205) is skipped while shifted, because its
  pixels are unshifted.
- Damage: no change. U2b1's hook adds the plane clip (device px, rounded out) while the plane
  moves or was last shifted. The SHM path tracks it per buffer, so every retained buffer
  repaints the clip, and a main-only step (same frame, no item damage) repaints exactly the
  clip.
- Plane copy (U3b): while one plane is shifted, the renderer keeps its clip as last painted
  (target bytes, clip px × 4: 2.1 MB at 560x960, 4.8 MB at 840x1440), with the content
  signature and presented scroll position (device px) it shows. A repaint whose damage is exactly
  the clip, with the same signature and an integral device-px delta smaller than the clip, copies
  those rows moved by the delta, then paints only the exposed band and every overlay's column
  (items drawn above the plane that reach the clip, such as the scrollbar thumb or a focus ring,
  over the clip's full height, at their last and current x). So neither their old place nor
  their new one ghosts. Then the clip is copied back. The copy is independent of which SHM
  buffer is painted, so buffer rotation needs nothing.
  - Proof it needs, else the whole clip is painted (and the copy kept, freshly painted): one
    plane and no x shift; one solid opaque fill (no radial) covering the clip as the last item
    under it; the same signature; an integral delta; damage that is exactly the clip.
  - The signature hashes the clip, scale, format, panel offset, the fill under the clip, and the
    plane's items in content coordinates (y + the plane offset; inner clips the same way). A
    worker re-bake at the new offset that main's shift compensates keeps it, a changed row or
    a new row in the plane does not, and overlays are not in it.
  - Without that proof (more than one plane, no solid fill), and on any render that is not a
    shifted direct paint (an empty set or the settle, a fade, a slide, a frozen snapshot, a
    close's atlas with a newer frame), the copy is dropped. It also goes with the repair
    scratch after 500 ms quiet. Damage outside the clip keeps it.
  - The copy's allocation may stay resident until malloc trims: glibc can keep a freed block of
    this size in its arena. Allocator trims are the POSTUSE design pass's, so U3b adds none.
- GL fix: gl.rs `paint` added the shift to the offset before placing the scissor, so the plane
  clip moved with the content and an inner clip moved twice. The scissor now uses the unshifted
  offset (one line moved).

Cost (laptop, `cargo test --release` with LTO off, BGRA, a list of rounded cards with an icon and
two text lines, 120 steps of 4 px; the job runs at idle CPU weight, so p95 is inflated), U3a's
whole-clip step (the U3b fallback):
- a 560x960 list clip on 1920x1080 at scale 1: p50 2.1 ms, p95 7.9 ms;
- the same list at scale 1.5 (840x1440 device px on 2880x1620): p50 9.1 ms, p95 12.9 ms;
- a full 1920x1080 clip at scale 1: p50 10.6 ms, p95 16.1 ms.
U3b band steps on the same cases (same run as its fallback; p95 is noisy at idle CPU weight):
- 560x960 at scale 1: band p50 0.21 ms, p95 5.2 ms (fallback p50 2.4 ms, p95 7.6 ms);
- 840x1440 at scale 1.5: band p50 0.51 ms, p95 5.6 ms (fallback p50 9.8 ms, p95 13.6 ms);
- full 1920x1080: band p50 0.97 ms, p95 6.2 ms (fallback p50 11.8 ms, p95 19.5 ms).

Tests: tree.rs `a_plane_baked_at_100_drawn_shifted_up_50_equals_it_baked_at_150` (scales 1 and
1.5; red with `scroll_shifts` returning `shifts.is_empty()`),
`nested_plane_shifts_compose_an_inner_clip_moves_and_items_off_the_clip_are_culled`,
`the_probe_composites_only_after_a_direct_paint_of_one_item_per_primitive`,
`a_shift_change_repaints_the_plane_clip_only_and_leaves_no_stale_rows` (sentinel outside the
damage; a band where an unshifted `opaque_start` would leave stale pixels); timing, ignored and
run in release by the lead's checks: `cpu_plane_step_paint_time_at_1920x1080` (band and fallback).
U3b: `plane_band_steps_equal_a_full_clip_paint_in_both_directions_with_a_thumb` (scales 1 and
1.5, down, a compensated re-bake, a thumb move, up; red with the band path forced off),
`plane_copy_fallbacks_paint_the_whole_clip` (non-solid background, fractional delta, damage
beyond the clip, content change), `the_plane_copy_is_dropped_on_an_empty_set_and_a_render_without_shifts`.
