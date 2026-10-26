# B0151 — Widgets finish slice

B0151 completes B0114's flexible bar forms, desktop resize, and detachable app panels.
Occupied bar slots snap into their pill instead of rejecting the canonical write.
The target slot is highlighted, neighbours reflow with the existing short easing, and settle/return use a soft overshoot.
The desktop forms remain in scope only for the existing clock/bar-growth regressions; B0152 owns widget design polish.

| Proof | Result |
| --- | --- |
| Desktop clock drop at 82% and 90% | Remote nested run pending |
| Six-state drag, app, resize, pearls, focus | Remote nested run pending |
| Bar parity against unchanged default | Remote CI/capture pending |
| Full Bead CI and widget memory budget | Remote CI/capture pending |

The pill fallback detects slot overlap after geometry hit testing, covering a drag anchor just outside the pill when the snapped span overlaps it. The generalized `out-back` easing is recorded in ADR 0004. No temporary duplicate paths were added.
