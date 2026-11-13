# 0250 — Popup panels for folder apps

A `[layer]` folder app (a bar) may declare named popup panels: a calendar under the clock, a volume slider under the speaker, a network list. Panels are Luau only and reuse bead-wayland's existing one-resident-panel machinery; no new surface system is added.

## Manifest

```toml
[panels.calendar]
view = "view/calendar.luau"   # relative, inside the app folder; sibling behavior.luau and bundle.json apply
size = [320, 240]             # fixed logical size, both > 0
```

Rules: `[panels]` requires `[layer]` (a window app has no bar to hang popups from); names follow the framework catalog rule (1–64 chars of `a-z`, `0-9`, `-`, `_`, `.`; at most 32 panels); unknown keys are rejected; `view` must be relative and stay inside the folder. Errors (prefixed by the manifest path): `panels require a [layer] surface`, `panel 'NAME' size must be positive`, the path errors of spec 0234, and the catalog message from `Panels::new`. `bead check` loads and CPU-renders every panel view at its declared size after the main view, with the same empty service snapshot.

## Opening, toggling, closing

Views send ordinary actions; when `[panels]` is declared, FolderView adds these to the host-action allowlist so UI validation accepts them without behavior handlers:

| Action | Argument | Effect |
|---|---|---|
| `panel.open` | panel name (Text) | Open that panel; replaces whichever panel is open. |
| `panel.toggle` | panel name (Text) | Close it if it is the open panel, else open it. |
| `panel.close` | none | Close the open panel (from the bar or from inside the panel). |

FolderView routes every declared host action (`desktop.*`, `command.*`, `panel.*`) in one place: a click, release, drag, Enter/Space or Left/Right range step updates focus and press state only, and the action goes to the host, never to `behavior.luau`; a host-owned slider sends its value at the pointer.

The bar view attaches its last pointer position along the bar axis (x for top/bottom, y for left/right, in bar-surface coordinates, which are output coordinates for a full-edge bar) as the panel centre, so a panel opens under the element that was clicked. Panel views pass no centre. Unknown names are reported as `COMPANION_ERROR panel view is not configured` at runtime; `bead check` does not validate action arguments.

## Placement, focus, dismissal

Each panel is a `PanelSpec` in `Options.panels` (`Panels::new(first_name, catalog)`), with `Options.panel_spec` seeded to the first entry. Per panel: `factory` loads a `FolderView` of the panel view at the declared size; `fallback = empty_scene(size)` fixes the surface size before the first frame; `position` is a `PanelPositioner` that places the panel flush against the bar's inner edge at `centre − extent/2`, clamped inside the output (bar edge and thickness come from `[layer]`); `keyboard_focus = true` (exclusive keyboard while open, the existing outside-click dismissal layer, Escape closes); `retain_when_closed = true` (the worker stays warm after its first open, so reopening does not rebuild the Luau generation); `renderer = Options.renderer`; `overlay`, `passive`, `text_focus_only`, `engages_decorations`, `initial_open` are false; `fallback_factory` is `None`.

Existing fields used: `Options.panels`, `Options.panel_spec`, `Options.renderer`; `PanelSpec.{factory, fallback, position, keyboard_focus, retain_when_closed, renderer}`; events `UiEvent::CompanionSelect { view, output: None, toggle, center }` and `UiEvent::CompanionVisibility(false)`, emitted through the source's `services` wakeup. The wayland runtime already routes those from an output session (the bar) and from the panel session to the host-level panel controller.

## State sharing and lifecycle

Panels and the bar share one `FolderServicesRuntime` (declared services, commands, snapshots) and the same palette; each panel is its own `FolderView`/`Session` on that runtime, so stores it declares are its own. There is exactly one resident panel: opening another replaces it (existing lifecycle). Creation is lazy on first open; closing retains only the current panel, and switching names replaces its worker. Panel views keep their declared viewport when the controller supplies output dimensions. Panels open on the output of the bar that sent the action (`output: None` resolves to the sending session's output); a request from inside a panel keeps the panel's output.

## Out of scope

Several simultaneously open panels; pearls (detaching); panels for `[window]` apps; hover-open and pre-warming on hover; per-panel focus or overlay options; anchoring to an element rectangle (the pointer centre suffices for a bar); panel-specific validation of action arguments in `bead check`.

## Evidence

This is a neutral framework example, with no corresponding product surface for QML parity.

Ubuntu 24.04 CI replica of `ci.yml` on origin/main def278e6 plus this change: formatting, framework and service boundaries, warnings-denied workspace Clippy, workspace tests (636 passed, 15 opt-in ignored), all renderer feature configurations, both minimal-host proofs, `bead check` on all five folder apps, the private UPower/NM/BlueZ, tray and clock proofs, and cargo-deny for all 13 roots. Regressions cover manifest validation, placement, the fixed panel viewport, action declaration, pointer, keyboard and slider routing of host actions, and panel-originated output selection.

Headless Sway proof (1280×720, pixman, private session bus, a virtual pointer from bead-wayland's `private-pointer` example; screenshots inspected): the bar; the clock opens the calendar flush under the bar and centred on the clock; the volume button replaces it with the volume panel clamped to the right edge; an outside click closes it; the clock reopens it; Escape closes it; the panel's Close button closes it; the log has no UI, companion or output-session errors. Raw logs and screenshots stay untracked.

Memory: an app without `[panels]` pays nothing. After its first open, a panel keeps one `FolderView` worker warm so reopening is instant (smoothness first). This cost has not been measured yet: no desktop test host was available for this unit.
