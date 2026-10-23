# B0057 - Control center tiles consume N0046

Status: **Review** (Claude worker, branch `task/b0057-control-wiring-20261023`). Pin `393b4aa6` (Nacre N0046, `docs/specs/0046-control-center-action-contracts.md`). Builds on B0049.

## What changed

| Layer | Change |
| --- | --- |
| Pin | `nacre-protocol` moved from `1432a440` to `393b4aa6` (no dependency changes in the lock). |
| Adapter (`control_center.rs`) | `controls` joins the topics leased while the panel is open (closed: nothing leased). New sources over it: `brightness.level`, `keyboard.backlight`, `airplane.mode`, `night-light.state`, `stay-awake.state`, `appearance.mode`, `recorder.state`; one-shot sources `screenshot.capture`, `color-picker.pick`, `wallpaper.next` exist exactly while the daemon advertises the method. Verbs: `toggle`, `set`, `press`, new `choose` (power profile). |
| Luau | `tiles.luau` asks and `need` per tile (a tile shows only when its member is published and its method advertised); `main.luau` opens a profile picker in place for the battery tile (rows per profile plus "Open power and battery details"). No new kit component. |
| i18n | Four ids: `control-recording`, `control-mode-{light,dark,auto}`. Nacre's catalog needs `tools/i18n-sync.py NACRE --write` (lead). |
| Reload host | `cc.do` returns to the panel with `panel.open=false` when a tile started an interactive tool. |

## Behaviour

- Volume and mic sliders send `audio.set_volume {target: sink|source, value: (slider/100)^3}`: the slider is perceptual (cube root of the linear level), the daemon's value is linear. The relative `audio.adjust_volume` fallback and the 0.2 step are gone. Mute is `audio.set_mute {target, muted}`.
- Brightness: `brightness.set {output, percent}` (B0022 contract) on the internal panel (`eDP`/`LVDS` output, else the first). Keyboard: `keyboard_light.set {level}` as integer steps (`round(slider*max)`); a target on the same step sends nothing.
- Power profile: tile shows the active profile; with `power.set_profile` advertised the tile opens a picker over the available profiles (`power-saver|balanced|performance`).
- Airplane, night light (not switchable while `external`), stay awake: `*.set {enabled}`. Appearance: tapping cycles light, dark, auto (`appearance.set_mode`). Next wallpaper: `wallpaper.next {}`.
- Recorder/screenshot/picker are interactive: the adapter asks the host to close the panel, then calls 320 ms later (the QML delay) from a short-lived thread, because a closed panel holds nothing resident. Region mode for screenshot and recorder. A running recording shows "Recording" and the tile stops it (`recorder.stop`, no close).
- Still "Waiting for Nacre": VPN (`vpn.list/vpn.set`), displays, settings-window route (wallpaper). `brightness.get` for DDC freshness is not needed: the topic carries the level.
- Sound panel slider (`services.audio.volume`) and the bar wheel (`audio.adjust_volume`, relative by design) use Bead's local service / wheel steps, not the control-center workaround; unchanged.

## Look and feel against QML (gaps, no side-by-side run)

B0056's `spikes/parity-compare.py` is not on main and no nested session was run for this unit; only goldens (`control-n0046-overview-1.png`, `control-n0046-profiles-1.png`, `control-catalog-*` refreshed) were eyeballed. Known differences from the QML sources (`NacreControlOverview/Tile`):
1. Appearance is a full-width segmented control (own component) in QML; here a half tile that cycles the mode.
2. Recorder, screenshot, picker are compact third-width tiles in QML; the recorder is a half toggle here.
3. QML shows unavailable tiles dimmed (alpha .4); Bead hides them and lists them in the catalog.
4. QML brightness slider follows the monitor of the panel's screen; here the internal panel.
5. Tile radius/typography/icon size follow B0049/B0056, not re-measured.

## Checks

Fake-daemon tests (`control_center_tests.rs`): absolute linear volume, mic, brightness, keyboard step dedupe, every N0046 method with exact params, interactive tools close first and call after, recording stops without closing, capability gating (member without method, method without member), picker and press dispatch in Luau. Goldens as above. Full CI-equivalent and the relative memory gate are in the report; CPU renderer; closed panel holds nothing (leases drop with it).

## Temporary duplicates

None.
