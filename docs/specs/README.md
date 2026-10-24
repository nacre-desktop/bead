# Bead implementation queue

2026-10-21 owner structure/licensing decision is recorded in
[ADR 0004](../decisions/0004-structure-licensing-generalization.md); the
planned “Bead for others” milestone and generalization list are in the roadmap.

Current owner handoff: 2026-10-22. B2 is done after Hyprland and niri passed the
desktop relative gate. B3 has measured evidence for five pairs per compositor.
B0019 adds serial query/order identity and RPC RTT evidence; typing target
acceptance remains open. The dated transfer and execution receipts below remain historical records.
[Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Eligible work |
| --- | --- | --- | --- |
| 0 | [B0093 Selector prompt (`menuPrompt open`)](0093-menu-prompt.md) | Review - `nacre-menu pick|input`, Wi-Fi chooser and window chooser now open a real prompt (on-demand `--app prompt`, launcher look, filter/arrows/Enter/Esc/mouse, answers via `menu-cli respond`); desktop parity rmse 3.2-3.6; full CI green | Ask: Nacre `i18n-sync --write` for 8 `bead.prompt.*` ids. Known gaps in spec (no open-menu refusal, no heart/scrollbar). |
| 0 | [B0094 Kit polish for QML parity](0094-kit-polish.md) | Review - `kit.Select` drop-downs (Displays scale/resolution, enums above 4 options), ApplyBar fix, control-centre header/DND-Clear pills/level rows/appearance pill, gradient slider fill (`interfaceGradients`), brightness description, Nacre i18n mirror; slider close-up rmse 30.7 -> 12.6 | Known gaps in spec (list pushes rows, no click-outside, estimated pill widths). |
| 0 | [B0091 Control centre parity](0091-control-centre-parity.md) | Review - QML slider drawing in the kit, QML default tile set and quick-action grid (registry default no longer hides them), device-name labels, tile geometry, profile buttons, Sound captions, bar updates count clipping fixed | Full Arch CI; before/after pairs and rmse in evidence/0091. |
| 0 | [B0089 Settings Displays page](0089-displays.md) | Review - the QML cards (primary and arrangement, per-output Scale and Resolution and refresh rate, Keep this display change?) over Nacre N0063 `displays` and `display.apply/confirm/revert`; Keep card gains a countdown; no arrangement canvas (QML has none); control-centre brightness/keyboard tiles already exist | Full Arch CI; fake-daemon tests and goldens pass; QML pair for the page |
| 0 | [B0088 Preview startup](0088-preview-startup.md) | Review - not reproducible on the desktop (first `ui.dispatch` 35-190 ms, same as the previous build); harness hardened: `TimeoutExpired` caught, 12 s process limit, 60 s deadline, clear errors | Python unit tests and py_compile on a remote host. |
| 0 | [B0084 Every Nacre route, keybind and entry point with Bead as the shell](0084-trial-switch-routes.md) | Review - Nacre `c6dc627b`; 49 CLI commands, 21 routes, all shipped binds audited: 15 routes work, 3 work with a stated refusal, 2 no-ops, 1 broken (selector prompt, fails fast); brightness readback and notification history already work through the daemon; fixed lock-screen media keys (Nacre `lock-info`) and Settings/Colors/Welcome window rules for Bead's app ids | Asks: selector prompt view (`menuPrompt open`); lock-screen content feeder (`lock-prepare`, QML-only today). Last unit before the trial switch. |
| 0 | [B0090 Live window previews](0090-live-window-previews.md) | Review — leased `CaptureHub` (ext-image-copy-capture + foreign-toplevel source; 350 ms, one frame in flight, downscaled PNG, released on close), Overview tile image with icon fallback, `livePreviews` honoured; unit-tested, live Hyprland frame not yet observed; **niri 26.04 lacks the protocol, icons stay** | `bead-wayland/src/capture.rs`, `nacre-shell/src/extras.rs`, `shell/extras/main.luau`. Unit tests, full Arch CI; nested CPU/memory/frame-rate harness written but the nested Hyprland would not start; niri needs `ext_foreign_toplevel_image_capture_source_manager_v1` + `ext_image_copy_capture_manager_v1`. |
| 0 | [B0087 VPN tile and page](0087-vpn.md) | Review — `vpn` tile and docked page over Nacre N0062 (pin `e994f3ce`), hidden when unavailable | Full Arch CI and six private QML/Bead pairs with a fake NetworkManager (RMSE 4.8-6.1%); gaps in spec. |
| 0 | [B0086 Wi-Fi Scan, application audio streams, microphone level](0086-wifi-scan-audio-streams.md) | Review — pin `4db189af` (N0061); Scan networks with progress, per-application volume/mute cards, lease-only input meter | Full CI; QML pairs not compared (B0083 harness fakes for pactl not landed). `bead.*` keys (5) need `tools/i18n-sync.py` against Nacre after B0083's renames. |
| 0 | [B0082 Consume N0059/N0056/N0057 producers](0082-consume-daemon-producers.md) | Review — pin `c6dc627b`; audio, battery and power profiles live in the control centre (profile ids, unsupported state, battery estimate, degradation reason), daemon weather photo with Commons credit, N0057 weather fields verified | Full Arch CI; fake-daemon tests and goldens pass; QML pairs for the control centre and Weather page are not compared (desktop busy, harness lacks fake audio/power). `bead.*` keys need `tools/i18n-sync.py --write`. |
| 0 | [B0085 Memory follow-ups](0085-memory-followups.md) | Review - release profile: fat LTO, dependencies `s` with render/text crates and all workspace crates at 3 (-2.0 MiB clean file, latency unchanged, build +26 %, release only); dashboard-close transient diagnosed (a +1.6 MiB bar-arena step independent of the close, panel, audio and allocator trim; +0.6 MiB retained after close), no fix | Lead review; optional heaptrack follow-up |
| 0 | [B0081 Memory gate on unreclaimable memory](0081-memory-gate-unreclaimable.md) | Review - `code_pages` removed; gate = anon + shmem + DRM + SwapPss, laptop cap about 10 MiB (stop 9.5), PSS and clean file-backed reported as information | Laptop reference capture under the new gate; optional opt-level/LTO profile unit (B0069 measured -2 to -3 MiB clean file) |
| 0 | [B0083 Weather consent in Settings, i18n sync, harness fakes](0083-weather-settings-i18n-harness.md) | Review — Date, time and language page: "Find location automatically (sends your IP address to ipinfo.io)" switch (`weatherIpLookup`, dimmed while a city is set); Nacre `bead.*` tables synced; harness fake pactl, UPower and power-profiles-daemon plus a private PipeWire for the QML arm | Full Arch CI; control-centre pairs (rmse 4.5-6.6) and differences in spec; `audio.level` live-projection `UI_ERROR` fixed. |
| 0 | [B0083 Weather location in Settings, i18n sync, harness fakes](0083-weather-settings-i18n-harness.md) | Review — Weather location hint "Set a city to show weather." (no IP-lookup switch, owner decision); Nacre `bead.*` tables synced; harness fake pactl, UPower and power-profiles-daemon plus a private PipeWire for the QML arm | Full Arch CI; control-centre pairs (rmse 4.5-6.6) and differences in spec; `audio.level` live-projection `UI_ERROR` fixed. |
| 0 | [B0092 Palette consumes N0058 palette mode](0092-palette-mode.md) | Review - `menu.query` mode and route `palette`, daemon order shown as is, Nacre pin `d07efe48`; fake-daemon tests; QML/Bead pairs | Full Ubuntu CI job on a remote host; desktop parity pairs. |
| 0 | [B0075 First-key regression](0075-first-key-regression.md) | Review — standalone first key 7.259 ms; five-pair memory/opening median gates and full Arch CI pass | Terminal animation pose, fresh SHM repair and shared raster work; Home-bar first key remains 10.017 ms in its distinct lane. |
| 0 | [B0079 Extras placement and motion](0079-extras-placement.md) | Review — QML bottom attachment, token-controlled reveal and shared window glyphs | Full Arch CI and final private nested pairs after B0075; whole-frame RMSE 7.803/4.369/2.851%; remaining gaps in spec. |
| 0 | [B0080 Populated Home first key](0080-home-first-key.md) | Review — Home 7.598 ms, standalone 6.508 ms; five-pair opening/memory gates and full Arch CI pass | Bounded markup reuse, ready-projection coalescing and partial SHM repair; fixture/nested only, no live switch. |
| 0 | [B0076 Docked device detail pages](0076-docked-detail-pages.md) | Review — shared Luau bodies, dock navigation, hidden unsupported tiles and catalog defaults | Full Arch CI, fake-daemon actions/retirement, goldens and private QML/Bead pairs; residual typography/provider gaps in spec. |
| 0 | [B0077 Overview, command palette and keybindings](0077-overview-palette-keybindings.md) | Review — independent lazy Luau surfaces; capture/move and exact palette content contracts remain | Scoped compositor/shortcuts data, menu engine activation, fake daemon and lifecycle tests; paired private evidence in spec. |
| 0 | [B0074 Dashboard first-reveal delay](0074-dashboard-reveal-delay.md) | Review — median 10% within −4/+5 ms of QML cold/warm; memory +0.262 MiB closed; full Arch CI | Admission clock and content-sized SVGs. Inherited first-key CPU gate remains; 90% image proxy ~25 ms later. |
| 0 | [B0072 Launcher Quick controls](0072-launcher-quick-controls.md) | Review — scoped controls/notifications/updates, shared control calls, release activation and search focus | Full Arch CI and native parity; five desktop pairs pass memory/opening/flips, first-key CPU 18.359 ms fails 8 ms (main 17.656). |
| 0 | [B0073 Visual parity round 6](0073-visual-parity-6.md) | Review — guarded audio/profile controls, optional updates consumer, shared control and OSD metrics | Full Arch CI and nested CPU evidence; live updates producer and dashboard reveal offset remain. |
| 0 | [B0071 Visual parity round 5](0071-visual-parity-5.md) | Review — rounded growing frame, top-attached status popouts, verified per-arm pointer targets and brightness/keyboard OSD fixtures | Full remote Arch CI and nested CPU pairs; remaining visual/control gaps and unmeasured memory gate explicit in spec. |
| 0 | [B0068 UI route handlers and live app palette](0068-ui-route-handlers.md) | Review - every `shell-routes.json` route answered or refused with a reason (session extras, `brightness set`, `keyboardLight step`, `drawers` launcher/session, `pearls open controls`, `welcomeApp refresh`, `menuPrompt` fails fast); drift-guard test; apps follow the daemon palette live (`PaletteFeed`) | `commands.rs`, `reload.rs`, `key_routes.rs`, `app.rs`, `wallpaper.rs`, `bead-wayland` palette feed. Full remote CI; parity harness for the app accent. |
| 0 | [B0065 Always-on file-backed memory](0065-mem-filebacked.md) | Superseded by B0081 (code_pages removed) - text of the larger binary was 88-93 % resident; clean file mappings are released every 3 s while closed-idle (`code_pages`); desktop CPU: 10-11.5 MiB vs control 24, anon unchanged by the change | `bead-wayland/src/code_pages.rs`, smaps/memory harness `spikes/b0065-smaps.py`; full CI container run; laptop capture left to the lead. |
| 0 | [B0070 Settings pages consume N0051](0070-settings-n0051.md) | In progress - pin `f3193ec0`; every B0063 "requires a Nacre contract" line replaced: capability flags `bluetooth`/`battery` from the topic, clipboard/launcher history clears, `software-input` layouts picker and touchpad, `shortcuts` list, `system.status`, `tools.launch` and `ui.dispatch`/`brain.open` launch buttons, `audio.set_default`, `bluetooth.set_trusted` | `settings/pages.rs`, `shell/i18n`. Fake-daemon tests, goldens, full remote CI; rebase only own commits (B0067 edits the window chrome). |
| 0 | [B0069 Code-page release verified](0069-code-pages-verify.md) | Superseded by B0081 (code_pages removed) - `code_pages` is a PSS/`Mapped` counting change (-15.7 MiB), not freed RAM (cgroup `memory.current`, page cache, `MemAvailable` equal); launcher +0.1 ms, dashboard/keys unchanged, 0 major faults, +0.05 % core idle CPU; recommendation: keep as is, optional build-profile proposal -2.3 to -3.0 MiB PSS | `spikes/b0069-codepages.py` (latency, idle, account, cgroup pressure); no product change. Desktop nested niri, CPU renderer, cp vs nocp same tree. |
| 0 | [B0063 Settings pages batch 2](0063-settings-pages-batch-2.md) | In progress - capability conditions (`capabilities` topic) in the registry engine; date and time (zone picker, NTP, 12/24 h), notification history, workflows, lock-login, privacy, keyboard-mouse, power, system, nacre-ai, sound, network, Bluetooth, add-ons, brain pages; Nacre asks C1-C10 in the spec | New kit pieces (`InfoRow`, `ChipsRow`, `DeviceRow`, `PickerList`, `SecretRow`), `settings/pages.rs` adapter. Fake-daemon test per page, goldens per page, full remote CI; side-by-side with QML in the spec; no push of live switch. |
| 1 | [B0067 Visual parity, slice 4](0067-visual-parity-4.md) | Review - harness: 25 ms overlapped frame capture with edge series, trigger beside the capture, wlr virtual pointer states; dashboard cold open now animates (first target started at 1) and the reveal reaches worker-prepared frames: open/close timing matches QML (10-90 % span 114 vs 113 ms); control centre as full-height dock (gear, notifications block and page, segmented appearance); Power page is the docked `session` view; Settings header, rail, raised section cards, "unavailable" caption. Left: rounded growing edge of the dashboard (frame art), docked network/Bluetooth pages, status popouts, launcher Quick controls | `bead-render`/`bead-wayland` motion, `control_center.rs`, `session_menu.rs`, `settings.rs`, Luau. Remote CI green on prod (one Arch-container smoke difference, see spec); paired QML/Bead evidence in the spec; no push of live switch. |
| 1 | [B0066 Visual parity, slice 3](0066-visual-parity-3.md) | Review - dashboard `reveal` panel motion, clock format follows `desktop.clockFormat`, updates count host store; ranked leftovers carried into B0067 | `bead-render`/`bead-wayland` reveal clip, `bar`/`dashboard` Luau. Checked on dev-2; no push of live switch. |
| 1 | [B0061 Visual parity, slice 2](0061-visual-parity-2.md) | Review - dashboard opens opaque in 360 ms in-out cubic (new general motion `fade`) with the QML tab strip (pearl, no pill); launcher Home title, hint text and prefix chips; bar notification bell with count, "Desktop" title, enamel workspace pearl; harness reports easing times. Left: control centre dock, status popouts, launcher Quick controls, hover/pointer states. |
| 4 | [B0062 Bluetooth state, power, discovery, connect](0062-bluetooth-state-ui.md) | Review - pin `edfda155` (N0047); QML-shaped popout: Find devices with scanning state and daemon auto-stop, device cards with battery and Pair/Connect/Disconnect (busy row), empty and error texts, 352 px wide; control-centre tile uses `set_power` only | `status_views/devices.rs` edge, `status/bluetooth.luau`; fake-daemon tests, 18 goldens, QML parity gallery with a fake org.bluez (`spikes/fake-bluez.py`), relative memory gate (CPU). |
| 4 | [B0064 Colors app and Welcome demo on N0050](0064-colors-live.md) | Review - pin `09cf91fc`; newest-wins preview, `minimumContrast`, apply/export `busy`, theme targets through `colors.set_target` in Settings, Welcome colours demo (scenes, eight styles, dark/light, desktop preview; preview only) | `colors.rs`/`welcome.rs` adapters, `settings/pages.rs`, `shell/welcome/main.luau`. Fake-daemon tests, goldens, full remote CI; A7 (Open image) waits for the portal method; no push of live switch. |
| 4 | [B0060 Colors app](0060-colors-app.md) | Review - `nacre-shell --app colors`: Studio, Compare, Accessibility on the kit, capability-gated; the explorer needs unpublished `colors.catalog/preview/apply/export` (asks A1-A7 in the spec), without them it shows the active palette and applies nothing | New kit pieces (`ColorSample`, `ColorTarget`, `PreviewCard`, `ImagePick`, `TabStrip`, `ChipRow`, `Paragraph`), `colors.rs` adapter, `shell/colors/*.luau`. Fake-daemon tests, 8 goldens, Settings wallpaper/colors rows; side-by-side with QML not run (gap list in the spec); no push of live switch. |
| 4 | [B0059 Welcome app](0059-welcome-app.md) | Review - `nacre-shell --app welcome`: the first-run tour (hero, colors, shortcuts from the `learn.shortcuts` menu route, app tiles with setup guides, ready, help) on kit components over a data catalog; 74 `bead.welcome.*` ids | `shell/welcome/*.luau`, `welcome.rs` adapter, window self-close in `bead-wayland`. Fake-daemon and runtime tests, 10 goldens, nested smoke, side-by-side with QML (gap list in the spec: palette and logos wait for B0056, the colors demo for B0060's asks); no push of live switch. |
| 1 | [B0056 Look-and-feel parity with the QML shell](0056-visual-parity.md) | Review - `spikes/parity-compare.py` (QML and Bead in identical nested sessions, gallery with numeric diffs and transition strips); fixes: live palette in frame/panels, bottom launcher, themed logo, bar pill spacing, external-panel frame cut, dimmed control tiles, preview teardown | Owner's five items fixed; ranked gaps in the spec (launcher Home, control-centre dock, popout content, session menu, pearl). Full CI replica on dev-2; no push of live switch. |
| 4 | [B0051 OSD kinds, pin bump, motion e2e, fixtures fix](0051-osd-kinds-pin.md) | Review - pin `1432a440`; all eight N0043 OSD kinds from daemon events (QML pill, kinds as Luau data), N0036-reply path removed; real-daemon dynamic wallpaper verified nested; `try-bead --dashboard-fixtures` fixed | `notice_service.rs` event consumer, `shell/osd/{main,kinds}.luau`, 11 goldens, `spikes/b0051-gates.py`. Relative CPU memory gate passes (prod, niri nested); full remote CI; no push of live switch. |
| 4 | [B0046 Dynamic (video) wallpaper](0046-dynamic-wallpaper.md) | Review — separate `bead-wallpaper-video` renderer (libmpv, crop fill, loop, last frame), generic policy/protocol/supervisor crate, QML pause rules plus battery/lock/sleep/DPMS; blocked end-to-end on one additive Nacre field (`wallpaper.active` `motion`) | ADR 0005 exception (renderer is not a UI provider). Renderer memory/CPU reported separately; shell relative gate; unit, supervision and headless-sway smoke tests; full remote CI; no push of live switch. |
| 2 | [B0047 Desktop frame, reserved edges, lips and hover intent](0047-desktop-frame-lips.md) | Review — live frame geometry and per-side toggles, reserved edge zones, hover/click lips with dwell and gate, lips declared in `shell/frame/behavior.luau` | `bead_wayland::{intent,edges}`, `ShellControl::edge_chrome`/`bar_thickness`, `placement::Frame::current`, `frame_art.rs`. Unit tests, 16 goldens, full remote CI; nested niri proof and relative CPU memory gate in the spec. Assistant lip hidden (no view); no keyboard focus, no fullscreen suppression; no push of live switch. |
| 7 | [B0031 Dashboard media and system finish](0031-dashboard-media-system-finish.md) | Review — seek, album art, Today mini player, network sparkline and Wi-Fi frequency implemented | Builds on B0027 (same pin). Host card and screenshot thumbnails: see B0034. Full remote CI; no push of live switch. |
| 7 | [B0034 Dashboard host card and screenshot thumbnails](0034-dashboard-host-thumbnails.md) | Review — pin `3499bcbe` (N0038); host card on System, thumbnails on Screenshots | Fake-daemon/fixture tests and goldens; full remote CI; no push of live switch. |
| 4 | [B0035 Settings stores](0035-settings-stores.md) | Review — inventory of every QML-honoured setting; reduced motion, temperature unit and dashboard core tabs implemented | Read-only `settings.store.desktop`/`dashboard` follower in `nacre-shell` (`prefs.rs`) plus a generic `bead-core` motion switch; live, no restart. 3 Nacre blockers (wallpaper rotation, power profile, battery alerts) and 7 Bead-side gaps listed with reasons. Fake-daemon wire test; full remote CI. No push of live switch. |
| 4 | [B0036 Settings gaps](0036-settings-gaps.md) | Review — panel flags, bar enable, fonts, text size and frame sheen live; workspace icons/count, window hints and launcher rail open | `bead_ui::text_style`, `bead_wayland::ShellControl`, mapping in `nacre-shell`. Fake-daemon tests; full remote CI; no push of live switch. |
| 4 | [B0037 Workspace icons, window hints and launcher rail](0037-workspace-launcher-parity.md) | Review — workspace count/icons/hints and rail order/hidden implemented live; its three Nacre blockers are resolved by N0040 and consumed in B0038 | `workspace_rows.rs`, `Workspace.fixed_slot/visiting`, prefs follow `profile` and `launcher-layout`. Fake-daemon tests; full remote CI; no push of live switch. |
| 4 | [B0038 Engine rail, visiting groups, glyph focus](0038-rail-visiting-focus.md) | Review — pin `4d816ca2` (N0040); `menu.groups` rail, niri visitor groups, `window.focus` on window glyphs | Rail from `menu.groups` with `launcher-layout` order/hidden; compositor-topic origins; capability-gated glyph focus. Fake-daemon tests, six launcher goldens (rail only); full remote CI; no nested smoke or memory capture (desktop lock held); no push of live switch. |
| 4 | [B0055 Bluetooth device list and pairing](0055-bluetooth-pairing-ui.md) | Review - pin `1432a440` (N0045); device list with Pair, pairing page for PIN, passkey, confirm, authorize and display prompts with countdown, cancel, errors and retry | `kit.ActionButton`, `status/bluetooth.luau`, `status_views/pairing.rs` edge. Fake-daemon tests per prompt kind incl. PIN hygiene, 14 goldens, full CI; N0045 Welcome timeout bug and unserved `bluetooth` topic recorded as Nacre asks; no live visual or memory run. |
| 4 | [B0050 Wi-Fi password, hidden network, saved networks](0050-wifi-secrets-ui.md) | Review - pin `61af74be` (N0042); masked password prompt with wrong-password re-prompt, hidden join form, saved list with inline-confirm forget | `TextInput masked`, `kit.PasswordField`/`kit.ConfirmButton`, `status/network.luau`, `status_views/join.rs` edge. Fake-daemon tests incl. secret-hygiene test, 12 goldens, full CI; no live visual or memory run. |
| 4 | [B0057 Control center consumes N0046](0057-control-center-n0046.md) | Review - pin `393b4aa6`; absolute volume/mic/brightness/keyboard, power profile picker, airplane, night light, stay awake, recorder, screenshot, picker, next wallpaper, appearance; VPN/displays/settings route still waiting | `controls` topic lease only while open, panel closes before interactive tools; fake-daemon tests per tile, goldens; QML gaps listed. |
| 4 | [B0049 Control center tiles](0049-control-center-tiles.md) | Review - tile grid, detail, drag edit, catalog; persisted through `settings.set` | Luau registry + kit tiles + `control_center.rs` adapter; unpublished tiles listed with their Nacre ask. Fake-daemon tests, goldens, full CI; no push of live switch. |
| 4 | [B0054 Pearl memory and drag cadence](0054-pearl-memory.md) | Review - pearl open vs docked +0.15 MiB (was +11.97), closed file-backed residue (bar worker never idled, fonts mapped) fixed; drag p99 not demonstrable nested (flat 32.5 ms), shell frame p99 <= 4.8 ms; QML side-by-side open | Border as 8 pieces over content, worker drops stale rasters on detach/dock, pearl timers routed through slots, `close_pearl` idles the primary view. No push of live switch. |
| 4 | [B0048 Pearls](0048-pearls.md) | Review - detach/dock/drag guides, goldens, restore; memory gate fails (+12 MiB pearl open, +1.4 MiB closed file-backed), drag cadence unmeasured on real output | Framework `bead_core::pearl` + `bead_wayland::pearls` (slots swapped around panel code); Nacre look in `nacre-shell/src/pearls.rs`. Pearls are layer surfaces (no xdg). Bead-local layout store (ask in spec). No push of live switch. |
| 4 | [B0043 AI sidebar chat](0043-ai-sidebar.md) | Review - chat, approvals, markdown, copy, streaming p99 4.4 ms (CPU, dev-2) | Framework `transcript`/`markdown`/`kit.Transcript`/`multiline` input + `chat.rs` adapter over `ai.chat` (pin `4d816ca2`). Saved-chat list, attachments and Brain are contract asks; no push of live switch. |
| 0 | [B0045 Settings window framework](0045-settings-window-framework.md) | Review — xdg toplevel window, registry-driven form kit, store read/write, Appearance and Notifications pages | `bead-wayland::window`, `kit.luau` form components, `nacre-shell/src/settings.rs` adapter, `shell/settings/**`. Fake-daemon round trip, 5 goldens, typing and lifecycle tests; full remote CI; no push of live switch. |
| 0 | [B0058 Settings pages batch 1](0058-settings-pages-batch-1.md) | In progress - slice 1: panels, windows and workspaces, date/time/language editable; planned-backend rows hidden like QML; lock/login view only | `shell/settings/registry/bead.json`, `settings.rs` planned filter. Fake-daemon test, 5 goldens, full remote CI. |
| 1 | [B0052 On-demand app mode](0052-on-demand-app-mode.md) | Review — `nacre-shell --app NAME`: Settings and the AI sidebar are separate short-lived processes (own daemon client, single instance, exit on close); cold open Settings 33 ms, sidebar 73 ms | `bead_wayland::Options::app`, `nacre-shell/src/app.rs`, ADR 0005/0004 text. Nested smoke + relative memory (+0.48/+0.52 MiB, borderline: window tracking in the shell), unit tests, full remote CI; frame-tile join with the sidebar and focused-output choice open; no push of live switch. |
| 3 | [B0041 Launcher grid key cost and compaction timing](0041-launcher-grid-key-cost.md) | Review — first key 10.4 -> 7.7 ms CPU (focused surfaces compact only after 5 s idle); grid window one row smaller at the top (-0.6 ms p50); the grid was already virtualised, view reuse across grid/list never hits (reverted) | `SHM_COMPACT_FOCUSED_IDLE` in `bead-wayland`, `kit.VirtualGrid`. Owner trade: up to ~+10 MiB PSS for 5 s after open vs 2.4 ms first key; closed/after-close memory unchanged. dev-2 sway harness; full remote CI; no push of live switch. |
| 3 | [B0044 Launcher flip: one commit, order-aware damage](0044-launcher-flip-paint.md) | Review — BackSpace (list -> grid) 11.8 -> 7.5 ms CPU, default lane p50 10.9 -> 6.9, first key 7.8 -> 6.2; goldens unchanged | `View.grid` result reuse in `nacre-shell`, `align()` damage in `bead-render`, allocation-free primitive hashing. Memory within the relative gate; full remote CI passed. No push of live switch. |
| 4 | [B0042 Session menu confirmations and Suspend](0042-session-menu-confirmations.md) | Review — inline confirm for log out/restart/power off (Enter/Esc, 4 s timeout), Suspend on the published route | Owner-chosen deviation from the QML (which has neither). Fake-daemon tests, goldens, full remote CI. No push of live switch. |
| 3 | [B0040 Steady typing CPU bisect](0040-typing-cpu-bisect.md) | Review — no steady regression: B0039 compared the grid/list lane (10.8 ms at B0019 and now) with B0019's list/list lane (1.2-1.3 ms in the fixture lane both times) | First-key cost flips on idle SHM pool compaction (+2.4 ms after B0039's adoption); disabling it gives 7.7-8.0 ms but is a memory trade-off for the owner. No product change; dev-2 sway harness, no push of live switch. |
| 3 | [B0039 Launcher first-key latency](0039-launcher-first-key-latency.md) | Review — first key -5.3 ms CPU / -5.3 ms presented on niri (CPU renderer); Gate E first-key CPU still above 8 ms | Spare-buffer adoption in `bead-wayland` rendering, ASCII font pre-selection for the launcher. 5 alternating pairs on niri and headless sway; relative memory gate passes; full remote CI; no push of live switch. |
| 7 | [B0031 Dashboard media and system finish](0031-dashboard-media-system-finish.md) | Review — seek, album art, Today mini player, network sparkline and Wi-Fi frequency implemented | Builds on B0027 (same pin). Host card and screenshot thumbnails are not published by the contracts. Full remote CI; no push of live switch. |
| 3 | [B0032 i18n consumer](0032-i18n-consumer.md) | Review — `tr()`, 279 `bead.*` ids, N0035 provider, live refresh | `ui.tr`/`tr` in Luau, `bead_ui::i18n`, paged `bead` namespace with stale-revision retry, English built in; Nacre catalog seeded first. Fake-daemon tests; full remote CI. No live switch. |
| 5 | [B0028 Launcher activation and hide](0028-launcher-activation-hide.md) | Review — ui.menu.activate and shell hide implemented; nested smoke passed | Provider handles copy, copy-clip, command, power (lock direct, rest via the session view) and route rows; daemon stale_revision re-queries. `nacre hide` is a shell-wide destroy/recreate-surfaces toggle. Wallpaper/tool/form rows, brightness OSD value and keyboard light stay contract-blocked. |
| 0 | [B0029 Packaging, coexistence and rollback](0029-packaging-coexistence-rollback.md) | Review — isolated rehearsal only | Separate `bead-shell` package, one-drop-in switch `nacre ui-shell bead|qml` (Nacre branch `task/b0029-nacre-side-20261022`), QML windows companion, journaled rollback. No install, push of live switch or live enablement; owner gate. |
| 5 | [B0026 Keybind routes and settings integration](0026-keybind-routes-settings-integration.md) | Review — 12 of 16 shell keys mapped; blockers A-E named | Launcher-family and brightness-step routes, launcher toggle, live palette into panels. Hide, keyboard light, AI sidebar and non-app launcher activation need new contracts. i18n/settings: findings only. Full remote CI; no push of live switch. |
| 7 | [B0027 Dashboard daemon contracts](0027-dashboard-daemon-contracts.md) | Review — pin `652d6c4f`; focus, clipboard, screenshots, AI usage, Brain, media playback card, system gauges and tab customization implemented | Everything gated on the advertised topic/method; lazy per-page leases, no polling (countdown wakes only while a session runs in an open panel). Fixtures + fake-daemon tests; full remote CI; no push of live switch. |
| 6 | [B0024 Dashboard extras parity](0024-dashboard-extras-parity.md) | Review — Notifications/Devices/Network extras implemented; rest contract-blocked | Read-only extras from published `notifications`/`bluetooth`/`audio`/`network` topics at Nacre tab indices 5/7/8. Player/MPRIS, focus, metrics, clipboard/screenshots/AI/Brain and customization writes need new daemon contracts (exact fields in spec). Full remote CI; no push of live switch. |
| 0 | [B0023 Session menu and secure lock](0023-session-menu-secure-lock.md) | Review — N0031 `session.request` schema applied | Fail-closed hand-off to the existing hyprlock lock; menu disabled unless the daemon advertises the method; non-lock actions confirm. No Bead-side lock, no live switch. |
| 0 | [B0025 Per-output UI sessions](0025-per-output-ui-sessions.md) | Implemented — acceptance passes | Per-output runtime/update/commit/input/teardown, shared immutable resources, scale/idle release. Full Bead CI and matched CPU memory gates pass; see result and niri nested hotplug limitation. Clears B0022 architecture blocker. |
| 2 | [B0022 Bar composition and lifecycle](0022-bar-composition-lifecycle.md) | Review — implemented on N0031 routes | Output-scoped workspace/title/pill/tray calls gated on the advertised registry, tray from the daemon topic, B0023 `session.request` schema fixed. Fake-daemon runtime tests, full CI and nested two-output proof recorded in the spec. No push of live switch. |
| 0 | [B0021 Static wallpaper](0021-static-wallpaper-surface.md) | Review — matched CPU wallpaper memory target met | Worker decoder at `ac27ca3`: +0.251/+0.619 MiB H/N over main excluding niri-held SHM. Both nested lifecycles, switch and full prod Arch CI passed; no install or push. |
| 1 | [B0053 Notice overlay slot](0053-notice-overlay-slot.md) | Review — popups coexist with launcher/dashboard (own overlay slot), suppressed only by popouts, session and control; follow the compositor-focused output | General second view slot (`Options::overlays`, `CompanionOffer` by catalog); notices and OSD are its first views. Fake-daemon tests, nested smoke 26/26, relative memory gate PASS (idle -0.07, closed -0.16 MiB), full remote CI; no push of live switch. |
| 1 | [B0030 Notices in the main shell](0030-notices-in-main-shell.md) | Review — in-process notices/OSD; smoke 35/35; memory table in spec | One process and one UI provider: popup stack and OSD are panel views offered by the bar sessions on the shell's daemon link; the separate notice process is removed. Popups now live in the B0053 overlay slot. |
| 0 | [B0020 Notifications/OSD](0020-notifications-osd-product.md) | Review — consumer implemented on published N0017/N0036; process split superseded by B0030 | Daemon consumer, views, dashboard history/DND, OSD; 32/32 nested smoke against the real producer. Keyboard-layout OSD, swipe dismiss and bar history popout remain out of scope. No live ownership or push. B0019 reserved for concurrent launcher work. |
| 4 | [B0019 Launcher parity](0019-launcher-parity.md) | In progress | Catalogue/favourites/categories, daemon ordering, input/activation and stale-result fencing; private validation, no push or live switch. |
| 4 | [B0019 Launcher parity](0019-launcher-parity.md) | Review — implementation validated; performance/memory acceptance open | Source c8b55b5; async catalogue/favourites/group routes, ordered results, input/activation and stale fences. Full remote CI and private input/reopen passed; see spec for diagnostics and limits. No push or live switch. |
| 0 | [B0018 Status pill](0018-control-center-status-pill.md) | Done | Source c6ba4b3; four device peek/click routes, switching and lazy close. Exact-source private CPU input and full remote Arch CI equivalent passed; no push or live switch. |
| 0 | [B0016 System tray](0016-system-tray.md) | Review — core producer blocked | Local source6efca12; headless service, lazy preview menu and private/nested proofs. Daemon ownership/action publication and matched memory gate remain open; lead reviews/integrates, no push. |
| 0 | [B0014 Bluetooth single-adapter power](0014-bluetooth-single-adapter-power.md) | Done | Sourcedcf541a9 integrated normally; exact-source full hostedCI39240289443 SUCCESS. Capability-gated; no producer or live enablement claim. |
| 0 | [B0013 Wi-Fi radio switch](0013-network-wifi-radio-switch.md) | Done | Sourcecdd9fb2 integrated021fdaad; exact-headCI39230443828 SUCCESS. Capability-gated, no live producer/enablement claim. |
| 1 | [B0012 light measurement prep](0012-measurement-readiness-light-prep.md) | Done: light prep accepted | Report14b93337 accepted; command routing and geometry/scope gaps recorded. No runtime/tool changes or captures. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Done — 2026-10-22 | Desktop Hyprland deltas +0.121/+0.134/+0.219/+0.047 MiB; niri +0.237/+0.249/+0.229/−0.998 MiB. Anonymous growth <+1 MiB; DRM unknown and excluded. Candidate 306474f0 / UI 7948c3c. See [desktop batch report](../research/b0002-desktop-batch-20261021.md). |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Review — 10 historical valid pairs; B0019 adds serial item/query identity and RPC RTT | B0003-S89afb381 accepted/CI39123319816 green. No product change or target acceptance; see [measured report](../research/b0003-measured-20261022.md). |
| — | [B0015 Audio](0015-daemon-audio-readback-popout.md) | Done | Source dc8832fe / CI39388972154 green; docs e5fc3d88 / CI39390219914 green. Read-only capability-gated view; no producer or live claim. |
| — | [B0011 Battery](0011-daemon-battery-status-popout.md) | Done | Sourceaf61d18/fullCI39147491406green; docs5ad2551. Lazy lease/optional readings/private lifecycle tests accepted. |
| — | [B0010 Bluetooth](0010-daemon-bluetooth-status-popout.md) | Done | Sourcee3e417e/CI39136586458green. Capability-gated; no running producer claim. |
| — | [B0009 Network](0009-daemon-network-status-popout.md) | Done | Source5d96384/docsbd7bf56/CI39134388246green. Capability-gated. |
| — | B0001/B0004/B0005/B0006/B0007/B0008 | Done | Prior accepted reports retained; no duplicate work. |

## New lead — 2026-10-20 evening

Claude Code (Opus) is now lead, with Codex workers. Owner direction for B2: test
the hypothesis that Luau's incremental GC never finishes collecting after the
first close in an idle shell. The fix runs one full GC cycle after a panel
releases and the UI is idle, adding malloc_trim only if evidence shows it helps.
Phase A, assigned to a Luna worker with the desktop heavy slot, is an
opened-once-only comparison on Hyprland: control (frozen `b01dfcca`) against the
fixed build, 3 alternating arms each, with unchanged capture gates. If the fixed
median is under 25 MiB and the anonymous increase is under 1 MiB, the full B2
batch runs on both compositors and is decided on medians under the transfer
rule. If the fix changes nothing, B2 stops and the result is reported; there are
no speculative fixes. B3 follows when the slot is free.

### Lead update — 2026-10-20 night

- The B2 GC fix is implemented: UI `7948c3c` runs one full Luau collection on
  the first idle notification after a release. 37 bead-ui and 113 nacre-shell
  tests pass, along with clippy and 49 harness mocks. The release build is
  `306474f0`, and the harness opened-once mode with its research note is at
  `cb79930`. Both are local only.
- Provenance: the frozen control artifact `b01dfcca` no longer exists on the
  host, and source `1cff9d17` does not rebuild bit-identically. The rebuild is
  `e21caa16`. Lead decision for phase A: compare a same-toolchain rebuild of
  `1cff9d17` (control) against `7948c3c` (fixed), with the common helper
  `4a65f4ec`, which was recovered and rehashed.
- Captures are blocked on an unattended swap reset. Host swap is about
  6.7 GiB, and the standing rule needs it below 64 MiB. Processes started by
  the new lead run with no-new-privileges, so the reset is launched through
  the user manager, which also needs a narrow NOPASSWD sudo rule for
  `swapoff -a` / `swapon -a`. The owner has been asked for it.

### B2 phase A result — 2026-10-21

The opened-once comparison on Hyprland compared a same-toolchain rebuilt
control (`e21caa16`) against the GC-fix build (`306474f0`, UI `7948c3c`), with
helper `4a65f4ec`. There were 3 valid arms per side; one environmental F2
host-swap-in was replaced once. Medians: control 22.05 MiB PSS and 5.38 MiB
anonymous; fixed 22.01 MiB and 5.25 MiB (Δ −0.04 PSS, −0.14 anonymous; DRM
unknown). This passes the owner's numeric rule. However, the earlier +1.94 MiB
anonymous opened-once growth did not reproduce in the rebuilt control, so no
causal GC improvement is claimed. The GC change stays in the candidate as
defensive, with no measured effect; the earlier excess most likely belonged to
the lost frozen artifact. The full B2 batch (both compositors, 5 pairs each,
unconditional zram reset before every arm) started 2026-10-21 01:21 local. The
research note and harness fixes (`9babf54`, `bbac822`) are local and integrate
with the batch result.

### B2 status — 2026-10-21 midday

- Laptop full batch, as supporting evidence only: Hyprland PASS on all four
  phases with 5 matched pairs (candidate medians 22.15–22.72 MiB, about −0.13
  to −0.17 MiB anonymous where pages opened). niri was paused after 6 valid
  arms when the owner's live switch needed the host.
- Owner rule: captures run on the owner's DESKTOP, not the laptop. The original
  frozen control `b01dfcca` and helper `4a65f4ec` exist there. The harness
  (`f676a1c` watchdog fix, `6185c6e`/`1b1729d`/`3109905` launchers with
  env/argv evidence) is ready. Desktop plan: a diagnostic first (`b01dfcca` vs
  rebuild `e21caa16`, opened-once), then the full batch (A = `b01dfcca`,
  B = `306474f0`, both compositors). It starts when the desktop heavy slot is
  free.

### Owner decision 2026-10-23 — no QML after the switch

The owner decided the switch target is complete QML retirement. The AI sidebar,
Settings, Colors, Welcome and every other QML surface move to Bead/Rust; no
QML windows companion remains. The session menu adds confirmations and Suspend.
Owner (same day): port EVERY QML feature; the inventory's retire candidates (video wallpaper, pearls,
desktop frame, tile editor, weather hero, VPN, live previews) are all ported. SDDM greeter stays for now.
Owner (same day): Bead must LOOK AND FEEL the same as the QML shell. Every visible unit compares
side by side against the QML in identical nested sessions (screenshots + animation timing) and fixes
or lists each difference; self-goldens are not parity. B0056 audits and fixes everything built so far
(owner preview found: icon spacing, launcher rising from top, panel colours, dropdowns not joined to the
frame, logo colour). Switch gate: the owner signs off a side-by-side QML/Bead gallery of every surface.
Plan: [QML retirement inventory](../research/qml-retirement-inventory-20261023.md)
(groups A–F). In flight: B0030, B0042, B0043 (AI sidebar), B0044, B0045
(Settings framework); Nacre N0042 (Wi-Fi secrets), N0043 (osd topic).
B0029's `nacre-qml-windows.service` is now a temporary bridge (expires at the
Group F deletion pass).

### Owner decision 2026-10-24 — gate on unreclaimable memory (B0081)

B0069 showed the B0065 `code_pages` release only lowers counted PSS (-15.7 MiB), not memory the kernel
holds. The owner chose option (b): remove it and gate always-on Bead on **unreclaimable memory = anon +
shmem/SHM + DRM + SwapPss**, about **10 MiB** absolute on the laptop (per-sample stop line 9.5), relative
deltas <= +0.5 MiB median per phase against a same-host control. Clean file-backed text/rodata is reported
as information next to PSS. This replaces the 25 MiB PSS cap and the 24.5 stop line, the 27 MiB one-time
file-backed allowance (removed from `dashboard-residency.py` / `dashboard-batch-summary.py`) and every
"PSS+DRM+SwapPss" gate wording below, which stays only as history. Detail: [B0081](0081-memory-gate-unreclaimable.md).

### Laptop reference capture — night of 2026-10-23 (in progress)

The owner chose option (a) and the lead runs the capture on the laptop while
it is idle. Three attempts stopped safely before a valid arm. Each stop
exposed a harness defect, not a product result:
1. The quiet-host gate (`nacre-lead` processes) saw remote-only worker ssh
   commands. The rerun waited for workers to finish; the gate is unchanged.
2. A CPU-rendered UI has no DRM client, so the harness reported "DRM
   unavailable". Fixed in 62cf817 (lead as spec agent): no /dev/dri fd means
   known-zero DRM; a DRM fd without fdinfo stays unknown and still blocks.
3. Open-panel samples were never classified (always "inactive"), so the
   strict backend validator could not pass. A fix is in progress; the
   validator stays strict.
4. Run 4 stopped at B1: a stale candidate binary (9ce4209) against the current
   `shell/` tree raised a Luau `UI_ERROR`, so `source_error=true` never cleared.
   Not an audio problem (both arms run with PipeWire unavailable). Fixed by a
   manifest `shell_git_tree` gate and a fail-fast on `UI_ERROR`; candidate rebuilt
   from 7f7cd74 and a six-arm desktop rehearsal was ARM_VALID.
The swap reset works on the laptop (7.3 GiB → 22 MiB, with MemAvailable >
swap + 8 GiB). Evidence: ~/.local/share/nacre/ai/lead/bead-laptop-reference-20261022/evidence-*.

Prep for the night of 2026-10-24 (Claude worker, current main `56b9499`: B0037–B0063, notices in
process, frame, pearls, app mode): the candidate and helper examples were rebuilt from that exact
commit (Rust 1.99.0, release, `--locked`, Arch container on prod; the build script stops on any
cargo error). The launcher now (a) records `measured_process` from the PID's cmdline and
invalidates an arm unless it is the always-on `nacre-shell --bar --core-views` (Settings, sidebar,
Colors and Welcome are separate `--app` processes outside the 25 MiB target), and (b) pins the
`shell`, `crates`, `vendor` and `Cargo.lock` git trees in the manifest and refuses a dirty
checkout. The control, its bundle and the dashboard fixtures tree (`51dbf56f`) are unchanged.
Known limit: the harness runs without a daemon link (see the runbook). During a capture no
process command line may contain "nacre-lead" (the quiet-host gate matches it), including
worker ssh commands and wrapper scripts. Runbook: `docs/research/laptop-reference-capture-runbook.md`.
Desktop rehearsal on the rebuilt candidate (56b9499; niri nested, 12/12 arms ARM_VALID, production and
`--include-cpu`, no verdict): candidate − control medians in MiB, production never +1.89, once +3.25,
several +3.37, combined +3.21; CPU batch +1.86, +3.22, +1.88, +3.35. File-backed PSS is +2.4 to +3.1
(larger binary); anonymous is −0.7 to +0.8. The candidate's absolute level on the desktop was 26.8 to 27.9 MiB
(control 24.4 to 26.0), so the 25 MiB laptop gate is at risk from file-backed growth.

### Lead note — render backend and the niri wallpaper buffer — 2026-10-22

B0021 measurements (9be5869) showed two things that matter for the memory gate:
1. **Render backend dominates absolute PSS.** Matched CPU-rendered builds idle
   at about 9 MiB (Bead main, 2880×1800@1.5). The same builds with native GL on
   the NVIDIA desktop measured about 55 MiB, because of driver, LLVM and Gallium
   mappings. The absolute 25 MiB gate is only meaningful with the backend
   recorded. The laptop reference capture (Intel i915) must log the backend and
   should run both CPU and native GL if time allows.
2. **niri keeps wl_shm wallpaper buffers referenced** (no release), so a static
   wallpaper costs about 10 MiB of SHM PSS on niri, against +0.25 MiB on
   Hyprland where the buffer is released. Count it separately from UI overhead
   and compare it with the QML shell's own wallpaper cost. A dmabuf/GPU-texture
   path is a possible follow-up.

### Owner decision — laptop reference memory capture — 2026-10-22

Owner chose option (a): Bead's absolute memory gate is measured on the laptop
(the reference machine) in a supervised capture. The owner picks an idle
window of about 30–45 minutes, and the capture runs in a private nested
session only, under the standing capture gates; the live desktop is never
touched. The lead prepares the harness in advance, so the capture starts when
the owner signals. Re-forecast: the observed unit throughput (B0016 tray about
4 h, B0018 control center about 5 h of worker time with full CI) suggests about
3–5 weeks to the core switch, against the gap review's conservative 8–13 weeks.
Serial gates remain: secure-lock review, installer/rollback rehearsal and the
owner's live-switch validation.

### Core-switch gap list — 2026-10-22

Read-only review of what Bead still lacks before it can run the always-on parts
on the owner's laptop: [research/core-switch-gap-20261022.md](../research/core-switch-gap-20261022.md).
The forecast is 8–13 calendar weeks with about 3 workers and full CI per unit;
lock and installer reviews can extend it. The order of units is Gate E
typing/memory on the laptop reference, then bar and tray, popouts and control
center, launcher parity, notifications and OSD, dashboard, static wallpaper,
session and secure lock, settings/keybinds/theming, and finally packaging and
rollback. B3 recommends a bounded investigation of rounded-row painting as its
follow-up. This is a plan, not authorization for live validation or the switch.

### Owner rule change — relative memory gate on non-reference hosts — 2026-10-21

Owner decision (2026-10-21 evening): on hosts other than the reference machine
(the laptop), Bead memory gates are judged RELATIVE to the frozen control
measured on the same host in the same batch. PASS if candidate − control is
≤ +0.5 MiB (median) in every phase, with no unexplained anonymous growth > 1 MiB.
The desktop's absolute level (~26.5 MiB on niri) is recorded as a host baseline
and is not a stop condition. The absolute 25 MiB ceiling / 27 MiB one-time cap
(stop 26.5) applies on the laptop as the reference machine. This supersedes the
absolute-cap check for the B2 desktop niri half and re-confirms the Hyprland
result (all candidate − control medians ≤ +0.22 MiB).

### B2 Hyprland desktop result and spec-agent allowance decision — 2026-10-21

Desktop (`siverteh-desktop`) Hyprland: 10/10 valid arms. A is the frozen
`b01dfcca`, B is the candidate `306474f0`, and the helper is `4a65f4ec`.
Medians in MiB (control | candidate): never 24.753 | 24.874, once 24.877 |
25.011, several 25.062 | 25.280, combined 25.640 | 25.687. Candidate − control:
anonymous 0 / −0.129 / −0.027 / −0.070, file-backed +0.105 / +0.195 / +0.195 /
+0.125, shared 0. DRM is unknown. The diagnostic (frozen vs same-toolchain
rebuild, opened-once) showed no binary difference (24.853 vs 25.186, anonymous
Δ +0.027), so the earlier +1.94 MiB excess was not the binary.

Decision (lead as spec agent, under the standing rule "ceiling may rise ONCE to
27 MiB (stop 26.5) with written rationale and spec-agent review"): the one-time
27 MiB allowance is granted for B2. Rationale: (1) the desktop host's
file-backed baseline puts even the frozen control above 25 MiB; (2) the
candidate's increase is +0.05 to +0.22 MiB, essentially file-backed, with
anonymous memory flat or lower; (3) the unexplained remainder is ≤ 0.03 MiB,
far below the 1 MiB stop; (4) all candidate medians are ≤ 25.69 MiB, under the
26.5 stop. DRM stays unknown and is not counted as zero or as file-backed. The
allowance is now spent for Bead; any later increase above 25 MiB goes to the
owner. niri (5 pairs) continues under the same gates with the 26.5 MiB stop.

## Lead-transfer receipt — 2026-10-20

### B2 decision and preserved source

Owner supersedes the single-arm24.5 stop for this fresh batch: decide each phase
from five matched alternating pairs per compositor. Candidate median below25MiB
with explained category changes passes; median25–27 only when the extra is
file-backed may use the one-time27 allowance; above27 or unexplained anonymous
increase above1MiB stops. Do not classify graphics as file-backed or unknown DRM
as zero. Quiet/swap, geometry, containment/watchdog and close bar+5 rules remain.

- Frozen UI source `1cff9d17d61fb05451546bb0ea13c826c93cb4c9`, release binary
  SHA256 `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b`;
  common helper source5f4 and SHA256
  `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`.
  No UI rebuild or feature-source substitution in this batch.
- Local source worktree `bead-b0002-failure-report-20261020`: commits
  `e66a747db0cd797ecdbf3e1c93d5cc56ec9ab90f` and
  `f8f504fde80c9c18fb0ca955780c562987e29b25`, not pushed. Final clean HEAD is
  `52d0df09ad1d7ef94d93475df454ef9160da90ec` on branch
  `b0002-capture-failure-report-20261020`; the two further local commits
  `cb1fca8` and `52d0df0` add then revert the unsuccessful Nice-property change.
  Preserve all four unpublished commits; do not blindly replay the reverted fix. These add explicit
  `BEAD_DASHBOARD_BATCH_MEDIANS=1`, paired category reducer and its collector,
  preserving strict defaults. Worker reports49 mocks/py_compile/diff-check pass;
  lead independently ran48 at the preceding revision. No hosted CI is claimed
  for these local changes; prior harness c807591 CI39410701291 is historical.
- Fresh batch begins15:22:31UTC after a pre-measurement failed priority check.
  Duplicate additive `nice` caused priority19; corrected controller ancestry and
  no-measure smoke proved priority10 plus12GiB/swap0 before the fresh start.
  Initial failure raw remains separate; it is not an accepted measurement arm.
- Initial raw endpoints from the old A1/B1 batch remain in the
  [earlier report](../research/b0002-hyprland-capture-20261020.md), not reused.
  New partial data below are a distinct fresh run, not a completed comparison.

### Partial Hyprland results, MiB

All listed completed arms have full endpoint samples; the worker reports quiet
and safety verification, SwapPss0 and zero host swap-in. DRM is absent/unknown;
reported PSS-plus-DRM field must not be described as measured graphics memory.

| Arm | Never opened | Opened once | Opened several | Combined |
| --- | ---: | ---: | ---: | ---: |
| A1 | 24.095703 | 24.123047 | 24.155273 | 24.426758 |
| A2 | 24.267578 | 24.275391 | 24.283203 | 24.565430 |
| A3 | 23.780273 | 23.780273 | 23.784180 | 24.141602 |
| B1 | 24.107422 | 25.345703 | 23.916016 | 24.587891 |
| B2 | 23.970703 | 25.310547 | 24.001953 | 24.502930 |
| B3 | 24.257813 | 24.109375 | 24.171875 | 24.883789 |
| A4 | 24.493164 | 24.499023 | 24.497070 | 24.238281 |
| B4 | 24.433594 | 23.828125 | 24.007813 | 24.673828 |

Final **four-pair partial** opened-once medians are control24.19921875,
candidate24.70996094MiB (+0.51074219); anonymous5.25390625→6.279296875
(+1.025390625), file18.25048828→17.67480469 (−0.57568359), shmem0.671875 both.
This is below25 total but slightly above the anonymous delta rule. It is NOT the
required five-pair batch and has no final PASS, GC trigger or causal leak verdict.

**Final stop/release:** A4 finished17:37:36UTC. Eight arms were captured
(A1–A4/B1–B4); seven have controller `CELL_ACCEPTED`. A4 capture returned0,
quiet/safety passed and immutable-evidence postcheck passed; an initial `Path`
import typo in the administrative stop hook ended the controller after verify.
The import was corrected and verify rerun on the same raw data, printing PASS
then explicit `OWNER_HANDOFF_STOP` rc75. No recapture and no B5 start occurred.
Two Hyprland arms (A5/B5) plus ten Niri arms remain unrun.

Private REMOTE raw/controller root is
`~/nacre-quiet/evidence/bead-b0002-20261020/median-cb1fca8/`;
`controller.sh`, `controller-status.txt`, `controller.log` and `postcheck.py`
remain there. Final postcheck SHA256
`8e47b1b8ca6b69c8b150971bbcae58b0125403032acc044a71c5701134417a82`.
Full raw archive sibling `median-cb1fca8.hyprland.raw.tar`, SHA256
`dcc9ce92f3c5da8643bb0fe8bf16142b897d3eba2af0e85e7bc2feb9533a52de`
(458 entries checked). Do not mistake these for local mirrored paths.
Worker confirmed controller PID and child absent, no capture processes/scopes,
bench-lock free; final MemAvailable12,982,712KiB and host pswpin unchanged.
**Heavy slot RELEASED; all workers stopped.** No continuation is armed or held.
The raw evidence is preserved, not deleted or reconstructed.

### Next units and boundaries

1. The new lead must explicitly schedule any continuation/fresh batch; this
   lead's owner-transfer stop is not a measurement failure or a retry grant.
   Preserve original rows and report any administrative gap without inventing
   completed pairs. No B3 measurements before B2 passes.
2. Conditional owner-authorized GC attempt only if the full opened-once median
   confirms above25: target one full Luau cycle after release/idle, and trim only
   if relevant. Read-only source mapping found the panel Runtime/VM is destroyed,
   but the retained bar Runtime receives `set_idle(true)` via its owning worker
   and currently uses the no-op trait default. This is a possible hook, not proof
   of causality. `malloc_trim` already runs immediately and after100ms. Verify
   actual mlua API and ordering before coding; no GC change was made.
3. If targeted fix is justified, owner asks three control and three fixed
   opened-once Hyprland arms with identical quiet/swap rules. Only below25 and
   anonymous growth below1 proceeds to a full five-pair batch both compositors;
   no improvement means stop/report, no speculative fixes. Base candidate on
   frozen1cff, not unrelated newer shell work.
4. N10 has priority for desktop builds between batches. Its latest local source
   and remaining gates are in the Nacre queue. Hosted GitHub CI does not consume
   the desktop slot. No package/live action is authorized here.
5. Other Bead status: B13 Wi-Fi, B14 Bluetooth and B15 Audio are Done,
   capability-gated; no producer/live availability claim. B3 source stage is Done,
   measured stage remains gated. No new independent worker was started at stop.

## Earlier execution receipts (historical)

B0014 reviewed source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d` is now on
main; exact-source full hosted CI39240289443 SUCCESS. All six acceptance groups,
additional retirement/boot cases and factory regression pass. Duplicate automatic
main CI39241346554 was canceled by lead instruction; no second full result is
claimed. Hosted slot released; no private host or lab used for B0014. Completion
receipt is documentation-only with CI skipped. Earlier ownership below is history.

B0013 focused correctness finished and RELEASED08:39UTC. Final sourcecdd9fb2
independently ACCEPTED;11 actor ticket,5 Wi-Fi Runtime and1 template tests PASS,
fmt and affected all-target Clippy PASS. Earlier failures are retained. Exact integration021fdaad published; full hostedCI39230443828 SUCCESS. Evidence archive6ccfe29daa51e4c6a92fc445552a6cdd47fae08facfee0423a8284a0a6517f71.
Scope manager/effective12GiB/swap0 and held-child membership verified; no OOM,
peak2,021,376,000B. Actual SCHED_IDLE/priority19 preserved; no host policy change.

Nacre Niri-only source8a quick pair released09:28UTC with valid cells but failed
memory screen +5.756/+6.121/+0.488MiB. No H/full-eight; no current capture.
Bead used the released sole heavy slot for021fdaad CI39230443828, now completed
SUCCESS09:37:56UTC. All workers released except read-only Nacre diagnosis.
Bead B2/B3 measurement remains after F4 and explicit grant.
Owner clarified installation was first laptop, then desktop08:08:50UTC during
an earlier Nacre candidate warmup: ONLY wayland-utils, no other package changes.
Desktop /usr/bin/wayland-info availability subsequently verified; no probe run.

B0012 light prep is complete. B0013 remains capability-gated to actual Welcome
method plus fresh topic; no producer/pin change. All task workers released.
Other device controls/static wallpaper need a concrete next bounded spec.

Frozen B2 runtime1cff9d17/releaseb01dfcca and helper5f4/4a65 remain separate from
newer shell features. Source/profile/hash enough for nested provenance; unknown
artifact identity remains unknown. Raw private evidence stays outside Git.
Capture rules: owner-sized scope max(knownbaselinepeak×1.5,12GiB) onlab,swap0,
verified heldchild containment and independent watchdog; host>=4GiBstart/abort<2GiB;
quiet SwapPss0/hostswapin<1MiB; only own exactscope/PIDs stopped. No hostpressure,
cache drops orsysctl. Geometry means actual output proof, not config request.

## Workflow and owner boundaries

Built-in workers only, maximum3 active; separate old implementer chats retired.
One heavy job on the desktop: local builds/tests, nested sessions or captures.
GitHub-hosted CI may run concurrently; desktop-hosted CI may not. Lead grants.
Light docs/read-only prep may proceed in parallel. Lead staysWorking while
workersrun, postsstarts/finishes and about30min long-job heartbeat (global skill).

Commit approvedsiverteh noreply author+committer, isolatedworktrees, normalmain
push onlywhenreviewed, noforce/rebaseothers. No live desktop/configs/units/
installedNacre/packages/tags/releases withoutowner; preserveAIpermissions/paru.
IncidentCLOSED. Informationaladvisory0192 ratified; exact0370/71/72 exceptions
retained; vulnerability/unsound advice owneronly. No new Nacre protocol changes.

## B0013 Done — 2026-10-19

Reviewed sourcecdd9fb2, structurally preserved in normal integration021fdaad735f3b1db835d267c117e325018d333e.
Exact-head [CI39230443828](https://github.com/nacre-desktop/bead/actions/runs/39230443828)
completed SUCCESS09:37:56UTC: fmt, Clippy, tests, feature/config/schema, capture
regressions, private lifecycle and dependency/license checks. No local full suite,
measurement, package, live enablement or producer availability claim. Source and
queue integrated normally; no force. B2/B3 remain explicitly unscheduled afterF4.

## B0014 Done — 2026-10-19

The fresh sole-adapter power switch uses published5f4 `bluetooth.set_power` v1,
shared CallTicket retirement and authoritative topic state. Zero/multiple/invalid
or stale adapters omit the switch; unavailable method keeps a healthy sole
adapter read-only. Full hosted [CI39240289443](https://github.com/nacre-desktop/bead/actions/runs/39240289443)
passed on exact main source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d`.
No protocol/pin/producer, lab, install or measurement change. B2/B3 remain gated
on their explicit grants; no next device-control unit is assigned here.

## Owner scheduling update — 2026-10-19 afternoon

F4 standard five-pair/compositor acceptance is next after N9/N10 private checks.
Bead light source/review/preparation may continue; no hosted heavy CI, full suite,
release build, nested session or capture during exclusive N2. Desktop available
now; lead announces actual capture start/release rather than waiting for night.
Wayland-utils is installed; no missing-tool blocker is inferred.

B0015 Done: source dc8832fe16db0db39185f342a2ae9a82d62a9924 is integrated on
main. It adds a dedicated read-only core Sound view over the published AudioState,
lazy audio-topic lease, bounded typed initial state, and no-client unavailable
rendering. Legacy preview route and controls remain unchanged. No new provider,
producer-availability, device-control, or memory-measurement claim. See
[spec](0015-daemon-audio-readback-popout.md).

Exact-source full hosted [CI 39388972154](https://github.com/nacre-desktop/bead/actions/runs/39388972154)
passed on dc8832fe: formatting, Clippy, workspace tests, renderer configurations,
facade/headless UI/Luau schema, capture-window regressions, private service
lifecycle, and dependency/license checks. Coverage includes explicit default
output selection, partial/boosted/unknown readback, invalid snapshots,
disconnect recovery, legacy preview preservation, lazy lease/unadvertised topic,
and the unavailable read-only factory. Final published-head full hosted
[CI 39390219914](https://github.com/nacre-desktop/bead/actions/runs/39390219914)
passed on exact head e5fc3d8826493147c05ae1ecf59df294678a36c0, including this
review evidence. No desktop capture, local full build/test, or producer
availability was used to claim acceptance.
