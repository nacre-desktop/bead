# B0129 — launcher icons on the first frame

The launcher must open with every visible app icon already rasterised, including
after its 120-second panel-worker expiry. Path lookup caching alone cannot keep
decoded pixels alive across that expiry. Startup warming must happen after the
bar presents, and catalog updates must refresh likely icons without keeping a
second catalog or view alive.

Budget: prefer **1 MiB** additional always-on unreclaimable memory; this unit
permits up to **4 MiB** only with a measured smoothness benefit and an explanation.
The general raster LRU is capped at 1 MiB including conservative entry charges;
resident worker/allocator overhead must be measured separately. Clean file PSS is
informational. Compare CPU renderer, identical full-release flags, same private
output geometry, at least five alternating control/candidate pairs.

Acceptance: no missing-icon frames for cold, warm, and expired opens; first-frame
open latency no more than control +2 ms; first-key UI CPU at most 8 ms. A bounded
initial-data wait must prevent committing an empty intermediate model. A failed
icon gets a fallback; an unavailable service must not block opening indefinitely.
Overview, palette, picker thumbnails, widgets, and drag latency are out of scope.

The renderer shares a small-image raster LRU across panel workers, keyed by
immutable contents, physical dimensions, fit and path. It admits images at most
128×128 physical pixels, with at most 256 entries and a 1 MiB charged-byte cap.
SVG and PNG use the same decoder for warming and drawing. All visible images
already bake synchronously before a worker's Ready packet reaches the presenter;
that existing transaction is the per-open raster readiness fence.

After the bar first presents, a lower-priority, coalescing worker queries Home
and the first apps page through the published engine contract. It retains up to
64 unique icon rasters, preserves engine order, refreshes on menu/scale changes,
and resolves recent IDs from `menu.preferences` against a transient bounded apps
reply. It keeps no query result or view alive between requests. A tiny offscreen Luau
view reads the same image dimensions as the visible launcher. The first menu
projection waits up to 20 ms for data; later typing never waits. Known invalid
or unreadable icons use the existing fallback, and a query timeout shows themed
skeletons. The worker is joined on shutdown and checks stop between work units.

Validation and measurements are pending. `spikes/launcher-icons-proof.py` uses
64 private SVG/PNG desktop entries, 12 favorites and 8 recents, with the real menu
engine. It correlates every `PANEL_PRESENTED` with its accepted scene update and
model/raster readiness. This avoids the existing draw trace's first-two-seconds
suppression. Nested presentation is not physical laptop scanout timing.
No temporary replacement or duplicate implementation is introduced.
