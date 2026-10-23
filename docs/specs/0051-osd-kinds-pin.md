# B0051 — Nacre pin bump, all eight OSD kinds, dynamic wallpaper end to end, dashboard fixtures fix

Status: **Review.** Pin `1432a440` (N0041-N0045). The only unit that moves the pin while it lands. Consumes N0043 (`osd` events) and N0044 (`wallpaper.active.motion`); the fallout of the new pin was one test initializer (`ActiveWallpaper.motion`) and the committed `wallpaper-active.jsonl` transcript (refreshed from the pinned export; N0044 added a motion pair).

## OSD (N0043)

- **Source.** The shell's one daemon link subscribes `osd` (which starts the producers) and hands every `ev changed` to `notice_service::route_event` (the link's event hook, next to the chat one). Data is `OsdEvent`; invalid events (`valid()`: toggles other than 0/1, brightness above 1, bad timestamps) are dropped. The `audio` topic fallback and the N0036 reply path (`brightness.adjust` `level_percent` -> `bead.osd`/`Notices::feedback`, `Brightness.with_local`, the private-process socket relay) are removed. `Notices::feedback` stays only for the private compatibility fixture (`--notification-bus`); the in-process service refuses it.
- **Kinds.** `volume`, `output`, `microphone`, `brightness`, `keyboard`, `night_light`, `power_profile`, `capslock`, as the QML `NacreOsdModel`/`NacreOsdPill`. The host projects `osd.kind|value|muted|label|device|open` only (value clamped to 0..1, so a volume above 100 % draws a full bar and reads 100 %). Everything else is Luau data: `shell/osd/kinds.luau` maps each kind to its title id, detail rule, level/toggle flag and glyph rule (muted, value steps, device/label matches, label table); `shell/osd/main.luau` draws the pill. Adding or reordering a kind is a Luau edit. Title/detail strings are `bead.osd.*` ids (10 new; "Caps lock" now reads "Caps Lock" as in the QML).
- **Visuals and timing** (QML values): pill 336 wide, height 76 with a level and a detail line, 60 without a detail, 68 for the toggles/output (fully rounded), 1 px outline at 28 %, glyph at x 19, text at x 60 with a 24 px right inset, 5 px track 18 px above the bottom, muted bar at 35 %, dim glyph for muted/off, 150 ms fade and 4 px slide, hide after 2 s, hover holds it, events inside the first 400 ms are ignored. Bursts are coalesced by the daemon (40 ms, output never replaced by its volume event, 250 ms duplicate drop), so each event shows at once.
- **Not matched.** The bar fill is a solid primary, not the QML's two-stop gradient (no gradient fill primitive in the kit; a general `fill` gradient is the way, not an OSD special case). The output name under brightness is shown by the QML only with several screens; the OSD panel has no output list, so the line is omitted (the daemon sends `device`, the kinds file has no rule for it). Font weight Medium on the title has no Luau knob.
- **Temporary duplicates.** Nacre's N0036 `brightness.*` reply field `level_percent` may be deleted 30 days after Bead renders the OSD from events (this unit) and the owner has the shell running on it. Bead's `core-compat-osd.jsonl` transcript and its `OsdState.current` compatibility test stay until the same date (N0043 says the same for Nacre's copy); delete them with it.

## Dynamic wallpaper end to end

B0046 already parsed `motion`. With the pin, `spikes/b0051-gates.py motion` runs the real Nacre daemon (built from `1432a440`, private HOME) with a generated 960x540 H.264 clip and a `media.json` as the daemon expects, the shell under a nested niri in a headless labwc (Arch container on prod, CPU renderers, libmpv software decode). Result: the daemon publishes `motion.kind=video` on the nested output, the renderer reports its first frame, two screenshots 1.2 s apart differ in 126 038 pixels (the clip moves under the bar and frame), and after `media.json` is removed the daemon republishes without `motion` and the screen is static again (0 differing pixels, shell alive). No code change was needed in the wallpaper path.

## Dashboard fixtures bug

`try-bead.py --dashboard-fixtures` died with `dashboard/today/behavior.luau: error converting Lua nil to function`. Cause: with the daemon link the dashboard controls are live, so the Today mini player binds the host action `dashboard.do`; `validate_action` only skips a fixed list of host actions and otherwise looks the name up in the view's `actions` table (nil). `dashboard.do` was never in the list. Fix: `dashboard.do` joins the list. Regression test `every_dashboard_page_loads_with_live_controls` loads all 13 pages with the controls on (it failed on Today before the fix).

## Checks

- Fake-daemon test `daemon_osd_events_of_all_eight_kinds_...` (real `nacre_source` client, `osd` events from a fake daemon, models asserted per kind); unit tests for every kind's projection, clamping, invalid events, arming/hover and event routing.
- Golden test `osd_kinds_show_their_glyph_title_value_and_detail`: 11 goldens `shell/tests/goldens/osd-*-1.png` (eight kinds plus low, muted and clamped volume) with the drawn texts asserted; `osd.png` re-recorded.
- Full CI replica (ubuntu:24.04 container on dev-2, the steps of `.github/workflows/ci.yml`): passed (`CI-EXIT 0`: fmt, clippy `-D warnings`, boundary, workspace tests + trace, feature checks, facade/Luau analyze, spike tests, packaging, motion smoke, services, private SNI, `cargo deny`)
- Real daemon to screen (`b0051-gates.py osdshot`): a night-light change in the private control record becomes a daemon `osd` event and the pill appears in the nested niri (On, then Off with a dimmed glyph; screenshots eyeballed).
- Relative memory gate (`b0051-gates.py memory`, Arch container on prod, labwc + nested niri, CPU renderer, release builds, 5 alternating pairs; control = `4dfa5ee`):

| Phase (Pss MiB, shell process) | control median [min-max] | candidate median [min-max] | delta Pss | delta anon | delta file |
| --- | --- | --- | --- | --- | --- |
| idle | 28.96 [28.83-29.14] | 28.60 [28.51-28.80] | -0.36 | -0.16 | -0.28 |
| OSD shown (changed every second) | 30.29 [30.17-30.40] | 29.71 [29.63-29.87] | -0.58 | +0.51 | -1.05 |
| OSD closed | 29.86 [29.81-29.94] | 28.69 [28.65-28.92] | -1.17 | -0.04 | -1.10 |

Gate (candidate - control <= +0.5 MiB median per phase, anonymous growth under 1 MiB): **pass**; DRM is not measured (CPU backend, tiny-skia wl_shm). The control shows its OSD through the old key-route feedback, the candidate through daemon night-light events (the control cannot be driven by `osd` events); both open the same panel view. The shown-phase anonymous +0.51 (the panel view with the kinds data, not investigated further) falls back to -0.04 when closed; the negative file-backed deltas were not investigated (the candidate no longer subscribes `audio`). Control `4dfa5ee`, Arch container, 12 CPUs, swap untouched, release builds `-j8`. The relative figures between control and candidate are the gate; the absolute 25 MiB cap applies only to the laptop reference run by the lead.
- Observation for Nacre (N0043): the `osd` service runs `pactl subscribe` and treats its end as a lost audio subscription. On a host where `pactl` exists but no PulseAudio/pipewire-pulse server answers (a CI container), the session restarts until the supervisor marks the whole `osd` service `disabled` after 5 failures, so night light, caps lock and brightness events are lost too, although the spec says a missing dependency only disables its kinds. The harness puts a silent `pactl` stand-in first on the daemon's PATH.

## Look-and-feel parity with the QML (owner rule 2026-10-23): partly open

The pill was built from the QML source numbers (above), not from a side-by-side. A paired capture was attempted: the QML shell (`quickshell` 0.3.2, Nacre `1432a440`) starts in the same nested niri on prod with the same daemon, but its OSD kinds read pipewire, UPower, power-profiles and `control-tools` state through helper tools that the container does not provide, so no QML OSD could be triggered and **no paired screenshots exist**. Known differences, from the source and the Bead screenshots: solid instead of gradient bar fill; title in regular instead of Medium weight; no output name under brightness; glyph font and exact text metrics are the harness's, not compared; fade easing (OutCubic in QML) and the 130 ms level-bar animation are not reproduced (the panel fade is the kit's); position relies on the existing panel placement (QML: bottom margin frame + 24, width min(336, output - 48)), not re-measured. The paired comparison needs the owner's desktop (lock was held) with the real services.

## Not done / notes

- `tools/i18n-sync.py NACRE --write` for the 10 new `bead.osd.*` ids and the Caps Lock text (lead; Nacre is read-only here).
- No live visual check on the owner's desktop (its heavy-job lock was held); the gates above are nested with software GL.
- Keyboard-layout and Do-Not-Disturb OSDs do not exist in N0043 (as in the QML for layout; DND is a notifications event) and are not shown.
