# B0161 — wallpaper widgets and Widgets app reference

The six owner-approved boards are rendered at their authored CSS sizes using the supplied forest wallpaper. Desktop forms now use shared palette-led glass/display styling; clock, battery, controls and feeds have the board's visible styles. The Widgets detail page has a widget sidebar, real-aspect wallpaper/bar stage, side-by-side form panels, segmented Click/Hover trigger and developer fields under Advanced. Style tiles render the existing live widget packages without clipping.

Outfit is bundled under OFL and registered with Fontconfig inside the shell process. The hollow-glyph appearance came from eight hard offset text copies used as a halo around Outfit-200 glyphs. Those copies are removed; `TextShadow` now blurs glyph coverage in the CPU text pipeline, paints the dark offset mask first, and paints the solid glyph over it. The renderer golden samples a fully covered pixel inside a large Outfit digit. The analog tick face is a palette-themed SVG image cached by the renderer; only the hands change during clock animation. The Widgets app disables Niri border/focus chrome for its app ID.

| Gate | Evidence | Result |
| --- | --- | --- |
| Reference renders | `evidence/0161/reference/{Main,Clock,Battery,Controls,Feeds,App}.{html,png}` | Complete, Firefox headless on desktop; original CSS dimensions |
| Bead board captures | `evidence/0161/bead/{Main,Clock,Battery,Controls,Feeds,App}.png` | Complete at 2880×1800, scale 1.5, CPU; each accepted arm completed with zero OOM |
| QML parity | `evidence/0161/parity/` | No matching QML wallpaper widgets or Widgets editor exist in the Nacre source; unrelated shell screens are not parity evidence |
| Full CI | dev-2 Ubuntu 24.04 container, B0161 task id | Review-fix rerun pending on the final commit; prior pushed revision `e7eeeb01` passed |
| Memory / frame pacing | 2 MiB always-on unreclaimable budget; 60/120 Hz animation target | Review-fix measurements pending |

Current changes: shared palette glass/display helpers; frameless digital/minimal clock text; palette-themed cached analog tick image with animated hands; localized date parsing; bundled Outfit font; battery ring/card/line catalog; power action icons and account/uptime header; sound, Bluetooth, Wi-Fi, notifications, favorites dock and update card; app page layout and fixed 120×70 logical style tiles with scaled live miniatures and selected accent borders. Current capture service fixture reports no connected Bluetooth devices or Wi-Fi network, so those widgets show their available/offline state. Notification rows are not exposed by the private daemon source used in capture; the boards use private Discord/Nacre and Firefox notifications, which appear as compositor popups. Battery charging/remaining estimates, sound mute/output actions, Wi-Fi band and running-app indicators are not available through current bindings. The power header reads the existing `dashboard.host` data when available and labels missing uptime as unavailable. The sound buttons open the existing audio route; they do not directly mute or select an output. Device and updates data use existing bindings and may be empty.

No new build dependencies or renderer primitives were added. Hidden B0152 style IDs remain compatibility aliases; remove them 7 days after the replacement layout is trusted.

Capture safety recorded zero OOM events and zero host swap use for each accepted
Bead arm. Per-widget unreclaimable memory attribution, the 2 MiB feature budget,
and 60/120 Hz frame pacing remain unmeasured. The app capture confirms the rounded
window without Niri focus chrome, a single Click/Hover trigger, and a visible
Advanced section. The minimal clock thumbnail is scaled to fit its tile.
