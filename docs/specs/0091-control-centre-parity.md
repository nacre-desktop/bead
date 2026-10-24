# B0091 — Control centre parity with QML

Status: **Review**. Owner rule: Bead must look and feel like QML. This unit fixes the differences B0083 found.

## What changed

| Difference (B0083) | Fix |
| --- | --- |
| Sliders had no fill/thumb (read-only ones were a bare track) | Kit `Slider` redrawn as `NacreControlSlider`: 6 px track (`surfaceContainerHighest`), fill, 8x16 handle, 6 px inset. Without `on_change` (or `enabled=false`) it shows the same drawing at 35 % and takes no input. `SliderTile` and the Sound page use it, so every slider benefits. The old circular thumb is gone. |
| Tiles/quick actions missing | Root cause: the Nacre registry's stored default (9 tiles) came back as "saved". `control_center.rs::saved()` now treats an earlier default set (the 9 registry tiles, or QML's earlier 11) as unedited while `defaultsRevision < 2`, exactly as `NacreControlPreferences` does, so the QML default set and order apply (tiles.luau already held them). `TileGrid` lets a short last row share the full width (QML `fillWidth`). Wallpaper opens Settings at Appearance (`cc.do open:settings:appearance`), Clipboard runs `nacre clipboard` (`app::command`); neither needed a dashboard view. Mic mute shows On/Off (`onoff`). |
| Volume label | Level tiles name the device (new `label` in each `cc.src` entry: audio node name, display name) with the tile label as fallback; the volume icon follows QML (`volume_off` only when muted, `volume_down` below 35 %). |
| Bluetooth subtitle truncated | Tile geometry follows `NacreControlTile` (icon at 14, text from 44, 8 px right margin, chevron at 12, 1 px outline, radius 24/20, 3 px between lines, medium-weight label); the name gets a second line when it would wrap. Wi-Fi/Bluetooth are link tiles: a click opens the page, as QML's `toggle()` does. |
| Power profile buttons | Icon centred (24 px), unselected icon muted but label `onSurface`, selected `onPrimaryContainer`; no dimming. |
| Sound captions | The two extra device captions are removed; sliders always draw. |
| Bar updates count | Real bug: the count text was 7.5 px per digit, clipping "130" to "13". Now 9.6 px per digit. (QML showing 0 in B0083 was its first check not finished.) |

Harness: both arms now start the private PipeWire (the daemon offers `audio.set_volume` only while `pipewire-0` exists, so Bead's sliders were read-only). Goldens regenerated and read; `session-*` goldens changed only through the shared tile frame (outline, radius).

## Evidence

`evidence/0091/*-before.png` / `*-after.png`: QML (left) | Bead (right), private nested niri 2880x1800 at 1.5 on the desktop, CPU renderer, one run per arm and state, before = origin/main `3ca6c8c` binary and shell, after = this branch; same harness and fixtures. `rmse.json` has whole-frame rmse (harness) and panel-only rmse (x 1900-2880, y 80-1300 px).

| State | frame before | frame after | panel before | panel after |
| --- | --- | --- | --- | --- |
| control home | 6.577 | 6.777 | 28.114 | 29.195 |
| audio detail | 6.084 | 5.404 | 25.722 | 20.973 |
| battery detail | 4.507 | 4.473 | 13.797 | 13.453 |
| popout-audio | 6.011 | 5.486 | 24.732 | 20.992 |
| popout-battery | 5.039 | 5.056 | 17.568 | 17.680 |

The home number rose because tiles that were missing now exist but sit 5-12 px away from QML's (below); rmse alone is not parity, the pairs are.

## Remaining differences (known gaps)

- Home layout offsets: status row ~6 px lower and slider rows ~12 px higher than QML; tile rows 5-7 px higher; the status row's battery text is larger. The notifications header has icon buttons, QML has DND/Clear pills.
- Dimmed tiles in the Bead arm are harness limits, not product gaps: no NetworkManager (Wi-Fi), no settings store (Light/Dark/Auto, Display settings), no user systemd bus (Stay awake), DND topic unavailable; VPN belongs to B0087, Display settings to B0089.
- Slider fill is flat `primary`; Bead has no gradient primitive (QML blends accent to secondary).
- Brightness names the output (`eDP-1`), QML its description.
- Toggle tiles keep a detail chevron zone; QML opens details by long-press/right-click (no secondary press in Bead).
- A Bead-saved list that equals an earlier default and has no `defaultsRevision` reads as unedited.
- The Sound/Power back button shows a focus ring after programmatic open; QML shows none.
- Input meter and streams: B0086.
