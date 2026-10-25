# B0111 — Daily-use sidebar, tabs and notification bugs

Work in progress. Private nested reproduction and full CI are being collected.
No live shell, installed configuration or daemon contract changes are included.

## Notifications

The UI-provider holds `notifications` as a permanent subscription. Both the
notification popup and control-center factory tried to lease it again;
`lease_topic` rejects permanent topics, and `filter_map(...ok())` silently
removed the source. The view therefore had no history and stayed on Loading.
The adapter now shares a permanent source or acquires a scoped lease when absent.
Launcher Quick uses the same acquisition rule (its previous rule was already correct).

The daemon-transport regression includes existing stored history, readiness,
count, clear availability, close/reopen and keeping the permanent subscription
while releasing only the view's controls lease. It and the existing permanent
Launcher Quick subscription regression passed in the private CI container.

## Remaining acceptance work

Sidebar hover/pin lifecycle, tab alignment at scale 1.5 and right-pill peek motion
are under investigation. Paired QML/Bead evidence, daemon-mode reproduction,
full CI, goldens and the final verdict will be added before closing this unit.
