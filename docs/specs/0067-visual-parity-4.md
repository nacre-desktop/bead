# B0067 - Look-and-feel parity, slice 4: motion measurement, control dock, Power page, Settings window

Status: **Review** (Claude worker, branch `task/b0067-parity-4-20261023`). Owner 2026-10-23: "Bead must look correct on all fronts." Continues B0066 (its missing queue row is added with this one). Lead stop: 00:30; what landed is below, the rest is ranked at the end.

## 0. Motion measurement (harness)

`spikes/parity-compare.py` now captures transitions with **overlapped `grim` processes started every `--seq-interval` ms (default 25)** instead of one grim after another (~100 ms each): 40 frames per second on a region, stamps are spawn time plus half of the fast-quartile capture latency, the same for both arms. `results.json` gains `edge`: per frame `[ms, bottom px, right px]` of the pixels that differ from the first frame, so a growing panel shows direction, speed and easing as numbers (the gallery prints them under each strip). The trigger now runs in a thread beside the capture: a state opened by several dispatches (toggle, then tab) took longer than a 360 ms transition and the old sequential harness hid it (B0066 could not see its `reveal` for this reason). New state `dashboard-warm` measures the open after one open/close.

Findings (dashboard, same output 2880x1800 at 1.5, region `300,0 1320x760`; times are ms after the trigger, the QML starts ~25 ms earlier because Bead's dispatch is a slower process):

| | QML | Bead before | Bead now (`g10`) |
| --- | --- | --- | --- |
| open, bottom edge (0.5-scale px) | 27 (150) 53 (175) 91 (250) 154 (275) 234 (300) 294 (350) 338 (400) 350 (450) | 0 then 350 in one frame (no animation) | 27 (150) 33 (200) 51 (250) 110 (300) 222 (350) 305 (400) 347 (475) 350 (500) |
| open progress 10 / 50 / 90 % | 205 / 264 / 318 | none | 229 / 297 / 343 (cold) and 228 / 280 / 343 (warm) |
| close progress 10 / 50 / 90 % | 286 / 329 / 387 | none (stays until gone at ~450) | 302 / 338 / 393 |

Direction (grows down from the bar, shrinks back), duration (~360 ms) and in-out cubic easing match; the 10-90 % span is 113 ms (QML) and 114 ms (Bead).

Causes found and fixed:

1. **The cold open never animated.** `Animations::target` takes the first target of a new id as where the panel already is; a panel surface is created per open, so its first draw started at progress 1. `rendering.rs` now registers the closed state (0) first when a panel is drawn open for the first time. (The warm second open already animated; B0061's measurement saw only that.)
2. **The reveal did nothing on real panels.** (a) The presenter used worker-prepared frames verbatim (`TreeCache::prepare` early return), so the reveal clip of B0066 was never applied; `TreeCache::revealed_frame` now derives the frame for the current progress (clips every item to the reveal window, re-rasters the body shadow when a body shape was found). (b) The body was "the first fill of the scene", which is a 12 px dot in the dashboard; it is now the scene bounds less the 32 px presenter margin (first shadow, then largest fill as fallbacks). (c) The harness ran its trigger (toggle, then tab dispatch) before the first frame, which took longer than the transition and hid it in every earlier gallery.
3. The harness could not see anything finer than ~100 ms (see above).

Known difference (unchanged from B0066, not fixable in this slice): the dashboard's body, its shadow and the bar-contour join are one baked frame-art Image primitive (`frame_art.rs`), so the growing edge is the clip of that image: square corners and no shadow below the edge until it is fully open. QML keeps both while growing. Fix: generate the art per progress (or an edge-aware 9-slice) in `frame_art.rs`; not started. `Scene::reveal` already reshapes a panel whose body is a plain `Rounded` + `Shadow` pair (unit test), which is the general path for panels without frame art.

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

`Scene::reveal(progress)` (bead-render): while a `reveal` panel is partly open a body made of a `Rounded` fill and a `Shadow` is reshaped to the revealed part (rounded on the growing edge, shadow follows) and the content stays put under the reveal window; progress 1 restores the open-state shapes exactly (`Motion::parts`). Unit test `reveal_reshapes_the_body_and_its_shadow_and_restores_them`. Panels with baked frame art (the dashboard) are clipped only, see section 0.

## Framework notes

ADR 0004 Generalization list updated (panel motion `reveal`/`progress`, settings nested rows, harness pointer tool). `kit.ToggleTile` forwards `position/left/top`.

## Temporary duplicates

`shell/settings/assets/nacre-logo.themed.svg` copies the bar's logo (the Settings app is its own process and loads only its own assets): delete when apps can share a bundle asset (not before B0065's memory work lands).

## Evidence

`docs/specs/evidence/0067/`: dashboard open/close strips (QML, Bead, Bead before) and QML-left/Bead-right pairs for the control dock, Power page, Settings (idle and Appearance) and the network popout (same session, 2880x1800 at 1.5, wallpaper and palette of the owner's record). The full gallery (all frames, numeric diffs, the `edge` series) is `parity-gallery/b0067` on the laptop. Pointer states (`control-hover`, `control-pressed`) are in the gallery only: the target is a QML coordinate, so they are not paired evidence yet.

## Not done (ranked, for the next slice)

1. Dashboard growing edge: per-progress frame art (rounded corners, shadow) in `frame_art.rs`.
2. Docked Wi-Fi/Bluetooth/battery/audio detail pages in the control dock (needs a second projector in one Runtime or the popout bodies as dock pages).
3. Status popouts (Wi-Fi, Bluetooth, battery, audio) geometry, content and bar-contour join: not touched; the gallery shows the bar-join and body-corner differences (`popout-*`, rmse 14-19 %).
4. Launcher Quick controls (section 4).
5. Per-arm pointer targets; brightness and keyboard OSD states (section 5).
6. i18n: the in-process catalog still comes from Nacre's `bead.*` mirror, so "Lock screen", "Nacre Settings" and "Connected to power" appear only after the lead runs `tools/i18n-sync.py NACRE --write`.

## Checks

On prod (Arch container `ghcr.io/archlinux/archlinux:latest`, `--cpus=12 --memory=24g`, ttf-dejavu and ttf-ibm-plex installed, git snapshot for spikes; the workflow's Ubuntu steps ported to pacman): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `tools/check-service-boundary.py`, `cargo test --workspace --locked` (nacre-shell 456 passed, bead-render reveal unit test), `memory_policy` trace tests, renderer feature checks, facade `--check`, `nacre-shell --check shell/ui/main.luau`, `tools/check-luau.py` with luau-analyze 0.740, spikes unit tests and `py_compile`, packaging tests, private service lifecycle, SNI/DBusMenu private bus, `cargo deny` (root and framework crates): green. `spikes/wallpaper-video-smoke.py` ran with the file capability of Arch's `sway` removed (docker refuses it) and reports `fallback_visible_after_kill=false`: not touched by this unit (no wallpaper code changed), recorded as an Arch-container difference, not verified against a pre-change build. Goldens regenerated (settings, control, session) and eyeballed. Memory: no new always-on allocation; the new data (`cc.notices`, section rows) exists only while the control panel / Settings window is open.
