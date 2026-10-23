# B0067 - Look-and-feel parity, slice 4: motion measurement, control dock, Power page, Settings window

Status: **Review** (Claude worker, branch `task/b0067-parity-4-20261023`). Owner 2026-10-23: "Bead must look correct on all fronts." Continues B0066 (its missing queue row is added with this one). Lead stop: 00:30; what landed is below, the rest is ranked at the end.

## 0. Motion measurement (harness)

`spikes/parity-compare.py` now captures transitions with **overlapped `grim` processes started every `--seq-interval` ms (default 25)** instead of one grim after another (~100 ms each): 40 frames per second on a region, stamps are spawn time plus half of the fast-quartile capture latency, the same for both arms. `results.json` gains `edge`: per frame `[ms, bottom px, right px]` of the pixels that differ from the first frame, so a growing panel shows direction, speed and easing as numbers (the gallery prints them under each strip). The trigger now runs in a thread beside the capture: a state opened by several dispatches (toggle, then tab) took longer than a 360 ms transition and the old sequential harness hid it (B0066 could not see its `reveal` for this reason). New state `dashboard-warm` measures the open after one open/close.

Findings, dashboard open (QML first, same output, 2880x1800 at 1.5, region `300,0 1320x760`, `--seq-scale 0.5`):

| | first change | bottom edge px (0.5 scale) over time | reading |
| --- | --- | --- | --- |
| QML | ~150 ms | 27 (150) 42 (200) 91 (250) 118 (276) 234 (301) 313 (351) 344 (426) 350 (451) | grows downwards from the bar, ~300 ms visible, in-out cubic (slow start, fast middle, slow end) |
| Bead before (`m1`, `g1`) | none | 0 then 350 at the first frame | no growth: a single frame jump |

MEASURED_WARM

## 1. Control centre dock

QML `NacreControlCenter`: a full-frame-height right dock (`contentHeight = parent.height`), status row (battery text, edit, **settings gear**, power), tiles, Light | Dark | Auto as one segmented control, then a **Notifications** block (title, count, clear, "view all" and the two newest entries or "No notifications"/"Loading notifications…"). Bead (shell/control/*.luau, `control_center.rs`):

| Item | Change |
| --- | --- |
| Height | `cc.max_height` is the viewport height minus the live frame (`Frame::current().top + bottom`); the panel is always that tall (scrolls when a page is longer). |
| Gear | `kit.IconButton settings` -> `cc.do open:settings` -> the host closes the dock and routes `settings open` (`app::route`) from a short thread. |
| Battery text | no battery -> "Connected to power" (QML text). |
| Notifications | host projects `cc.notices` (newest first, from the already leased `notifications` topic: `history.order/items`), `cc.notice_count`, `cc.notice_ready`, `cc.notice_clear`; verb `clear:notifications` calls `notifications.clear {scope: history}`. The dock shows the QML block; the bell tile and the DND tile open the docked page "notifications" (DND switch plus the list) instead of the dashboard / a text page. |
| Appearance | `kind="segmented"`: Light | Dark | Auto with icons; `choose:appearance.mode:<mode>` -> `appearance.set_mode`. |
| Sub-labels | tiles show the published status (`Off`, `On`, device name, profile); an unavailable tile stays dimmed with its label. |

Power page: `nacre session` / Alt+X open the `session` view, which is now **the same dock** (control width, framed viewport height): back arrow (`view.open control`), title "Power", a two-column tile grid (Lock screen, Log out, Restart, Power off; Suspend is not offered by the QML page and is no longer listed, the host action stays) and the inline confirmation card with Cancel and the action. Entries are the Luau `entries` table; focus and Esc handling follow the new ids `act-<key>.main`.

Known differences: Wi-Fi/Bluetooth/battery tile detail pages still open the status popouts, not docked pages (QML docks them: needs the popout bodies hosted by the control view, a second projector in one runtime); QML hides unsupported tiles (no backlight, no sink) while Bead dims everything it cannot use; tile order follows the stored tiles/defaults, not QML's catalog order; the notification list shows two entries without actions or swipe.

## 3. Settings window

| QML | Bead change |
| --- | --- |
| Header 64 px: logo, "Nacre Settings", current page at the right, rule | `shell/settings/main.luau` header (own copy of the logo asset, this process only); `bead.settings.title` is "Nacre Settings" (also the bar title). |
| Rail: raised rounded surface, width `huge*7+xl`, body-colour bordered search field, caption-size items `huge+compact` tall, accent wash on the current page | same tokens. |
| Sections are raised rounded cards (title, description, rows) | the projection nests rows (`rows`) in their section item (`settings.rs`); `section()` draws a `kit.Card`-style column. Search results stay flat. |
| Unavailable setting: label and description stay, a muted caption gives the reason | `help` rows carry `title`/`note`; drawn by `unavailable()`. |

Known: QML sliders/dropdowns are steppers/plain text here; the content width follows the window like QML.

## 4. Launcher Quick controls (not done; ask)

The QML column needs, for `actions.controls.dnd|night-light|stay-awake`, the current on/off and the reason, and the updates count. Bead already receives DND (`notifications.do_not_disturb`), night light and stay awake (`controls` topic) for the control centre, so no new Nacre contract is needed for those three: the launcher adapter would lease `controls` and `notifications` while the launcher is open and project `launcher.quick` rows. The updates count needs the `updates` topic (B0066: nothing publishes it to Bead; ask: a one-key count on the launcher topic, or lease `updates`). Not implemented here: typing lanes must stay unchanged and the column needs its own launcher golden review.

## 5. Harness

Pointer injection: `spikes/virtual-pointer.c` (wlr virtual pointer on the harness' private parent labwc, absolute motion, optional held button; build recipe in its header, binary passed with `--pointer-tool`). States `control-hover`, `control-pressed`, `lip-hover-right`. The pointer target `PTR['control-tile']` is a QML-layout coordinate; Bead's tiles sit elsewhere, so the hover comparison is per-arm approximate until the target is per-arm. Brightness and keyboard OSD states are not added: the Bead OSD shows daemon `osd` events only and the standalone daemon has no backlight device to emit them (ask: a harness fixture in the daemon, or a `previewFeedback`-like route in Bead).

## Reveal (dashboard growth)

`Scene::reveal(progress)` (bead-render): while a `reveal` panel is partly open its body fill and shadow are reshaped to the revealed part (rounded on the growing edge, shadow follows) and the content stays put under the reveal window; progress 1 restores the open-state shapes exactly (`Motion::parts`). Unit test `reveal_reshapes_the_body_and_its_shadow_and_restores_them`. Fixes B0066's known difference (square corners, clipped shadow).

## Framework notes

ADR 0004 Generalization list updated (panel motion `reveal`/`progress`, settings nested rows, harness pointer tool). `kit.ToggleTile` forwards `position/left/top`.

## Temporary duplicates

`shell/settings/assets/nacre-logo.themed.svg` copies the bar's logo (the Settings app is its own process and loads only its own assets): delete when apps can share a bundle asset (not before B0065's memory work lands).

## Checks

STATUS_CHECKS
