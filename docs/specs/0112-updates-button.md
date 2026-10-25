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

Private labwc→niri parity capture: 2880×1800, scale 1.5, private homes/bus, fake count 3;
updater actions were not activated. QML-left/Bead-right crops: [idle](evidence/0112/bar-idle.png),
[hover](evidence/0112/updates-hover.png), [pressed](evidence/0112/updates-pressed.png),
[open](evidence/0112/updates-open.png).

| State | Full-frame RMSE % | Bar RMSE % |
| --- | ---: | ---: |
| Idle | 2.237 | 4.969 |
| Hover | 2.746 | 5.144 |
| Pressed | 14.121 | 8.418 |
| Open | 14.011 | 12.701 |

Full Bead CI-equivalent passed on pushed `bf1f097`; GitHub Rust checks run 40174548582 passed.

## Known gap

QML's `Updates.message` can show the Qt preflight warning. The published `dashboard.updates`
contract has no preflight-message field, so Bead shows the count and leaves the message line
empty; adding that warning waits for a published schema field. QML also offers “Full panel”
and aligns its wider dropdown to the right edge; Bead's status popout omits that control and
centres on the bar item. The capture records this remaining placement/affordance difference.
