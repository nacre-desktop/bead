# B0210 Widgets round 2: smooth drags, free bar, ordinary Widgets window, pearl blur

Status: candidate on `task/claude-b0210-widgets-r2-20261030`; lead live gate pending.
Bar model (section gaps, centre row, insertion slot) superseded by B0211 (anchored offsets).
Pairs with Nacre N0103 (`task/claude-n0103-pearl-rule-order-20261030`: the generated session
includes `bead-pearls.kdl` after `desktop.kdl`, whose session-wide window rule otherwise wins).

## Owner feedback on the live B0208 build (2026-10-30)
1. Desktop widget drags still lag. 2. The grid was meant for the BAR, not the desktop; the bar
only took drops at the top right. 3. The Widgets window opens with a ghost effect, has extra
borders and no focus border. 4. The detach ghost: pearl windows inherit the global blur.

## 1. Drag cost: causes (real input, nested niri 2880x1800 @1.5, 120 Hz parent, CPU renderer)
A placed desktop clock dragged 1.6 s across the desktop (`virtual-pointer`, 4 ms steps):

| | before (origin/main + markers) | after |
|---|---|---|
| ghost frames presented | 36 Hz (gap median 24 ms, p95 40, max 64) | 99 Hz (gap median 8.0 ms, p95 16) |
| ghost repaints during the move | 62 (CPU paint median 7.6 ms, p95 32 ms, ~312 k px each) | 1 (the first frame) |
| ghost surface re-allocations | 5 (resize + new SHM pool + configure round trip) | 0 during the move |
| ghost content rebuilds (Luau + layout) | every motion | 0 |
| position smoothing | 60 ms out-cubic easing on purpose (trails the pointer) | none: surface at the pointer |
| pointer to surface commit | n/a | median 20.5 ms, of which ~2 ms ours (host model); the rest is nested input delivery |

Causes: the ghost and the drop zones/grid were one full-output overlay canvas re-rendered from
Luau for every motion (the pointer was a store value), its visible bounds changed with the
ghost (surface resize, new 2880x1800 SHM buffers, configure round trip), every frame repainted
the ghost and grid rectangles on the CPU, and the ghost position was eased over 60 ms. The
desktop canvas also received every pointer value (`widgets.drag.x/y` in its projection).

Design: **the ghost is its own canvas drawn relative to the pointer and moved by position.**
- Framework (general): `UiEvent::CanvasTranslate {view, output, x, y}` places a canvas whose
  scene is anchor-relative at an output-space anchor: a layer margin change and an empty
  commit, no buffer, no repaint, no layout (`App::canvas_translate`). Anchored canvases are not
  clipped to the output before placement. `CanvasLayer::Top` (above windows, below overlays).
- Shell: the ghost runtime (`with_widget_drag_preview(true)`) offsets its scene by the pointer
  it rendered at; the host sends the ghost a Model only when something other than the pointer
  changes (`widget_drag_content`: destination/form, phase, source) and a `CanvasTranslate` per
  motion. While dragging the ghost draws from the frozen base, not the per-motion bar preview.
  The settle/return animation runs in pointer-relative offsets, so a drop animates from where
  the ghost is. The drop zones moved to their own static `widgets-zones` canvas (Top layer);
  "over Remove" is a model flag computed by the drop model, not a pointer read.
- The desktop canvas no longer receives the pointer.

## 2. Desktop: free placement; bar: anywhere along its width
- **Desktop grid removed** (cell token, landing grid lines, per-motion landing). A drop lands
  at the ghost's position; if it would overlap another widget it slides to the nearest free
  side of that widget (`desktop_gap` = `spacing.sm`). Nothing is drawn while dragging.
- **Bar model: sections plus gaps.** The bar keeps its three sections (left from the left
  edge, a centred centre band at 28-72 %, right from the right edge) and pointer-order insertion
  between any two widgets. New: a widget in a side section keeps `slot.gap` (px, multiples of 4)
  toward its section's edge, measured from the bar's live geometry at the drop, so it lands
  where it is dropped: far left (before the launcher), anywhere left of centre, anywhere right
  of centre, between any two widgets or pill children. The centre band centres its row
  (a drop just left of centre sits left of the title). Gaps survive normalisation on both VMs
  and reloads; the renderer adds a spacer on the edge side. No spacer widget was needed.
- **Steady geometry:** within one gesture each widget is measured where it first stood
  (`widgets.drag.geometry`), so the preview, which shifts neighbours by the gap, cannot feed back
  into the insertion point (a real-input capture caught the clock pushing the launcher ahead of it).
- **Insertion indicator:** while a widget is over the bar, the bar shows the existing outlined
  placeholder at the computed slot and gap, so it follows the pointer across the whole width.
- Likely why drops "only attached top right" before: regions were output thirds with order-only
  packing, so any drop in the right third (or over the pill) joined the pill/right section,
  and a left-third drop packed against the workspaces regardless of where it was released.

## 3. Widgets window
The app no longer draws its own outline or radius; the `nacre-widgets` rule no longer turns
niri's border and focus ring off (the focus border lights like any window, no double border).
Its background is opaque, so its rule sets `background-effect { blur false; }`: the session's
xray blur only showed the blurred wallpaper through the window while it animated open.

## 4. Pearl blur (detach ghost)
`shell/niri-pearls.kdl` pearl rule: `background-effect { blur false; }` (confirmed live by the
owner with the lead's temporary override). B0209's early release of the cover layer's own blur
is reverted: the ghost was niri's window blur behind the invisible prewarm, and releasing the
cover's blur made the cover's last frame lose its blur during the handoff.

## Removed
Desktop grid (`grid_cell`, landing/grid drawing, `desktop_preview`, per-motion landing),
the ghost's per-motion position easing, the zones inside the ghost root, the app's own window
outline, the `nacre-widgets` border/focus-ring-off rule, B0209's `release_cover_effect`, and
the B0208 grid tests (replaced).

## Proof
- `widget_editor_tests`: `desktop_drops_land_where_dropped_without_a_grid_and_never_overlap`,
  `desktop_motion_moves_only_the_ghost_surface_and_writes_only_on_drop` (120 motions: content
  unchanged, anchors follow, one write on drop), `the_ghost_is_drawn_relative_to_the_pointer`,
  `the_bar_takes_a_widget_anywhere_along_its_width` (far left, left of centre with a gap,
  centre before the title, between wifi and bluetooth, gap survives reload),
  `a_bar_drag_shows_the_insertion_slot_under_the_pointer`,
  `niri_rules_keep_pearls_unblurred_and_the_widgets_app_an_ordinary_window`;
  `owner_layout_loads_unchanged_with_the_title_as_a_widget` still passes (owner fixture);
  `widget_drag_preview_tests` updated: a pointer-only update is a `CanvasTranslate`, and the
  desktop canvas gets no Model for it. `niri validate` accepts the rules on niri 26.04.
- Markers (`BEAD_FRAME_TRACE`): `CANVAS_FRAME/MAP/MOVE/PRESENTED view=…`, `WIDGET_DRAG_HOST`.
- Captures: see the report (`~/nacre-lead/b0210/evidence` on the desktop test host).

## Memory
No new always-on state. During a drag the zones canvas is a second (mostly empty) surface;
the ghost surface is ghost-sized instead of output-sized while it moves.
