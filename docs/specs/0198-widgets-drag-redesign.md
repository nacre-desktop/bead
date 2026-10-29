# B0198 Widgets: drag-only placement, edit-mode drop zones, robust drops

Status: candidate on `task/claude-b0198-widgets-drag-redesign-20261029`; lead live gate pending.
Pairs with Nacre N0099 (the Nacre-style Widgets app icon).

## Owner feedback (2026-10-29) and scope
1. Library structure: owner later kept the Overview + per-widget pages ("it can stay like this");
   only a clearer Overview entry was asked for.
2. Dragging onto the bar/desktop was not obvious: show drop targets on the real bar and desktop.
3. Dragging the clock out of the pill and back skewed everything to the right: make drops robust.
4. No "In the bar" / "On the desktop" toggles or Add/Remove buttons; placement is by drag only.
5. The detail preview did not show the bar.
6. Nacre Widgets needs its own Nacre-style icon (header, desktop entry, window).

## Root cause of the skew (point 3)
The first drag converted the bar from natural content packing into a 60-tile grid
(`tile_document`): spans were measured from geometry (the clock got span 4), widgets were
translated by `start * pitch`, the pill was given `span * pitch` width and every child was
wrapped in a `span`-wide centred row. Re-entering the pill appended the clock after
notifications with its inflated span, so the pill content was pushed right and padded.
Evidence: `.lead/tmp/widgets-after-bad-drag.json` (clock centre/6/4, power right/0, mode tiles).

## Design
- **The bar is order-based.** Layouts always use content (natural) packing, the same as the
  shipped default. A slot is only an order key: per list (top level, each pill) and region,
  starts are contiguous from 0 and the span is the selected form's `min_span` (1 for every
  shipped widget). Rendering no longer depends on the preset name, so a dragged layout renders
  exactly like a clean layout with the same order.
- **One normaliser, two VMs.** `registry.normalise` (markup side, repairs on load: bar, canvas
  and app all normalise the document they read) and `normalise` in `app/behavior.luau` (the
  placement model, normalises every preview and commit). Rules: regions left→centre→right in
  list order, saved order kept (start, then list index), contiguous starts, form spans, only
  pills keep children, canvas slots clamped inside the canvas, saved bar placements mirror the
  live item. The behaviour VM cannot `require`, so the ~40-line normaliser exists twice; both
  are covered by the property tests below (keep them identical; no expiry, VM boundary).
- **Drop = detach, insert by pointer order, normalise.** The target list is the pill under the
  pointer, else the top-level region by output thirds. The insertion index comes from the
  measured centres of the siblings (`context.geometry`); between two pill regions the nearer
  neighbour's region wins. Invalid drops (no form for the destination, a pill into itself, a
  stale revision, an app drop inside the app) return `false` and the ghost snaps back.
  Undo pushes the exact pre-drag document.
- **Removal by dragging off.** While a placed widget is dragged, the drag overlay shows a
  "Remove" target at the bottom centre of the output (`dragging.remove_zone`, tokens only);
  dropping there removes the instance (pill children are promoted). The edit-mode hover "x"
  on placed widgets sends the same `remove` op (pointer/keyboard context action).
- **Edit mode drop zones.** During a widget drag the overlay (`widgets/drag/ghost.luau`)
  draws the bar strip and the desktop area as palette-role zones, the active destination
  highlighted; the bar shows the existing snap placeholder at the insertion point.
- **App.** The bar/desktop toggle row and Add/Remove buttons are gone; the drag hint explains
  drag-to-place and drag-to-Remove. The detail stage is first on the page and sized to the
  output aspect (up to 62 % of the window height, so the bar reads at about 0.4× instead of a
  13 px strip) and shows every desktop widget at its slot; the selected widget's placed
  desktop instance is draggable in the stage. The sidebar's Overview has an accent icon, a
  rule and a "Widgets" group label. The header uses `widgets/app/assets/nacre-widgets.themed.svg`
  (the N0099 glyph in the themed Nacre shell).

## Removed (replaced code deleted)
`tile_document`, tile reflow/packing (`reflow_bar_region`, `reflow_preview_region`,
`pack_bar_region`, `next_bar_slot`, `bar_slot_free`, `insert_bar`, `child_slot`,
`bar_drop_placement`, `pill_overlapping_bar_slot`, `resize_pill`, `reflow_pill_child_region`,
`resize_bar`, `grow_saved_bar_placement`, `preferred_bar_span`), the `toggle-form` op, the
renderer's tiles/reservation paths (`local_layout`, `translate_x` slot offsets,
`flex-reservation` rows), the tiles projection, 6 i18n strings, and 7 tests that asserted
tile spans/reflow. App behaviour 1331 → ~950 lines; registry −40 lines.

## Real-input findings (round 2)
- Desktop → bar: the nested compositor confines a Bottom-layer canvas drag to the work area;
  traced pointer events stop at y≈67 (the bottom of the reserved top edge under a 50 px bar)
  and the bar never receives the pointer. `widget_drag::prepare_event` now counts a
  canvas-sourced position within half a bar height below the bar as the bar; the bar
  publishes its widget rects while a drag is active (`note_bar_geometry`) so the canvas drop
  is ordered among them; a bar drop over the pill's x-span joins the pill at any height.
- The bar is its own drop zone (outline on the bar surface while a widget is dragged).
- The pill width is set, not animated: the shrinking right-anchored pill left stale copies of
  itself in the bar mid-drag (partial damage). The animated-pill test was removed with it.
- The drag overlay (passive canvas) now drops the layout's 32 px shadow margin like the widget
  canvas (`Runtime::widget_drag_overlay`), so zones and the ghost are drawn in output
  coordinates and line up with the Remove/bar hit tests (capture with the hit rects overlaid).
- Removed: the unreachable app "entry flight" path (toggle-form enable → flight from the app to
  the bar): reload.rs entry structs/state/timers (~700 lines), the Luau entry-seed/entry-flight
  phases and entry target stores, the entry metadata markers, and their 5 tests.

## Proof
- `crates/nacre-shell/src/reload/widget_layout_property_tests.rs`:
  random drag sequences (12 seeds × 8 gestures: into/out of the pill, between regions, to the
  desktop and back) keep previews and commits valid and render pixel-identical to a clean
  layout with the same order; the clock out of the pill → bar → pill → desktop → back restores
  the default slots and pixels; undo restores the exact document; invalid drops snap back;
  the Remove zone removes; the owner's broken file repairs on load and renders like the clean
  layout with the same order.
- Updated tests: content-mode assertions, `remove` op, preset repair instead of rejection.
- Real input (desktop, nested 2880x1800 @1.5, B0176 harness + `b0198-capture.py`):
  clock bar→desktop→pill and pill→bar→pill both restore the default slots with 0 changed bar
  pixels (`b0198-*-roundtrip-proof.json`); 0 UI_ERROR.

## B0198b: copies, drag sources, drag background (owner feedback 2026-10-30)
- **Copies.** A drag from the Widgets app always adds a new instance with its own group
  (`clock`, `clock-2`, `clock-3`, ...): its own settings, style and colour in
  `widgets[group]`, a unique iid, stable order through normalisation. A placed instance moves
  (or changes form) only when that instance itself is dragged on the bar or desktop. Removing a
  copy (Remove zone or the edit-mode x, which now names the iid) drops its empty group state;
  the base group keeps its saved style. The page's "Choose a copy" picker now works
  (`widgets.navigate` `instance`).
- **Drag sources.** The style cards on each widget page (bar and desktop) are the drag sources:
  a grip in the corner, a "Drag onto the bar / desktop" hint beside each section heading, and the
  sections sit at the top of the page. Overview cards' bar/desktop previews are drag sources
  too. "Add to bar" / "Add to desktop" add a copy without dragging (end of the status pill, or
  cascaded at the desktop centre). The screen stage is a live preview only (no drag source, no
  candidate widget); render_bar's entry-candidate path is gone.
- **Drag background.** The desktop drop zone is an outline (no tinted fill over the whole
  desktop); a desktop-form ghost has no card behind its own rounded glass (the rectangle showed
  around it). Bar ghosts keep their card. The Remove zone keeps a light tint.
- Tests: `dragging_from_the_app_adds_copies_with_their_own_settings` (two more clocks on the
  bar, per-copy colour, a second and third battery on the desktop, move, remove, exact undo,
  Add buttons) and random sequences now include app adds.
