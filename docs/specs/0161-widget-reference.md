# B0161 — wallpaper widgets and Widgets app reference

The six owner-approved boards are rendered at their authored CSS sizes using the supplied forest wallpaper. Desktop forms now use shared palette-led glass/display styling; clock, battery, controls and feeds have the board's visible styles. The Widgets detail page has a widget sidebar, real-aspect wallpaper/bar stage, side-by-side form panels, segmented Click/Hover trigger and developer fields under Advanced. Style tiles render the existing live widget packages without clipping.

The framework currently has no text-shadow primitive and this Bead checkout exposes no charging estimate or remaining-time battery data. Outfit is not installed on the desktop test machine (`fc-match Outfit` resolves to Noto Sans); the Bead renderer uses that Fontconfig fallback. No fabricated service values are added.

| Gate | Evidence | Result |
| --- | --- | --- |
| Reference renders | `evidence/0161/reference/{Main,Clock,Battery,Controls,Feeds,App}.{html,png}` | Complete, Firefox headless on desktop; original CSS dimensions |
| Bead board captures | `evidence/0161/bead/{Main,Clock,Battery,Controls,Feeds,App}.png` | Not completed; only an invalid Main attempt existed and was discarded |
| Paired QML parity | `evidence/0161/parity/` | Not completed |
| Full CI | dev-2 Ubuntu 24.04 container, B0161 task id | Passed at 07cc5904 (763 passed, 0 failed, 2 ignored); final fc0eee0a rerun unverified |
| Memory / frame pacing | 2 MiB always-on unreclaimable budget; 60/120 Hz animation target | Not measured |

Current changes: shared palette glass/display helpers; clock date/minimal composition and animated polyline hands; battery ring/card/line catalog; power, sound, Bluetooth, Wi-Fi, notifications, favorites dock and update card; app page layout and unclipped miniatures. The renderer has no soft text shadow or hover-dismiss event. Battery charging/remaining estimates, power account/uptime, sound mute/output actions, Wi-Fi band and running-app indicators are not available through current bindings. The sound buttons open the existing audio route; they do not directly mute or select an output. Device and updates data use existing bindings and may be empty.

No new build dependencies or renderer primitives were added. Hidden B0152 style IDs remain compatibility aliases; remove them 7 days after the replacement layout is trusted.

Validation stopped when SSH connectivity to both dev-2 and the desktop test
machine was lost after launching final CI. No CI result for fc0eee0a, full-screen
Bead capture set, or QML parity is claimed. The implementation remains unpushed
until those required checks can be completed.
