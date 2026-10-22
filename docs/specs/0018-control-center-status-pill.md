# B0018 — Control-center status pill routing

Status: In progress. Requested as B0017; 0017 already names network connect
controls, so this unit uses the next free number, 0018.

## Objective

Match Nacre's settled status-pill interaction: one tightly and evenly spaced
top-right pill, quick hover information, and compact right-edge device panels.
Use the existing Network, Bluetooth, Battery and Audio views and published
contracts. Framework behavior stays generic; shell theme owns appearance.

## Behaviour

| Event/state | Result |
| --- | --- |
| Hover an icon with no clicked panel | Show that item's small quick-info peek, using reference QML timings. |
| Click an icon | Close any peek and open its compact control-panel-sized panel from the right edge. |
| Hover other icons while clicked open | Keep pointer cursor/highlight; suppress every peek. |
| Click another icon while open | Switch panel content to that icon. |
| Pointer leaves the active pill/panel interaction region | Close using reference QML pointer-away semantics/timing; crossing into the panel remains possible. |
| Escape | Close peek/panel and cancel pending hover/open/close work. |
| Panel opens | Hide the edge charge line immediately. |
| Closed and idle | Release lazy panel resources using existing lifecycle policy. |

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
5. Full `.github/workflows/ci.yml` equivalent in an Arch CI container on dev-1:
   fmt, warnings-denied clippy, workspace/trace tests, renderer configurations,
   facade/headless/Luau schema, capture regressions, private services/tray and
   dependency/license checks. Fix in-scope failures and review the final diff.

No tests/builds on the laptop. Cargo -j2, nice 10; remote work in task-private
directories/tmux. Desktop nested checks hold the desktop-heavy flock and use
private HOME/session; all capture gates apply if capturing. No push. Stop for
owner-only decisions or unavailable external prerequisites, with exact evidence.
