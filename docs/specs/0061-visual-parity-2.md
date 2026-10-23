# B0061 - Look-and-feel parity, slice 2: dashboard motion and tabs, bar, launcher Home

Status: **Review** (Claude worker, branch `task/b0061-parity-2-20261023`, pushed to main in slices). Owner 2026-10-23: "Bead must look correct on all fronts."
Continues B0056 (harness `spikes/parity-compare.py`, ranked leftovers). Everything user-visible is Luau/tokens; the only engine change is one general motion flag.

## Fixed in this slice

| Owner / ranked item | QML behaviour (measured from source and gallery) | Bead change |
| --- | --- | --- |
| 1 Dashboard opens "from the top over the bar" | `NacreDashboardPanel`: `presentedHeight` 0 to 650, `NumberAnimation` 360 ms `Easing.InOutCubic`, no fade; the panel body is opaque while it grows out of the bar contour | Panel motion `duration=.36`, `transition="ease-in-out"` (cubic in-out, already an engine curve) and the new general `fade=false` motion property (`bead_render::Motion::fade`, default true; the scene stays opaque while it moves instead of fading with progress). The cold-start fallback in `dashboard.rs` carries the same values. |
| 1 Dashboard tab strip | `NacreDashboardNavigation`: icon (14 pt) and title (10 pt) centred at y 6 in slots of `width/n`, accent icon and ink title when selected, an 8 px gradient pearl with a 12 px halo under the selected slot; no pill | `shell/dashboard/main.luau`: same geometry, pearl and halo, no pill, hover wash on the others. Goldens `dashboard-*-1(.5).png` regenerated. |
| 2 Top-left opener | The QML header has no button; the dashboard opens from the title band and the frame lips | Already so since B0056 (button removed, B0047 lips); unchanged. Hover itself still cannot be driven (see gaps). |
| 3 Launcher Home | Home title and `Favorites` line, hint text when there are no favorites, five prefix hint chips in the search field while the query is empty (24 px, radius 6, 8.5 pt, `onSurface` at .045) | `shell/ui/{main,behavior}.luau`: `launcher.home`, `launcher.hints`, `launcher.empty_text`, chips; i18n ids `bead.launcher.hint-*`, `bead.launcher.home`, `bead.launcher.home-empty`. Golden `launcher-rows-grid.png` regenerated. |
| 6 Bar | notification bell with the retained count (min 999), workspace pearl with enamel, 22/20/18 px sizes, the "Desktop" title with no focused window | `shell/bar/{main,behavior}.luau`; `notice_service::publish` sends `bar.notifications.count` to every driven bar session (only on change, and once when a bar attaches). The bell opens the `control` view. Pearl: base accent, a `inverseSurface` wash, a sheen over the upper half and a specular pill (kit tokens only, no literal colours); number ink is `onPrimary`. i18n `bead.bar.desktop-title`, `bead.bar.open-notifications`. |
| 7 Harness | easing numbers | `spikes/parity-compare.py`: every transition now reports the times at which a frame sequence passes 10/50/90 % progress (progress = 1 - distance to the final frame / distance first-to-final), shown beside first-change..settled. |

Mirror for Nacre's catalog (lead): `python3 tools/i18n-sync.py NACRE --write` for the 8 new `bead.*` ids (`bead.bar.desktop-title`, `bead.bar.open-notifications`, `bead.launcher.hint-{actions,ai,brain,calculate,files}`, `bead.launcher.home`, `bead.launcher.home-empty`).

## Framework notes

- One general primitive: panel motion `fade` (document in ADR 0004's list: "panel motion: edge, duration, curve, slide_distance, fade"). No Nacre-specific knowledge in engine crates; the bar count rides an ordinary store key.
- Luau pitfall found: a table constructor `f{a, cond and b or nil, c}` drops `c` (the array border stops at the nil). Children that may be absent are collected into a list and unpacked last.

## Numbers

The gallery is `~/.local/share/nacre/ai/lead/parity-gallery/latest/index.html` (laptop). Full-image rmse barely moves because the differences are small areas; before/after of the touched areas is in the gallery crops. rmse % (B0056 final -> this slice): idle 3.862 -> 3.887, dashboard-0 7.803 -> 7.742, launcher 4.495 -> 4.593 (the QML Home has the Quick controls column that is not ported yet, the chips alone add differing pixels), everything else within 0.1.

## Known gaps, ranked (what is left)

1. Control centre: full-height right dock, settings gear in the header, tile sub-labels (state text under the label), appearance as a full-width segmented control and the notification list with DND/Clear (needs the notice list in the control session).
2. Status popouts (Wi-Fi, Bluetooth, battery, audio): geometry and content (Scan and a toggle in Wi-Fi, widths, dock position).
3. Launcher Home Quick controls column (do not disturb, night light, stay awake, update count): needs launcher-provider values for the toggles.
4. Session/power: the QML opens a Power page in the control sidebar; Bead keeps the compact top-right popout (`nacre session` / Alt+X route is unchanged).
5. Dashboard: content density of the Today page, tab 4+ (not configured), the real grow-from-bar reveal (content fixed while the body grows) - the panel still slides, now opaque, 360 ms in-out cubic; the visible difference is that the tab strip travels with the body.
6. Bar: updates count is a fixed `0` text (QML reads real package state), pearl enamel is a layered approximation of the QML gradient (no gradient fill primitive), notification bell has no popout, only the control centre.
7. Harness: pointer injection (hover/pressed states, lip hover) and brightness/keyboard OSD states are not done; no virtual-pointer tool exists in the headless labwc parent.

## Temporary duplicates

None.

## Checks

CI-equivalent on dev-2 in the Arch container (fmt, clippy -D warnings, service boundary, `cargo test --workspace`, memory policy tests, feature checks, Luau schema and `check-luau.py`, spike unit tests, packaging tests): all green at 7e41057 (cargo-deny not run: known advisory handled elsewhere). Relative memory and typing gates: no new always-on allocation (one `u64` and one store key per bar; motion flag is a bool); the B0044 typing lanes and the control-build memory comparison were not re-run in this slice (no hot-path change), to be run with the next capture window.
