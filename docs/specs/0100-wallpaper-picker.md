# B0100 — Wallpaper picker

Status: implementation committed; private desktop parity and memory verification in progress.
N0070 supplies the missing catalogue. Bead remains unpublished pending evidence.

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
The shell edge first requests metadata (`ids:[]`), then hydrates only IDs used by the
current Luau scene (at most 128 per call). Logical image handles carry a resource
generation so newly hydrated bytes trigger a fresh scene. Animated images join videos
in Dynamic. Missing/evicted advertised paths refresh on demand; a failed or still-missing
resource is attempted once per visible set, then retries after that set changes or reopening.
Encoded cache and scene attachments each have a 16 MiB byte budget, individual files
are at most 2 MiB and dimensions at most 1600×1000.
Closing clears this cache; closing-frame rasters retire with the existing panel worker after idle.
No product dependencies or duplicate implementations were added. The Arch CI container needs
`swaybg` for the video smoke fallback, and removes Sway’s packaged file capability inside
the container because Docker does not grant that capability.

Remote workspace/all-target clippy with warnings denied and 13 wallpaper tests pass.
They cover real keyboard input, pointer navigation, revision-fenced calls, stable filtered
selection, cache bounds/close release and static/dynamic layouts at 1×/1.5×.
The protocol pin is Nacre `34e81afd` (N0070 bounded hydration); both wallpaper transcripts are mirrored.
Full remote Arch CI passes at `9920e2a` after the final theme-token pass and rebase
onto B0102. This includes the video renderer smoke (12/12 checks),
private service and tray lifecycles, Luau/generated schemas and independent license graphs.
Paired captures at 2880×1800, scale 1.5 are pending. The lead clarified that B0104
owns the shared desktop slot; this continuation waits without a timeout and takes the
lock separately for the parity script and each memory arm. The harness also rejects
any host swap-in during an arm, alongside its existing swap-used and process SwapPss gates.
Transition sequences use grim process-spawn timestamps relative to the trigger; they are
image-capture proxies with scheduling uncertainty, not physical presentation timings.
The prepared fixture is ten images plus two short videos, covering every layout/media
kind, hover/pressed, search/empty and image/video activation; memory uses five alternating
same-host CPU control/candidate pairs at idle and after close+idle. This is not acceptance evidence.
Evidence belongs in `evidence/0100/`; missing measurements are not zeros.

Product-source delta against the rebased control is 1018 added / 65 removed lines,
excluding tests, generated schemas, documentation and the capture harness. N0070 adds
599 / removes 32 Rust product lines. No duplicated legacy implementation remains.

## Known gaps

- Add is visibly disabled: the catalogue/selection contract does not offer file import.
- Candidate video/animated-image previews remain posters. The existing video renderer exposes
  output wallpaper surfaces, with no in-panel frame API; adding decoding is B0099's scope.
- Panel reveal uses the existing 300 ms bottom motion. Card travel, selected-backdrop crossfade
  and spotlight side-card opacity do not yet match QML. Hex captions lack QML's text outline.
- Wheel navigation uses each Wayland distance event; QML's accumulated 120-unit threshold is
  unavailable in the current event bridge. Catalogue file changes refresh on reopen/reconnect.
