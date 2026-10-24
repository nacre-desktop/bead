# B0094 - kit polish for QML parity

Status: **Review** (Claude worker, branch `task/b0094-kit-polish-20261024`). Closes the gaps B0089, B0091 and B0093 listed. Nacre
side: one i18n mirror commit (`tools/i18n-sync.py --write`, which also carries the 8 `bead.prompt.*` ids of B0093).

## What changed

| Gap | Fix |
| --- | --- |
| No drop-down (Displays scale/resolution were chips) | `kit.Select` / `kit.SelectRow`: field (40 px, radius 8, raised fill, 1 px outline, accent border while open, `expand_more` at the right) and, while `open`, a list card under it (radius 16, 1 px outline, 8 px padding, 40 px rows, chosen row in the accent, hover accent at 15 %, 288 px tall at most, then it scrolls; a filter line above it for long lists, 352 px wide then). Settings: new row kind `select`; used for Displays Scale and Resolution (QML `NacreDisplaysPage`; the mode list is searchable above 8 entries, matching case-insensitively on the typed text) and for every registry enum with more than 4 options (QML `SettingsRow`: up to 4 is the segmented row). Keyboard: Tab / Up / Down move over the options, Enter chooses, Esc closes (`reload.rs`), a choice or any other settings action closes the list. |
| Enum choices sent a bare `id\|n` (never matched `set:`) | Found on the way: option ids of `choice` rows now carry `set:`; `enum_rows_are_segments_to_four_options_and_a_drop_down_beyond` covers it. |
| `kit.ApplyBar` buttons collapsed | The buttons keep their width (`shrink=0`, 96 x 32 pills, Apply in `primary`), the sentence clips. Golden `settings-kit-1`. |
| Control centre header | Status row: QML geometry (14 px from the top, 3 px between the parts, glyph 15 pt, percentage 10 pt medium, estimate 9 pt), 32 px round tool buttons with no fill until hovered, `selected` fills (QML `NacreControlButton`). Page headers are 36 tall with a medium 18 pt title. The page starts 64 px down (14 + 32 + 18). |
| Notifications header: icon buttons | The full-sidebar QML history toolbar: title (14 pt), a **DND** pill (bell glyph, `primary` while Do Not Disturb is on) and a **Clear** pill (dimmed without entries), 10 px apart, entries 12 px under it; no count and no "view all" chevron (QML has none there; the Notifications tile opens the page). The page has the same pills, no title, and no extra DND switch. |
| Level rows 5-12 px off | `kit.SliderTile` redrawn from `NacreControlLevels` numbers (56 x radius 18, caption at 46/7, icon button at 8/17, slider at 42/23, value 40 from the right, chevron 4 from it); the appearance control is QML's 44 px pill with 4 px inside and 2 px between the segments; the home column spaces are 18 (status row, tiles, notifications) and 8 inside the tile grid. |
| Header battery text larger | 10 pt medium / 9 pt, as QML. |
| Back button focus ring on Sound/Power | The control-centre tool buttons set `focus_ring=false`; the host still focuses the back button of a docked page (keyboard keeps working), QML only draws a fill when the keyboard is used. |
| Slider fill flat | `kit.Fill` (also used by `kit.Slider`): accent into `secondary`, laid as steps of alpha over a flat `primary` bar (no renderer gradient, no seams: every step covers everything to its right). Round ends. `tokens.gradients` comes from the `interfaceGradients` setting (`Prefs::interface_gradients` -> `TextStyle::solid` -> `text_style::gradients()`; a change reloads the views like a font change). Off, or dimmed (35 %, no `on_change`), or shorter than 16 px, it is the flat `primary` bar. |
| Brightness named the output | `brightness.level` takes the label from the `displays` topic ("Make Model", leased by the panel now), falling back to the output name. |
| i18n | Two new ids (`bead.panel.control-pill-dnd`, `-clear`); two unused ones removed (`control-notices-all`, `-clear`). Nacre mirror synced. |

## Evidence

`docs/specs/evidence/0094/` (QML left, Bead right; private nested niri 2880x1800 at 1.5 on the desktop, CPU renderer, `--fake-nm`, one run per arm;
before = Bead `892b62c` build, after = this branch before the B0086 rebase; the Sound page of both arms predates B0086). Harness rmse (percent): control home
6.84 -> 5.79, Sound page 5.40 -> 5.28, Power page 4.47 -> 4.05, Displays 7.87 -> 7.72. Region rmse (0-255, `rmse.json`):

| Region | before | after |
| --- | --- | --- |
| control home, whole frame | 17.43 | 14.75 |
| control home, panel | 26.01 | 20.69 |
| notifications header | 29.64 | 24.43 |
| slider close-up | 30.65 | 12.63 |
| Displays page, whole frame | 20.06 | 19.70 |
| Displays page, window | 17.52 | 16.88 |

Not compared with live QML: the open drop-down (QML needs a pointer click; the goldens `settings-select-open-1` and `-search-1` show Bead's), the
brightness description (the harness has no backlight device; the "Make Model" form follows QML's `description`/`name` chain by reading), the
notification list (empty in both).

## Known gaps

- The list opens in the layout (rows below move down); QML overlays it. There is no click-outside close (Esc, a choice, or the field close it).
- Pill and percentage widths are estimated from character classes (the layout cannot measure text), so the DND pill is ~8 px wider than QML's.
- Bluetooth tile detail wraps to two lines on an estimate; QML measures. Two-line tiles may sit a few px off.
- Tile order of unavailable tiles in the harness (no NetworkManager etc.) is the harness' limit, as in B0091.
- The Time zone picker and the font pickers stay a filtered list (QML: select); not touched.

## Temporary duplicates

None.
