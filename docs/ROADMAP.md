# Roadmap and gates

Updated 2026-10-13. Phase 0 recommendation is proposed. No Feature 1 approval yet.
This is a feasibility order, not a commitment to complete every feature.

For every feature: state the smallest proof and acceptance criteria; match Nacre's
current look; run fmt/clippy/tests plus feature checks; measure equivalent QML;
append FEASIBILITY.md with time/LOC/numbers/problems/verdict; update STATUS.md;
commit/push private main; provide one command/what to inspect; **stop for next**.
Do not bundle unrequested later features into the current proof.

## Phase 0 — current

Scope/source/protocol/renderer/UI/service/prior-art research, blank renderer spikes,
ADR 0001, roadmap and handoff. Cargo workspace/CI initially covers only measurement
spikes; Feature 1 adds production surface/shell crates. Stop for architecture OK.

## Feature order

| Feature | Smallest useful proof / acceptance | What the gate answers |
| --- | --- | --- |
| 1. Production skeleton and surfaces | Minimal Bead surface crate + nacre-shell executable; blank separate-namespace layer on every output on nested Hyprland and niri; clean startup/exit and hotplug removal; fmt/clippy/test CI | Can the surface/lifecycle foundation work on both without touching daily Nacre? |
| 2. Drawing and palette | Current Nacre primitives/rounded corners/borders/shadows; real font fallback/emoji; PNG/SVG/theme icons; validated existing Orient/presentation palette and last-good fallback | Does CPU-first rendering actually match the look at useful size? Compare GPU alternative if necessary |
| 3. Reactive UI/reload | Minimal typed markup/store/action schema plus Luau candidate VM; include the smallest working editor, keyed scroll list and Taffy layout needed to prove state preservation; invalid multi-file/script/resource candidate retains old UI with file/line error; no leaks across 100 cycles | **Gate B:** go/adjust/stop from measured reload/preservation/memory, owner decision |
| 4. Layout/animation/input | Taffy integration for real scene, easing/springs, pointer/scroll/keyboard/focus; basic selection/clipboard, IME and AccessKit semantics; matched animated panel pacing | Does a toolkit replacement work beyond static rectangles? Input/IME/a11y move early |
| 5. Backend state/actions | Same neutral output/workspace/window/event interface on Hyprland/niri; existing Python facade can bridge first, independent native transports next; IDs/capability errors/reconnect tested | Are backend semantics coherent rather than widget-specific hacks? |
| 6. Bar v1 | Clock/workspaces, SNI+DBusMenu tray, PipeWire audio, UPower battery, network/Bluetooth display and helper actions; daemon/item restarts; per-output look | Can actual service-driven UI sleep and share contexts? SNI is new in audited QML snapshot |
| 7. Command/files/helpers | Private versioned command socket/client plus compatibility target/method catalog; all current CLI/keybinding routes tested in fixtures; bounded runner/watch/helper protocol | Can existing tools call Bead without changing live configs? Minimal helper transport may already support earlier proofs |
| 8. Popouts/panels | One actual panel and animations, open/close/focus/dismiss across outputs; preserved state and pacing; output disappearance | **Gate C:** bar parity on both + panel; measured benefit versus cost; owner decision |
| 9. Notifications/OSD | Server on private bus, replacement/actions/expiry/history/DND, inhibition and visual transitions; no takeover of live notification service | Is native notification behavior complete enough for parity? |
| 10. Launcher | Same launcher-menu.json catalog/modes/ranking, populated-catalog grid/selection, cold/warm launch/idle reopen and preserved inputs | **Gate D:** estimate all remaining work from measured pace; owner decision on full parity |
| 11. Dashboard | MPRIS/media, resource/calendar/system/weather cards, wallpaper/palette presentation; retained helpers and bounded artwork caches | Can rich data/views stay efficient and resilient? |
| 12. Pearls | xdg detachable managed windows, geometry/docking, ownership/focus/output mapping, backend-specific placement | Is native managed-window behavior possible without unauthorized rules/config changes? |
| 13. Wallpaper/video/effects | Static/video comparison with GPL shell-side playback boundary, GPU/copy/decode/frame sync accounting, Canvas/MultiEffect-style parity | Do media/effects erase gains or require an incompatible renderer? |
| 14. AI/sidebar and remainder | Streaming chat/markdown/input/attachments, left/extras/settings/colors/welcome/control/session and audited remaining scope | Full parity recommendation from proven behavior, not a feature checklist of stubs |

Gate D is explicitly placed **after Feature 10**, resolving the original list's
later label placement. It must forecast difficult unbuilt pearls/video/AI/input
work rather than extrapolate the easier bar features at a uniform rate.

Secure session lock, if included beyond the current lock-preview, is a separate
fail-closed protocol/safety gate before any daily-session trial. It is not silently
counted as done with a session menu or decorative lock widget.

## Measurement gates and reproducibility

Each feature records client PSS/USS, SwapPss, aggregate helper cost where relevant,
60-second idle CPU, common-boundary startup, hot-reload median/p95 and presentation
pacing where applicable. Use repeated matched workload/output/scale/cache conditions.
Record cold/warm shader/font/catalog state, versions/revision, elapsed work and
authored versus generated LOC. The full-shell 212/306 MiB reference is context;
component comparisons require equivalent QML fixtures. Missing values are N/A
with a reason; no stub metric is zero.

Keep output timestamps precise: save notification/validation/commit/first callback/
presentation are different boundaries. Cross-monitor process-atomic reload can
display on adjacent refreshes. PSS is resident process memory, not total GPU RAM.

Budgets/kill criteria are proposed in ADR 0001. A concern is surfaced immediately,
and at each owner gate results include a go/adjust/stop recommendation. A credible
negative outcome is a successful experiment.

## Safety and release boundary

Nested compositors by default; task-owned processes and exact PID/namespace stop
commands for any real-session test. Never replace/kill daily Nacre or edit installed
shell/config/bindings/autostart. Runtime/fixtures privately scoped. Nacre and Orient
remain read-only; external helpers remain Python; no duplicated palette engine.
Private main only, no PRs/publication/crates/AUR/releases. Every crate publish=false.

Current run command/status and the exact next action are in [STATUS.md](STATUS.md).
