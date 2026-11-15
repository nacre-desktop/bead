# 0261 Documentation and tutorials pass (B0261)

Bring the reference pages and tutorials up to date with the code and execute every snippet, so a newcomer
can follow them. Docs only; code problems found on the way go to a bug unit.

## Status

Pass 2 (2026-11-15), against origin/main fff5e3a1:

- Pages: README, docs index (a numbered "Start here" path), COMPONENTS, `reference/{kit,luau-ui,manifest,
  rust-hosts,services}`. New entries: motion tokens and `ui.animated`/`ui.pulse`/`ui.mix`, `Panel.body`
  and `join` (0263), Badge, Image tint and radius (0264, 0276), GraphCanvas, kit batches 1 and 2 (0274,
  0276), interaction poses (0280), `#[non_exhaustive]` `Options`/`WindowSpec` (0262), closed-panel
  wakeups and `PanelVisibility` (0271), lip tab, `LipState::attach`/`hovered` and manifest `[lips]`
  (0279), `UiPackage` (0285).
- Executed on dev-1 in the prebuilt image: every Luau snippet of the reference pages as a `bead check` folder
  (26, 3 fixed: they need the `surfaceContainer` palette role, now said), the tutorial apps assembled from
  the doc text (bar, widget, popup: check, run under headless sway, screenshots equal to `docs/images`),
  the widget tutorial's live edit and capability error, and the 3 Rust host snippets (compiled, run).
- Links: 70 Markdown files, 0 broken. Boundary scripts pass. No image regenerated (the UI did not change).
