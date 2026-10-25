# B0112 — Updates button opens the updater controls

The QML button opens the `updates` control popout; it does not start an upgrade directly.
The popout shows the current count and message, a refresh action, and an Open updater action.
Hover requests the existing updates peek. QML accepts only primary-button activation; middle/right clicks do nothing.
Bead's top-bar button now opens the matching status popout, which is loaded lazily from the kit view.

## Source and routes

QML reference: Nacre `origin/main` at `713d38b3a0421b42da995f6cc562650510807e88`.
`NacreHeader.qml` calls `openStatus("updates", ...)` and `requestPeek("updates", ...)`.
`NacreControlUpdates.qml` displays `Updates.count`/`Updates.message`, calls `Updates.refresh()`,
and opens `nacre-shell updates`; `updates.sh` keeps the Qt preflight and package flow unchanged.
The Bead panel leases `dashboard.updates`, refreshes through `dashboard.updates.refresh`,
and launches the existing updater via `tools.launch` with the fixed `updates` tool id.
`nacre controls updates` now maps to the Bead status panel.

## Validation

Fake topic projection and route-mapping tests assert the published update values and fixed calls.
Tests never execute `tools.launch`, `nacre-shell updates`, `updates.sh`, or any package command.
No Nacre source or update command changes; no dependencies or temporary duplicate implementations.
Paired QML/Bead captures and full Bead CI results are recorded here after remote validation.

## Known gap

QML's `Updates.message` can show the Qt preflight warning. The published `dashboard.updates`
contract has no preflight-message field, so Bead shows the count and leaves the message line
empty; adding that warning waits for a published schema field.
