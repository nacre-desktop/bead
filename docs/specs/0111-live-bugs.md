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

## Sidebar

The external app route lost the hover-open origin, and panel pointer-region
notifications went only to the bar runtime. The sidebar now receives those
notifications and uses Luau-defined transient/pin/detached rules with QML's
130 ms leave grace. Explicit click/key opens stay open; close/Escape clears pin.
The lip and body are separate regions: either cancels the pending close.
An ordered, small-stack adapter lane waits for the cold app's prepared scene
before delivering queued lip observations. This avoids dropping a leave during
startup or letting it overwrite a later body enter.

Regression checks cover explicit opens, no-visit lip departure, reentry, pin and
unpin, detached panels, close resetting pin, and both startup ordering races.
Static layout is unchanged. No daemon schema changes or extra provider are added.

## Remaining acceptance work

Tab alignment at scale 1.5 and right-pill peek motion are under investigation.
Paired QML/Bead evidence, daemon-mode reproduction, full CI, goldens and the final
verdict will be added before closing this unit.
