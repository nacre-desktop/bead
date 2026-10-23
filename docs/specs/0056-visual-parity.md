# B0056 - Look-and-feel parity with the QML shell: side-by-side harness and first fixes

Status: **Review** (Claude worker, branch `task/b0056-visual-parity-20261023`). Owner 2026-10-23: "things should look and feel the same when it's done."
Goldens that match themselves are not parity; this unit adds the comparison and fixes what it showed first.

## Harness: `spikes/parity-compare.py` (slice 1)

Runs on the owner's desktop (never the laptop), under `flock ~/nacre-lead/desktop-heavy.lock`:

```
python3 spikes/parity-compare.py --nacre NACRE_TREE --bead-bin DIR --bead-shell DIR --out GALLERY [--states a,b] [--arms qml,bead] [--baseline OLD/results.json]
```

- `--nacre`: a Nacre `git archive` (or checkout) of origin/main holding `nacre/shell` and `nacre/shell-tools`; `--bead-bin`: built `nacre-shell` and `nacre-helper` (the daemon, same Nacre revision); `--bead-shell`: Bead's `shell/` bundle for that binary. `spikes/parity-build.sh HOST TASK NACRE ORIENT` builds both in an Arch container (fails on any cargo error; `REBUILD_NACRE=1` after Nacre moved).
- One arm at a time, each in its own private world: a headless labwc parent, a nested niri at 2880x1800 scale 1.5 (the laptop geometry), a private D-Bus session, temp HOME/XDG, and the same standalone `nacre-helper daemon` with the same copied presentation record (wallpaper and palette) and no settings. The QML arm is the real `quickshell --path nacre/shell/shell.qml` with `NACRE_DAEMON_CLIENT=1`; Bead is started with the flags `bead-shell start` uses plus `--notices`.
- States are driven through the daemon's `ui.dispatch` (the vocabulary both shells register), plus per-arm steps where the routes differ (fake `wpctl`/`pactl` so the daemon's OSD sees volume changes; Bead's own command socket for views the daemon vocabulary lacks; app sockets to close Settings).
- Output `index.html` (QML | Bead | difference x4 per state, numeric rmse/bar-rmse/changed-pixel share, before/after with `--baseline`), `results.json`, full-size JPEGs, transition strips (grim region frames about every 90 ms with timestamps; first change and settled time per arm). Copy the directory to the laptop for viewing: `~/.local/share/nacre/ai/lead/parity-gallery/<name>/index.html` (baseline = before any fix).
- The clock and the updates count differ between arms by nature (different minutes; the QML reads the real package state), so every state carries a small constant rmse (bar rmse about 6.5%).

## What the first gallery found and what is fixed

Owner items: (1) bar pill spacing, (2) launcher from the top, (3) panels not the frame colour/contour, (4) logo colour, (5) frame not cut around the AI sidebar.

| Item | Cause | Fix |
| --- | --- | --- |
| 3 panel/frame colour | The frame tiles and every joined panel (`placement::Chrome`) were drawn with the bundle's static palette (`12120f`) while text and fills followed the wallpaper palette (`091412`). Even idle, the frame and the bar were different colours. | `placement::set_live_palette` + `ShellControl::palette_changed` (called when the committed presentation palette changes, bumps the decoration variant); the chrome reads the live colours. Panel apps (sidebar, Settings) are started with a palette file of the live presentation (`app::live_palette_file`). |
| 2 launcher | `launcher_panel.rs` and `shell/ui/main.luau` attached it to the top edge; QML `NacreLauncher` is bottom-attached (centered, joined to the bottom frame). | Edge `bottom` in the positioner, chrome, motion and Luau panel. launcher rmse 34.2 to 8.1. |
| 4 logo | `shell/bar/assets/nacre-logo.svg` was the QML full template with the colours of one old palette baked in. The QML `BrandLogo` fills `@PRIMARY@` etc. from the live palette with a contrast lift. | General palette-templated SVG: an image named `*.themed.svg` has `@role@` tokens (a palette role) and `@role+@` (lifted to 3.4:1 on the surface), filled at materialize time (`bead-ui/src/themed.rs`, unit tests). The bar logo is the compact QML template with tokens. |
| 1 pill spacing | QML `NacreStatusIcons` (grouped, horizontal): cells are glyph 24 + 3 px padding each side, 1 px between cells, detail text 4 px after the glyph; updates/clock holders add 6; power 30 wide. Bead used 24 px icons, 2 px gaps and an unrelated text. | Same numbers in `shell/bar/main.luau`; the battery value gets a width binding (`bead.desktop.battery.value.width`, empty when no battery as in the QML). The dashboard button that the QML header does not have is removed (the title band and lips open it). |
| 5 sidebar frame | B0052 moved the sidebar into its own process; the shell's frame tiles could not know a panel in another process was open. | `placement::set_external` / `External`: the host asks the app's `bead.panel state` (surface size, 32 px shadow margin each side) once it is open and describes the body to the frame (attachment edge + size); tiles leave that rectangle out, and the description is dropped when the app process exits. sidebar rmse 27.0 to 5.1. |
| harness bug | `spikes/foundation-preview.py` waited on the shell and daemon only; after the nested niri quit the preview (and `nacre-shell`, `nacre-helper`) ran on. | The wait loop probes the nested compositor's Wayland socket (`display_alive`, test `spikes/tests/test_foundation_preview.py`, in CI) and tears down shell, daemon and the private bus when it is gone. |
| B0057 control centre | Tiles of unavailable sources were dropped (QML dims them at alpha .4); the recorder was a half tile (QML third). | Unavailable default tiles are drawn under a 60 % surface veil; `record` is a third tile. control-center rmse 18.8 to 7.7. B0057 gaps 1 (appearance as a full-width segmented control), 4 (monitor of the panel's screen) and 5 (radius/typography) stay open. |

## Numbers (rmse % full scale, QML against Bead; baseline = before any fix, then this unit; same session geometry)

| State | Baseline | Now |
| --- | --- | --- |
| idle | 3.921 | 3.86 |
| launcher | 34.178 | 8.048 |
| launcher-typed | 34.235 | 5.1 |
| launcher-wallpapers | 34.646 | 8.473 |
| menu-power | 34.403 | 5.435 |
| dashboard-0 | 7.814 | 7.756 |
| dashboard-1 | 7.112 | 7.11 |
| dashboard-2 | 6.092 | 6.028 |
| dashboard-3 | 7.643 | 7.455 |
| dashboard-4 | 7.814 | 7.756 |
| popout-calendar | 4.781 | 4.704 |
| popout-network | 17.802 | 17.824 |
| popout-bluetooth | 17.677 | 17.754 |
| popout-battery | 14.277 | 14.337 |
| popout-audio | 19.123 | 19.181 |
| control-center | 18.791 | 7.718 |
| control-network | 12.956 | 12.927 |
| notification | 7.459 | 7.425 |
| osd-volume | 5.168 | 5.124 |
| osd-microphone | 5.163 | 5.119 |
| session | 22.466 | 18.245 |
| settings | 10.731 | 10.193 |
| sidebar | 26.972 | 5.131 |
| pearl-detached | 39.435 | 10.453 |

## Owner follow-ups (2026-10-23, second look)

| Owner item | Result | Named state in the gallery |
| --- | --- | --- |
| Top panel slides over the bar; the QML bar grows into the panel | Open. In the QML the dashboard's tab strip lives in the bar band and the panel grows from that contour; Bead draws the tabs inside the panel body and the panel surface slides over the bar layer. Needs the bar's centre band to host the tab strip while the dashboard is open and a reveal clipped to the area below the bar. Listed as gap 1. | `dashboard-0` (open and close strips) |
| Top-left opener is a button, the QML has a hover lip | The bar button is removed (the QML header has none); the title band and the B0047 lips open the dashboard. Hover itself cannot be driven yet (gap 8). | `idle` |
| One workspace pearl only | Same in the QML in this session: both read the same `workspaceCount` setting and the nested niri has one workspace; the sidebar state shows both shells with two. No difference found. | `idle`, `sidebar` |
| Top-right icon spacing | Measured from the captures (full-resolution pixel runs): cell pitch is 46-47 px in both (30 + 1 logical, 1.5 scale), the right edge is identical; the remaining width difference is the QML notification bell cell (gap 5). | `idle` (bar crop) |
| Launcher opens on All apps | Fixed: no query and no route opens on Home (`key_routes::Launch::model`), a typed query searches everything. The Home page still lacks the QML Quick controls column (gap 2). | `launcher`, `launcher-typed` |
| Session menu opens inside the launcher | Fixed: `nacre session` (and the preview's Alt+X) toggles the top-right session view; `nacre menu power` still opens the launcher's Power route, as in the QML. The QML opens the control sidebar on its Power page; Bead's view is the compact top-right popout. | `session`, `menu-power` |

## Known gaps, ranked (what the gallery still shows)

1. Dashboard/top panel: tab strip in the bar band and grow-from-the-bar reveal (above); content density differs.
2. Launcher Home: QML Quick controls column (do not disturb, night light, stay awake, update count) and footer hint chips (`> Actions`, `= Calculate`, ...).
3. Control centre: QML is a full-height right dock (424 px: battery line, edit, settings, power, tiles, appearance as a segmented control, notification list); Bead's panel is content-high with no settings button or notification list.
4. Status popouts (Wi-Fi, Bluetooth, Battery, Audio): geometry and content differ (QML Wi-Fi has Scan and a toggle; widths; Bead docks to the right edge); colour and the bar join match now.
5. Bar: workspace pearl is a 3D pearl in the QML (halo, gradient), flat here; no notification bell with count (the bar has no notifications data path; the notice view has `notifications.history_count`); active-window title text ("Desktop" with no window) is drawn by the QML, Bead shows the glyph only; audio glyph follows data.
6. OSD (volume, microphone) and notification popup positions/sizes were not re-measured; brightness and keyboard OSD cannot be triggered in the private session (no backlight).
7. Animations: transitions are 0.4-0.5 s in both (frame strips); launcher direction now matches; easing and exact durations are not yet compared numerically (the strips sample about every 90 ms).
8. Frame lips hover, pressed/hover states and dashboard tab 4+ (not configured in Bead) are not driven yet: the headless labwc parent has no virtual-pointer tool.

## Temporary duplicates

None. The harness keeps no copy of an older harness.

## Checks

See the report for the CI-equivalent run on dev-2; unit tests `themed::tests` (token fill, lift reaches 3.4:1, bad tokens are errors), `spikes/tests/test_foundation_preview.py`; goldens regenerated and eyeballed where the bar, launcher, control tiles and frame changed. Memory: no new always-on allocation (the palette handoff is one small map; the external description lives only while the sidebar process runs; the themed asset is replaced per palette); relative gate not re-run in this unit.
