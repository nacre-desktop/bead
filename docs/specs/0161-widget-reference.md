# B0161 — wallpaper widgets and Widgets app reference

The six owner-approved boards are rendered at their authored CSS sizes using the supplied forest wallpaper. Desktop forms now use shared palette-led glass/display styling; clock, battery, controls and feeds have the board's visible styles. The Widgets detail page has a widget sidebar, real-aspect wallpaper/bar stage, side-by-side form panels, segmented Click/Hover trigger and developer fields under Advanced. Style tiles render the existing live widget packages without clipping.

The framework currently has no text-shadow primitive and this Bead checkout exposes no charging estimate or remaining-time battery data. Outfit is not installed on the desktop test machine (`fc-match Outfit` resolves to Noto Sans); the Bead renderer uses that Fontconfig fallback. No fabricated service values are added.

| Gate | Evidence | Result |
| --- | --- | --- |
| Reference renders | `evidence/0161/reference/{Main,Clock,Battery,Controls,Feeds,App}.{html,png}` | Complete, Firefox headless on desktop; original CSS dimensions |
| Bead board captures | `evidence/0161/bead/{Main,Clock,Battery,Controls,Feeds,App}.png` | Pending |
| Paired QML parity | `evidence/0161/parity/` | Pending |
| Full CI | dev-2 Ubuntu 24.04 container, B0161 task id | Running |
| Memory / frame pacing | 2 MiB always-on unreclaimable budget; 60/120 Hz animation target | Not measured |

Current changes: shared palette glass/display helpers; clock date/minimal composition and animated polyline hands; battery ring/card/line catalog; power, sound, Bluetooth, Wi-Fi, notifications, favorites dock and update card; app page layout and unclipped miniatures. The renderer has no soft text shadow or hover-dismiss event. Battery charging/remaining estimates, power account/uptime, sound mute/output actions, Wi-Fi band and running-app indicators are not available through current bindings. The sound buttons open the existing audio route; they do not directly mute or select an output. Device and updates data use existing bindings and may be empty.

No new build dependencies or renderer primitives were added. Hidden B0152 style IDs remain compatibility aliases; remove them 7 days after the replacement layout is trusted.
