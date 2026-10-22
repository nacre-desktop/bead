# Bead core-switch gap list — 2026-10-22

Scope: Bead checkout `origin/main` 463e483; Nacre comparison uses fetched
`origin/main` be15e3e. Nacre is assessed from its current `nacre/shell/modules`
QML tree, not the local checkout. Status means parity for the owner's laptop,
not whether a framework primitive or fixture exists.

## Always-on parts

| Part | Bead now (refs) | Nacre QML today / user-visible gap (rough size) |
|---|---|---|
| Bar + status popouts | **Partial.** Native bar/kit, top edge, placement and joined contours; Network/Bluetooth/Battery/Audio read-only popouts and guarded Wi-Fi/Bluetooth actions. B0009–15; `docs/COMPONENTS.md`; accepted placement `d88e94e`. | QML modules include status icons, active title, power button, calendar/history/network/sound/battery/Bluetooth popups and workspace/status peeks. Bead still lacks complete per-output bar composition, SNI/DBusMenu tray, full control-center pill→peek→click/detail behavior, robust service restart/tray-item lifecycle and matching action inventory. **L** (tray/actions and parity), **M** (composition/peeks). |
| Launcher | **Partial.** Search/query UI path and real `menu.query` measurement harness exist; B0003 remains Review (10 valid pairs; trace lacks exact item IDs/query identity/RPC RTT). Favourites/categories, complete populated grid/modes/ranking and activation parity are not demonstrated. B0003; Roadmap Feature 10 / Gate D. | QML has launcher panel, app browser, calculator, software/theme/wallpaper previews, menu model and sources. Missing catalogue/modes/ranking parity, favourites/categories, preserved inputs, cold/warm/idle-reopen and accepted typing target. **L**. |
| Popouts / control center | **Partial.** Shared Popover/Peek/Tile/Slider/Switch/SegmentedControl kit and real network/Bluetooth/battery/audio views; dashboard placement/lifecycle proof. `docs/COMPONENTS.md`; B0009–15; B0002. | QML controlcenter has Overview, Levels, Media, Power, Session, Displays, VPN, Wi-Fi saved networks, Bluetooth pairing, edit/catalog/appearance/notices/updates/calendar. Bead lacks complete pill/peek/click/detail routing, editable controls for all published daemon schemas, broad device/display/VPN controls and QML-matched interaction coverage. **L**. |
| Dashboard | **Partial (core subset).** Today/calendar, Weather, enrichment-only Media, System/Updates; lazy page resources and lifecycle/placement coverage; B0001–4, accepted first pages `8558bd6`. Explicit fixture data where contracts unavailable; no invented providers. | QML has Today, Weather, Media, System, Focus, overview, customization, metrics and extras (AI/Brain/clipboard/devices/focus/network/notifications/screenshots). Bead still lacks full page/card and customization/extras parity, real providers for unavailable schemas, rich media/MPRIS, and per-page mapped behavior beyond core. **L**. |
| Notifications + OSD | **Partial / largely missing as always-on product.** Notification service/API and transcript coverage exist (`crates/bead-services`; protocol v1 history/popup/OSD fixtures); not evidence of complete resident server, visual stack, action/expiry/history/DND/inhibition parity. Roadmap Feature 9. | QML has popup stack/notice, History popout, control-center notices, and OSD event/model/panel/pill/level notice. Bead needs private-bus service ownership/activation, replacement/actions/expiry, persistent history, DND/inhibition and visual OSD/notification transitions; never take over live service before switch. **L**. |
| Static wallpaper | **Missing as an always-on surface.** Dashboard can present wallpaper/palette data; no static wallpaper shell surface/selection/application path established. Roadmap Feature 11/13; Gate E. | QML background scene/wallpaper plus launcher picker/backdrop/hex exist; settings prototype has wallpaper content. Bead needs static image selection, scaling/crop/monitor policy, palette integration and daemon/config persistence. Keep video/effects deferred. **M/L**. |
| Lock + session | **Missing for secure lock; partial session.** Session action/helper groundwork is not a secure lock screen. Roadmap explicitly says fail-closed protocol/safety gate; lock-preview cannot count. | QML has session panel/controls and lock settings/preview; core target explicitly includes hyprlock/session. Bead needs secure `hyprlock` integration, fail-closed behavior, unlock/failure/restart semantics, session menu parity and approved action routes. **L** (lock), **M** (session). |
| Keybinds, settings, packaging (cross-cutting) | **Partial.** Key command catalog/helper compatibility and protocol fixtures exist; palette uses existing Orient roles, size scale mirror; no complete laptop routes/settings integration/package installer. Roadmap Features 2/7/14; Gate E. | QML has global shortcuts, keybindings extra, settings host/pages (appearance, desktop, lock, notifications, devices, etc.), palette/theme pages and installer/runtime integration in Nacre. Bead needs route-by-route compatibility then managed config/settings wiring, palette live refresh/theming, English labels/i18n plan, and package/install + rollback. **L**. |

## Gates that remain open

- **Performance:** Gate E typing UI ≤8 ms/key and presented p99 ≤16.7 ms; after fair effort ≤33 ms p99 is only an owner candidate. B0003 source measurement is not target acceptance. Measure daemon/provider and UI separately; match official same-engine QML.
- **Memory:** laptop reference settled UI PSS+DRM+SwapPss ≤25 MiB; stop at 24.5 MiB, close ≤ own bar +5 MiB, unexplained anonymous growth >1 MiB stops. B0002 desktop relative gate passed, but DRM was unknown; do not infer laptop clearance from desktop captures.
- **Feature/quality:** complete remaining bar/popout/dashboard and Feature 9 notification/OSD; validate tray, all routed actions, restart/reconnect, themes, and multi-output behavior. Current GUI coverage is top bar and left/right panels; bottom edges unvalidated. No physical keyboard/accessibility parity claim.
- **Switch prerequisites:** Nacre remains QML daily driver. Desktop validation plan is HOLD; no live test or switch authorization. Secure lock needs fail-closed review. Fresh owner approval is needed for live desktop validation/switch/install.
- **Packaging/rollback:** produce and verify scoped packages/installer, config migration, service activation, coexistence and rollback-to-QML procedure. Current roadmap forbids release/deployment/session installation absent approval; these are separate owner gates.
- **Cross-cutting:** use published daemon schemas only; no guessed providers/actions. Preserve lazy closed+idle release. English input required; deeper IME/screen reader deferred to public release. Integrate each unit with full CI and feature proofs before proceeding.

## Ordered units and calendar forecast

Assumption: about 3 parallel Codex workers, one writer per unit/fileset, roughly
16–20 aggregate task-hours/day available, full hosted CI per unit, and owner
reviews/contract publication do not sit in the critical path. Calendar ranges
are estimates, not measured throughput; large UI parity and security reviews can
expand them.

| Order | Unit; dependencies | Size / estimate |
|---|---|---|
| 1 | Close Gate E typing evidence/acceptance and memory on laptop reference; finish any required daemon contracts. Blocks broad feature claims. | M; 3–6 working days |
| 2 | Bar composition, SNI/DBusMenu tray, per-output lifecycle/restart and QML-matched status behavior. Requires 1 and published contracts. | L; 1–2 weeks |
| 3 | Complete popouts/control-center routes and schema-backed controls; pill/peek/click/detail + tray/media entry points. Depends 2 and contracts. | L; 1–2 weeks |
| 4 | Launcher catalogue/favourites/categories, ranking/modes, activation/input preservation; close B0003 gaps and Gate D decision. Depends daemon menu.query and 1. | L; 1–2 weeks |
| 5 | Notification server + stack, OSD, history, DND/actions/inhibition parity on private bus. Depends shared service host and safe coexistence design. | L; 1–2 weeks |
| 6 | Finish dashboard pages/media/providers/customization and per-page QML parity. Depends published schemas; can overlap 4–5 with separate writers. | L; 1–2 weeks |
| 7 | Static wallpaper, palette refresh, monitor policy and persistence; no video. Depends palette contract/settings route. | M; 4–7 days |
| 8 | Session routes then secure lock integration/fail-closed review. Depends backend command contracts; security review serializes acceptance. | L; 1–2 weeks |
| 9 | Settings/config integration, command/keybind migration map, theme/i18n and owned-package service wiring. Depends units 2–8 stabilizing. | L; 1–2 weeks |
| 10 | Package/installer, coexistence, rollback rehearsal in isolated validation; then request live-switch approval. Depends all units and full CI. | L; 1–2 weeks |

Parallelism helps units 2–7, but dependency and review gates dominate. Forecast
**8–13 calendar weeks** from a clear start with three active workers and CI per
unit; add owner/daemon contract delays, a discovered performance or memory
regression, or lock/installer redesign time on top. This is not authorization to
run the live switch.
