# B0133 — Welcome fixes

Status: **Review candidate**; full remote integration CI and native page checks pass. Scope: Welcome logos, instant bundled demo,
window fit and shortcuts; Settings and Colors header artwork too.

The Welcome/Colors SVGs still contained QML `@PRIMARY@` tokens under ordinary
`.svg` filenames. Bead therefore rendered their base paths black. Their QML
full templates now use the existing B0056 themed-asset mapping; Settings uses
its full QML emblem rather than the generic compact shell. Old assets are deleted.

Welcome opens at 1100×800 logical pixels, with that minimum. Its thumbnails and
210-pixel miniature desktop leave room for all controls, role samples and fixed
navigation. The redundant Colors-app shortcut and trailing explanation are
removed to match QML; the demo retains its preview-only explanation.

## Instant demo

The four QML demo wallpapers have 512×288 thumbnails, 1280×720 posters and all
64 palettes baked by `tools/bake-welcome-demo.py`. The generator uses Nacre's
existing `colors preview` JSON-line interface with `compare:true`, once per
scene/mode, inside a temporary HOME. No engine implementation is copied. The
bundle records artwork sources, CC0 licenses, original checksums and helper hash.

Welcome decodes the eight images and loads the palette table before its first
frame, including a cold `nacre-shell --app welcome colors` deep link. Every scene/style/mode choice
is a cache lookup; the old asynchronous catalog/preview demo path and its wire
fixture are deleted. The Colors application still uses its published daemon
contracts unchanged. Demo defaults: Natural/dark, fixed noon for Tide; preview
choices never apply to the desktop. No new dependency or engine primitive.

## Shortcuts

Welcome now follows the published `shortcuts` topic, like Settings, replacing
the empty launcher menu route. Late topic updates wake the view; last-good rows
survive disconnect. Luau groups named compositor submaps, app/panel routes and
other desktop bindings, preserving source order inside each group. The contract
does not supply QML's five semantic categories; that remains a parity difference.
Large real shortcut catalogs intentionally scroll, as QML does.

## Validation and evidence

Ten focused remote tests pass, including all 64 cached choices with no daemon,
first-paint readiness, eight predecoded images, late shortcuts data and default
page overflow assertions. The old fallback colors golden and synthetic demo
fixture are replaced by a golden using the shipped artwork and baked palettes.
The complete current CI job passed on rebased `5ddf9ce8` (628 shell tests passed,
one ignored; all other workspace suites and workflow steps pass): [receipt](evidence/0133/ci.txt).
Private desktop captures cover the five tour pages, Help, Settings and Colors:
[QML-left / Bead-right gallery](evidence/0133/gallery.md), [capture receipt](evidence/0133/captures.json).
Every Welcome shot verifies a mapped window on the requested page at 1100×800,
scroll 0. Both arms pass containment, zero swap-in and zero OOM gates. Initial
fixed-delay shots were discarded; the harness now waits for mapped page state.
Welcome/Colors adapters and shell assets in captured `0085d5c6` are unchanged
in integrated `5ddf9ce8`.

## Observed parity limits

- QML's 1020×760 window is smaller, intentionally changed here. Its hero name is
  bold; Bead remains regular. Ready's wallpaper/style summary is absent in Bead.
- Apps retain Material glyphs versus QML's branded logos; Brain availability and
  some Help wording/links differ (B0059). Shortcut titles/keys and semantic groups
  differ between the dedicated topic and QML's legacy helper; the grouped fixture
  verifies richer rows without inventing producer metadata.
- QML reference `836df1a` still invokes `python3 wallpaper-media demo-start` in
  [NacreWelcomeApp.qml](https://github.com/nacre-desktop/nacre/blob/836df1a643eae6e2f54ec86967a83d2f27561d3d/nacre/shell/services/NacreWelcomeApp.qml#L129), although `NacreWallpapers.tool` is now `wallpaper-media`,
  a native helper subcommand. Its demo scenes are absent in the native reference.
  Bead's four bundled scenes and 64 palettes are verified separately. Nacre stays read-only.
- Welcome previews rather than applying desktop changes (B0064). Existing Settings
  form-control/scrollbar and Colors tab/rail/preview-layout gaps remain; this unit
  checks their header logos. Hover/pressed states, page-transition motion and
  physical presentation latency were not re-measured.

## Memory and lifecycle

The fixture counts **12,828,672 bytes (12.23 MiB)** of decoded demo image pixels,
plus one current-poster copy and normal rendering caches. This is an intentional
on-demand app cost to remove loading and selection stalls, freed when Welcome
closes. It is an allocation count, not process PSS/DRM. No always-on allocation
is added (always-on feature budget: 0 MiB). No live deployment or laptop-cap claim.
No temporary duplicate implementation remains.
