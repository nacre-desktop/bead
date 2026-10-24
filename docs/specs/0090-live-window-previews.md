# B0090 — Live window previews in the Overview

Status: **Review**. The capture primitive, the Luau tile and the lifecycle are implemented and
verified on Hyprland. **niri 26.04 does not offer per-window capture**, so on the owner's main
compositor the Overview keeps the window-icon tiles (the same as the QML shell, which turns
`windowCapture` off for niri). What is missing there is listed exactly below.

## Compositor research (2026-10-24, desktop, private nested sessions)

`wayland-info` against a nested compositor in a headless labwc parent:

| Global | niri 26.04 (8ed0da4) | Hyprland 0.56.2 |
| --- | --- | --- |
| `ext_foreign_toplevel_list_v1` | v1 | v1 |
| `ext_image_copy_capture_manager_v1` | **absent** | v1 |
| `ext_foreign_toplevel_image_capture_source_manager_v1` | **absent** | v1 |
| `ext_output_image_capture_source_manager_v1` | **absent** | v1 |
| `hyprland_toplevel_export_manager_v1` | absent | v2 |
| `zwlr_foreign_toplevel_manager_v1` | v3 | v3 |
| `zwlr_screencopy_manager_v1` (output only) | v3 | v3 |
| `zwp_linux_dmabuf_v1` | v3 | v5 |

- **QML.** `Overview.qml:113-135` uses Quickshell's `ScreencopyView`, running only while the launcher
  is visible and `livePreviews` is on, and only when the helper reports `windowCapture` (`hyprland.rs`
  true, `niri.rs` **false**). So the QML shell shows icons on niri too; "parity on niri" is the icon tile.
- **niri.** Only `wlr-screencopy` (whole outputs) exists. It cannot see a window that is on another
  workspace, scrolled out of view or covered by the Overview, and the compositor gives no window geometry.
  `niri msg action screenshot-window --id N --path P` (the helper reports `nativeWindowScreenshot`) renders
  one window by id, but it also puts the image on the clipboard and is a niri-private route, so a 3 fps
  loop over it is not acceptable. Not used.
- **Hyprland.** Everything the primitive needs; the QML route (`hyprland-toplevel-export`) is private
  to Hyprland, `ext-image-copy-capture` is the standard route and works for every compositor that adds it.

What niri would have to add (or the owner would have to accept a niri-specific route):
`ext_foreign_toplevel_image_capture_source_manager_v1` plus `ext_image_copy_capture_manager_v1` (shm
buffers are enough). Bead needs no change then: the same `CaptureHub` becomes `supported()` at start.

## The primitive (general, in `bead-wayland`)

`bead_wayland::CaptureHub` (`capture.rs`, exported with `CaptureLease`, `CaptureWindow`, `CapturePreview`):
"capture toplevel X into a thumbnail at most every N ms while leased".

- The product creates a hub and passes it in `Options::capture`. `supported()` is true once the
  compositor advertises `ext_image_copy_capture_manager_v1`, `ext_foreign_toplevel_image_capture_source_manager_v1`
  and `ext_foreign_toplevel_list_v1` (detected from the registry at start).
- `hub.lease(windows, interval, bound, wake)` returns a `CaptureLease`; `lease.set_windows(..)` re-lists;
  dropping the lease ends it. `hub.preview(key)` returns the newest `CapturePreview { path, width, height }`.
- Wayland thread only, and only while a lease exists: the toplevel list is bound, one capture session per
  paired window is opened, and one 50 ms tick runs. Without leases there is no list, session, timer or pool.
- **Rate limit.** At most one frame per window per `interval` (350 ms, QML's budget, ≤ 3 fps per tile);
  one frame in flight at a time (the most overdue window goes first), so several tiles cost one shm
  buffer. The buffer is a `SlotPool` freed at release.
- **Downscale.** The frame is box-averaged (at most 3×3 samples per output pixel) to fit `bound`
  (480×320 device px), keeping the aspect ratio, never upscaled, and written as a small PNG
  (`$XDG_RUNTIME_DIR/bead-capture-<pid>/w<session>-<seq>.png`, new name per frame, previous file removed).
  The renderer already loads images by path, so the CPU and GL renderers both work with no new primitive
  and no renderer change; if capture is unsupported or a frame fails, the tile has no `preview` and shows the icon.
- **Release.** Dropping the lease removes thumbnails no other lease wants, the directory when the last lease
  goes, and on the next tick destroys sessions, sources, the toplevel list and the buffer pool.
- **Window association (the open contract from B0077).** `ext_foreign_toplevel_handle_v1` carries
  `identifier`, `app_id` and `title`, not a compositor window id, and no compositor offers a portable id
  mapping. Bead pairs a compositor window (from the daemon's `compositor` topic: id, app id, title) with a
  toplevel only on an **exact app id + title match that is unique**; ambiguous or missing windows keep the
  icon (never a wrong picture). Known limit: two windows with identical app id and title (two fresh terminals)
  show icons until their titles differ. A daemon-side fix is possible later (the compositor backend could
  publish the foreign-toplevel `identifier`); not needed for this unit, so no Nacre change.
- Generation and output ownership: a preview is keyed by the daemon's window id and belongs to the
  lease that listed it; the Extras adapter re-lists on every `compositor` snapshot (filtered to the focused
  output when workspaces are per-output), so a window that moves to another output or closes loses its
  preview with the next snapshot.

## Luau binding and Overview tile

The binding is the model, not a new Luau global. The Extras adapter (`nacre-shell/src/extras.rs`) leases
only while `panel.open` is true **and** the Nacre setting `livePreviews` (from `settings.store.desktop`,
default on) is not false **and** the hub is supported, and adds `preview`, `preview_width` and
`preview_height` to each window row. `shell/extras/main.luau` (`preview_content`) draws a
`ui.Image{path=item.preview}` contain-fitted inside the preview box with the token `overview.preview_inset`,
else the existing icon. Closing the panel (or turning the setting off) drops the lease within the same
projection. `BEAD_CAPTURE=off` keeps the icon tiles (measurement control).

## Tests

- `bead-wayland` unit tests (6): lease release removes thumbnails and directory; a window shared by two
  leases survives one release and unwanted ones go; per-window rate limit (350 ms) with most-overdue-first
  and ready-only selection; thumbnail fit, aspect, no upscale, channel order, unsupported formats refused;
  unique-pairing rule.
- `nacre-shell` `extras::tests::previews_lease_only_while_open_and_with_the_setting...`: no lease while
  closed, one lease while open, no preview until a frame lands (icon fallback), the newest thumbnail attached,
  re-projection reuses the lease, closing releases lease and files.

## Evidence

RESULTS_PLACEHOLDER

## Known gaps

- niri: icons only (protocol absent, above).
- The thumbnail is not corner-clipped to the tile's 8 px radius (Image has no radius primitive).
- Identical app id + title windows show icons.
- `PREVIEW_BOUND` (480×320) and the 350 ms interval are constants in the adapter (`extras.rs`), not settings.
- No Hyprland QML pair: the QML shell was not run on Hyprland here (the owner's QML default on niri is the icon).

## Temporary duplicates

None added.
