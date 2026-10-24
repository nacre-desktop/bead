# B0100 — Wallpaper picker

Status: implementation and remote verification in progress. N0070 supplies the missing catalogue.

## QML reference

Read from Nacre `origin/main` at `3afcdc64ec5a2150d660bfb5aabb888d3d028ec7`: `NacreLauncherPanel.qml`, `NacreWallpaperPicker.qml`,
`NacreWallpaperHex.qml`, `NacreWallpaperBackdrop.qml`, `NacreWallpaperMotion.qml`,
`NacreWallpaperVideo.qml` and `services/NacreWallpapers.qml`.
Super+W calls `nacre launcher ">wallpaper "`; QML selects launcher mode `wallpaper`.
Bead previously treated this as menu route `style.wallpapers`, showing an inert picker row.
The visual picker is bottom attached and slides from the bottom in 300 ms OutCubic.
Carousel defaults to min(1360, viewport width − 96), at least 320 wide, and 320 high.
Spotlight and hexagons use the launcher’s full scene viewport (the picker’s standalone
implicit dimensions are viewport minus 96/100); both have a selected-image backdrop
with a #70000000 scrim. Backdrop crossfade is 300 ms InOutCubic, settled after 320 ms.

The toolbar contains Static / Dynamic, Carousel / Spotlight / Hexagons and Add.
Search is 40 high, min(640, width − 48) wide, and matches every whitespace-separated
lowercase word against name plus source path. Left/right wrap, wheel accumulates
120 units per selection, Enter applies, Escape closes, and detached Escape clears search.
Clicking a card browses and applies it after the service’s 35 ms selection debounce;
closing commits the queued selection. Current source path anchors selection across refresh.

Carousel has an odd number of slots, 16 px gaps, up to 280 px card width, radius 15,
a 2 px primary selected border and 3 px image inset/radius 12. Circular travel uses
300 ms OutCubic. Spotlight uses a widescreen hero (half available width, max 1000,
height constrained), overlapping smaller side cards and distance opacity.
Hexagons have 3–7 columns, tile width up to 340, height .87×width, .76×width pitch,
alternating half-height column offsets and a polygon mask with matching hit region.
Offscreen cards restrict decoding. Fullscreen selected previews decode up to 1600×1000;
card thumbnails use 640×420 (hexagons 480×420).

Dynamic selected cards wait 180 ms before starting AnimatedImage or QtMultimedia
video; only the selected card runs, and lock/sleep/paused suppress motion. The video
component is created lazily. Bead must retain at most one motion preview.
The source is the helper’s recursive catalogue of `cli.json` wallpaper.directory,
falling back to Pictures/Wallpapers (including Dynamic). The helper owns image/video
classification, posters, bounded thumbnails, import/pick, and wallpaper-picker preferences.

## Implementation and verification

UI/layout/filter/navigation belong to Luau on the kit. Nacre-specific contract calls
belong to the shell edge adapter. Catalogue leases and images retire with the panel.
Existing revision-only wallpaper.activate remains compatible; N0070 adds selection by id.
No replacement catalogue, ranking engine, or decoder is implemented in Bead.

The implementation is `shell/extras/wallpaper/main.luau`, with selection/filtering in
`extras/behavior.luau`; the extras factory acquires the catalogue/settings only on opening.
Super+W, `nacre mode wallpaper`, and the launcher's wallpaper route row select this surface.
Static and video entries activate through `wallpaper.activate {revision,id}`. Pointer clicks,
Left/Right, navigation buttons, Enter and wheel navigation share stable-ID Luau selection.
Only the latest selection waits behind an admitted call; revision-only background reads remain compatible.

Generic additions: normalized `clip_polygon` (3–32 vertices, even-odd image mask and matching
hit region), `image_fit="cover"`, and signed edge insets for clipped offscreen content.
The shell edge resolves only advertised thumbnail/preview handles; encoded assets have a
16 MiB byte budget, individual files at most 2 MiB and dimensions at most 1600×1000.
Closing clears this cache; closing-frame rasters retire with the existing panel worker after idle.
No new dependencies or duplicate implementations were added.

Remote workspace/all-target clippy with warnings denied and 13 wallpaper tests pass.
They cover real keyboard input, pointer navigation, revision-fenced calls, stable filtered
selection, cache bounds/close release and static/dynamic layouts at 1×/1.5×.
Full CI on the final protocol pin and paired captures at 2880×1800, scale 1.5 are pending.
Evidence belongs in `evidence/0100/`; missing measurements are not zeros.

## Known gaps

- Add is visibly disabled: the catalogue/selection contract does not offer file import.
- Candidate video/animated-image previews remain posters. The existing video renderer exposes
  output wallpaper surfaces, with no in-panel frame API; adding decoding is B0099's scope.
- Panel reveal uses the existing 300 ms bottom motion. Card travel, selected-backdrop crossfade
  and spotlight side-card opacity do not yet match QML. Hex captions lack QML's text outline.
- Wheel navigation uses each Wayland distance event; QML's accumulated 120-unit threshold is
  unavailable in the current event bridge. Catalogue file changes refresh on reopen/reconnect.
