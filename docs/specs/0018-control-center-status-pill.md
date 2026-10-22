# B0018 — Control-center status pill routing

Status: Done — private/nested proof and full CI equivalent passed.
Requested as B0017; 0017 already names network connect controls, so this unit
uses the next free number, 0018.

## Objective

Match Nacre's settled status-pill interaction: one tightly and evenly spaced
top-right pill, quick hover information, and compact right-edge device panels.
Use the existing Network, Bluetooth, Battery and Audio views and published
contracts. Framework behavior stays generic; shell theme owns appearance.

## Behaviour

| Event/state | Result |
| --- | --- |
| Hover an icon with no clicked panel | Show that item's small quick-info peek, using reference QML timings; pointer-interactive, without keyboard focus or an outside-click shield. |
| Click an icon | Close any peek and open its compact control-panel-sized panel from the right edge. |
| Hover other icons while clicked open | Keep pointer cursor/highlight; suppress every peek. |
| Click another icon while open | Switch panel content to that icon. |
| Pointer leaves the active pill/panel interaction region | Close using reference QML pointer-away semantics/timing; crossing into the panel remains possible. |
| Escape | Close peek/panel and cancel pending hover/open/close work. |
| Panel opens | Hide the edge charge line immediately. |
| Closed and idle | Release lazy panel resources using existing lifecycle policy. |

Reference: Nacre `origin/main` at `9331b3d`,
`nacre/shell/modules/topbar/NacreHeader.qml`,
`nacre/shell/widgets/NacreHoverDwell.qml` (350 ms dwell), and
`nacre/shell/modules/drawers/NacrePanelInput.qml`: peek expiry 120 ms;
clicked pointer-owned controls allow 600 ms for the first header-to-panel
crossing, then 130 ms after entering the panel. Returning to the header resets
the initial-crossing state. Compact controls use a 280 ms OutCubic opening in
`NacreControlCenter.qml`. Read these from Nacre `origin/main`, not its checkout.
`bar/popouts/NacrePopupPanel.qml` uses a 230 ms InOutCubic peek reveal;
clicked controls use the 280 ms OutCubic reveal above.
`drawers/NacrePanelHost.qml` places compact controls at the usable top-right:
the body touches the header and right frame, independently of the clicked icon.

The reference right-edge charge cue is `NacreFrameLips.qml` / `NacreEdgeTab`.
Bead has no edge-lip charge graphic in this baseline; this unit must keep it
absent while a panel opens, and must not substitute a pill-top graphic or claim
edge-hover gesture parity. Building edge activation remains outside this unit.

## Scope and non-goals

Implement reusable routing/state transitions in the kit/runtime and shell wiring.
Preserve tray interaction, existing guarded actions, capability checks and lazy
leases. No new device controls beyond existing schemas, providers, protocol
changes, full control-center pages, live switch, install or publication.
Record discovered framework Nacre assumptions in ROADMAP's Generalization list.

## Phases and acceptance

1. Spec/queue and QML reference review, before code.
2. Generic state machine and thorough transition/timer/cancellation tests.
3. Shell pill, peek and right-edge panel wiring, existing view contents and
   immediate charge-line suppression. Commit implementation phases.
4. Private nested interaction smoke covering hover, click, suppression, switch,
   panel crossing, pointer-away, Escape and reopen. Match B0016's native-input
   proof style; record exact source/binary identity and owned cleanup.
5. Full `.github/workflows/ci.yml` equivalent in a remote Arch CI container:
   fmt, warnings-denied clippy, workspace/trace tests, renderer configurations,
   facade/headless/Luau schema, capture regressions, private services/tray and
   dependency/license checks. Fix in-scope failures and review the final diff.

No tests/builds on the laptop. Cargo -j2, nice 10; remote work in task-private
directories/tmux. Desktop nested checks hold the desktop-heavy flock and use
private HOME/session; all capture gates apply if capturing. No push. Stop for
owner-only decisions or unavailable external prerequisites, with exact evidence.

## Verification — 2026-10-22

Final code `c6ba4b3`; source archive SHA-256
`6eb292d70887eb7dd4b67101387777c7c8843e4780d5f0caf72fb995d29bf7d2`.
The complete `.github/workflows/ci.yml` equivalent passed in the remote Arch
container: fmt, warnings-denied clippy, boundary, workspace/trace tests,
renderer configurations, facade/Luau (58 modules), capture regressions,
private services/tray, and workspace plus independent dependency/license graphs.
The final workspace run includes 186 passing shell tests; routing has 17
deterministic machine tests and three native input-bound tests. Existing QML
placement and status goldens pass unchanged. The container needed `ttf-dejavu`
for native font matching; no host package or production dependency was added.

Exact-source nested Labwc CPU input passed four peeks, a fully revealed peek
followed by native same-icon click, suppression and three switches, panel
crossing, pointer-away close, idle release and Escape. Icon centers are 26 logical
pixels apart; the clicked body is 400×286 at the usable top-right. Peeks have
neither keyboard focus nor dismissal shield. Source/binary identity, private
bus/HOME isolation, host prechecks, and owned-process cleanup were recorded.
The debug CPU binaries had DWARF sections stripped for transfer; shell SHA-256
`909406c2a2efe6439a6750d9ac46d1d1db36bf62c081b5040c04c1ec00d2f69f`.

The native proof caught and fixed a warmed nonmodal preview with no surface;
companion input now excludes shadow padding and stays within its final content
footprint during animation. One full-run watchdog deadline failed; its isolated
retry and the unchanged complete rerun passed. Cause is unconfirmed and the
deadline was not changed. No live, producer availability, matched timing or
memory acceptance is claimed. No push or installation was performed.
