# 0274 — Kit batch for the design review

Owner-approved review components, mostly pure kit Luau (`crates/bead-kit/assets/kit.luau`). Call-site shapes follow the Nacre stand-ins (`shell/widgets/segment.luau`, N0238-N0243 asks) so the Shell can switch with minimal edits and delete the stand-ins at its next pin bump.

## Components

- `kit.Segment { options, value?, enabled?, aria_label?, on_change }`: 34 px `surfaceContainerHigh` pill track, one full-radius option each (caption text, weight 500), selected option `primary`/`onPrimary`. `options` are `{id, label, selected?, enabled?}`; `value` selects by id. Same call shape as the Nacre stand-in's `Segment`.
- `kit.LinkRow { title|label, subtitle|description?, icon?, enabled?, on_activate }`: 56 px row-wide button, title and caption on the left, `chevron_right` at the end, 6 % hover tint, radius `metrics.radius.row`.
- `kit.Chip { text, icon?, on, on_activate }`: 36 px pill, radius 18, label 13/500; `on` is `primaryContainer` with `onPrimaryContainer` ink, else `surfaceContainerHigh`.
- `kit.Tag { text }`: 20 px high, 11 px mono text, 1 px `outline` border, radius 6.
- `kit.SliderTile` end slot: `end_icon`, `on_end`, `end_label`, `end_color` add a 28 px round icon button 8 px from the tile edge (before the details chevron when both are set); the slider and value text shrink to make room.
- `kit.metrics` (frozen): radius `panel` 24, `tile` 18, `row` 12, `chip` 18; text `caption` 12, `label` 13, `body` 14, `title` 16 (logical px). Tiles and `SliderTile` read their radius and type from it; the control tile is now radius 18 (was 24; stacked 20), title 14/600, caption 12. Hosts that need other values keep using `tokens`.
- Press feedback: chips, segment options and tiles use the existing hover fill one step deeper (`onSurface` 12 % instead of 8 %) while the pointer is down. `bead.pressed` is global but the hover fill is per node, so only the pressed node deepens.

## Close duration (framework)

`Motion::transition(false)` capped the close at 0.75 x the open even when the host set `return_duration`. An explicit `return_duration` is now used as written (D10: open .16, close .2); the default is unchanged (0.7 x open, at most .22 s). Test `close_duration_may_exceed_the_open_and_defaults_are_unchanged` (`bead-render`).

## Transcript (framework)

`transcript::Entry` gains `align_end: bool` and `max_width: f32` (fraction of the row; 0 or >= 1 = full): the bubble, text and buttons lay out in the narrower width and shift right for `align_end`. An empty `label` drops the caption band. Hosts constructing `Entry` add `align_end: false, max_width: 0.` (user messages: `true`, `0.88`, `label: ""`). Test `entries_align_to_the_end_with_a_max_width_and_no_label_band`.

## Not built

- Scale 0.97 on press: needs a per-node pressed state and a paint-only scale in the cached scene (the hover patch in `materialized.rs` is the place); the fill step above is the kit-level part. Parked with the framework items.
- Parked (not trivial): hover-only children, shadows on non-panel nodes and shadow prewarm, animated list reflow, `LipState.hovered` and edge-art repaint.

## Evidence

`kit_batch_components_use_the_review_metrics` (`bead-ui`), `examples/kit-gallery` (`bead check`), and a headless-sway screenshot of the gallery inspected during the unit (kept in the worktree's `.lead/`).
