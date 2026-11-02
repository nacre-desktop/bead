# B0216 — icon centring

Owner report: icon ink sits left of button hover outlines, especially in control centre.
Scope: icon-only controls and icon boxes; retain icon+label layouts. Sliders and scrolling are excluded.
Source baseline: d21f8065. Builds/tests run in the task's Ubuntu CI container; no laptop execution.

## Inventory

| Surface | Shared path / exceptions |
| --- | --- |
| Control centre, quick controls | `kit.IconButton`: `control/main.luau` header tools/remove and kit quick-control toggle/detail; labeled tiles retain their rows. |
| Bar status | `widgets/context.luau:status_glyph`: network, Bluetooth, sound, battery, notifications, calendar, power, updates; recording has icon+elapsed text. |
| Service tray menu | `tray/main.luau` back/close text-symbol buttons migrate to IconButton; menu rows retain labels. |
| Popouts | `status/network-body`, `sound/{main,body}`, `popouts/calendar` IconButtons; Bluetooth scan and battery profiles retain labels. |
| Dashboard | `dashboard/{media,today/page,today/summary-calendar,customize}.luau` transport/seek/calendar/wallpaper/reorder controls; labeled navigation retains rows. |
| Launcher | `ui/components/{results,app-grid}.luau` favorite SVG buttons; `widgets/launcher/widget.luau` favorite images/add/brand. |
| Session/power | Labeled power tiles and text session rows; device-action IconButtons in kit. |
| Notifications | IconButton expand/dismiss and widget dismiss. |
| OSD | Display-only glyph, no icon button. |
| Widgets app | IconButton plugin gear/remove; custom resize glyph; sidebar/gallery/drag labels retain layout. |
| Settings | `kit.luau` form reveal, +/-/save/cancel/reorder/remove/device actions/lock; labeled sidebar and wallpaper picker link. |
| AI/chat | `chat/main.luau` PillButton icon-only settings tab; labeled pin/tools/send/attach/screenshot/stop/latest/refresh/new. |
| Wallpaper/extras/colors/welcome | Centered navigation glyph row, IconButton refresh/mode controls; labeled close/choices. |
| Media | Dashboard transport and seek; today transport; sound controls above. |
| Title/pearl/windows | Title glyph in title row; workspace number/icon pearls; tray image buttons. |
| Prompt | Labeled choice rows, no icon-only control. |

All shell Material Symbols and IconButton call sites were scanned; image buttons were also inspected.
Icon+label containers, icon order, and text layout are outside the layout change.

## Cause and fix

IconButton inherited a padded, left-aligned text rectangle. Material Symbols advance
and baseline metrics also differ from visible ink. The renderer now measures the
unclipped icon raster and centres its ink before cropping to the glyph box.
Nearest-pixel ties resolve downward so layout rounding and raster rounding do not add.
Icon-only PillButton bypasses its extra padded row; labeled pills keep that row.
Bar optical widths remain a spacing policy; a centred row replaces the manual glyph offset.
Its full-width glyph box avoids clipping (an 8 px battery box lost 30 ink pixels at scale 1).
Custom media seek and launcher favorite image containers explicitly centre their children.
Tray-menu back/close now use IconButton instead of left-aligned ordinary text symbols.

## Validation

Rendered checks: six renderer glyphs, seven kit variants, and real control-centre,
bar status, media transport/seek and service tray navigation buttons at scales 1 and 1.5.
Every ink-centre assertion retains the requested 0.5 logical px tolerance.
Reviewed all 229 changed/new goldens in 21 before/after sheets, including hover states.
The bar pixel hash is deliberately updated; original bar geometry assertions remain.
Full Ubuntu CI results are recorded in the private unit report.
Desktop test host SSH timed out on 2026-11-02; paired QML/Bead nested captures unavailable.
No live desktop changes or deployment are authorized by this unit.
