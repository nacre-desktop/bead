# B0161 — wallpaper widgets and Widgets app reference

The six owner-approved boards are rendered at their authored CSS sizes using the supplied forest wallpaper. Desktop forms now use shared palette-led glass/display styling; clock, battery, controls and feeds have the board's visible styles. The Widgets detail page has a widget sidebar, real-aspect wallpaper/bar stage, side-by-side form panels, segmented Click/Hover trigger and developer fields under Advanced. Style tiles render the existing live widget packages without clipping.

Outfit is bundled under OFL and registered with Fontconfig inside the shell process. The hollow-glyph appearance came from eight hard offset text copies used as a halo around Outfit-200 glyphs. Those copies are removed; `TextShadow` now blurs glyph coverage in the CPU text pipeline, paints the dark offset mask first, and paints the solid glyph over it. The `outfit_thin_digit_has_solid_ink_inside_the_stem` golden samples a fully covered pixel inside a large Outfit digit, and `glyph_text_shadow_is_soft_and_stays_behind_solid_ink` checks the blurred edge. The analog tick face is a palette-themed SVG image cached by the renderer; only the hands change during clock animation. The Widgets app disables Niri border/focus chrome for its app ID.

| Gate | Evidence | Result |
| --- | --- | --- |
| Reference renders | `evidence/0161/reference/{Main,Clock,Battery,Controls,Feeds,App}.{html,png}` | Complete, Firefox headless on desktop; original CSS dimensions |
| Bead board captures | `evidence/0161/bead/{Main,Clock,Battery,Controls,Feeds,App}.png` | Complete at 2880×1800, scale 1.5, CPU; all arms completed with zero OOM and zero host swap |
| Reference pairs | `evidence/0161/comparison/{Main,Clock,Battery,Controls,Feeds,App}.png` | Complete; each sheet places the HTML reference render beside its Bead capture |
| QML parity | `evidence/0161/parity/` | No matching QML wallpaper widgets or Widgets editor exist in Nacre; parity cannot be run against an equivalent surface |
| Full CI | Ubuntu 24.04 CI container on dev-2 | Pass on the final review-fix source snapshot; exact commit and command are in `.lead/report.md` |
| Memory | CPU backend, one sample per fixture, 2880×1800/1.5 | Empty shell: 48.400 MiB unreclaimable. Active-widget deltas vs Empty: Clock +1.936; Battery +4.934; Power +4.494; Sound +7.852; Bluetooth +4.150; Wi-Fi +6.713; Notifications +3.518; Launcher +5.633; Updates +5.039 MiB. Clean-file PSS: 56.294–56.885 MiB (informational). The ≤2 MiB always-on delta versus a same-host control is not isolated and remains open. |
| Analog pacing | CPU backend, five windows per phase, synthetic parent refresh at 60/120 Hz | 60 Hz: commit p50 178.06–178.22 ms, p95/p99 209.17–209.36 ms, estimated drop rate 91.2%; render p99 31.2 ms. 120 Hz: p50 178.35–178.46 ms, p95/p99 209.40–209.46 ms, estimated drop rate 95.4%; render p99 31.2 ms. `refresh_ns=0`, so there is no usable physical presentation cadence. Smoothness gate remains open. |

Current changes: shared palette glass/display helpers; frameless digital/minimal clock text; palette-themed cached analog tick image with animated hands; localized date parsing; bundled Outfit font; battery ring/card/line catalog; power action icons and account/uptime header; sound, Bluetooth, Wi-Fi, notifications, favorites dock and update card; app page layout and fixed 120×70 logical style tiles with scaled live miniatures and selected accent borders. Current capture service fixture reports no connected Bluetooth devices or Wi-Fi network, so those widgets show their available/offline state. Notification rows are not exposed by the private daemon source used in capture; the boards use private Discord/Nacre and Firefox notifications, which appear as compositor popups. Battery charging/remaining estimates, sound mute/output actions, Wi-Fi band and running-app indicators are not available through current bindings. The power header reads the existing `dashboard.host` data when available and labels missing uptime as unavailable. The sound buttons open the existing audio route; they do not directly mute or select an output. Device and updates data use existing bindings and may be empty.

No new build dependencies or renderer primitives were added. Hidden B0152 style IDs remain compatibility aliases; remove them 7 days after the replacement layout is trusted.

All six board capture-safety records show zero OOM events and zero host swap use.
The app capture confirms a rounded window without Niri focus chrome, a single
Click/Hover trigger, visible Advanced fields, and style tiles with no overlap or
clipping. Its 926×1108 logical default window is smaller than the near-full-screen
reference; app window-size parity remains open. The always-on delta versus a
same-host control was not isolated, and analog commit cadence fails the 60/120 Hz
target. Pacing `refresh_ns` is zero, so it does not measure physical scanout.
