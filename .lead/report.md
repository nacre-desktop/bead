# B0172 report

Status: done; pushed to `main`.

Root cause: the old app used a transparent full-stage drag source appended above the bar and desktop previews, so it intercepted both hit areas and carried the selected form's payload. The live nested trace found a second drop blocker: fallback screen geometry had an empty output ID, which failed the terminal-drop guard. Both are fixed and covered by `acceptance.json`.

1. **Standalone app:** `Nacre Widgets` / `nacre-widgets`; Settings shortcut removed. Evidence: `app-home.png`, Settings goldens.
2. **Drag and actions:** app launched with `nacre-shell --app widgets` through the parent socket; synthetic pointer drops to bar and desktop, desktop move, four add/remove clicks, and both close/reopen hashes passed. Evidence: `acceptance.json`, `drag-in-progress.png`, `desktop-edit-mode.png`, `desktop-move-edit-mode.png`, `app-to-bar.png`, `desktop-reopened.png`, `bar-reopened.png`.
3. **Window size:** default 1100×800 logical, capped at 70% output; last size persists. Evidence: `app-home.png`, `acceptance.json`.
4. **Miniatures:** style cards keep aspect and hit targets while scrolling. Evidence: `style-tiles.png`; restored pointer test passes.
5. **Stable sidebar:** same widget list on overview and detail. Evidence: `app-home.png`, `battery-page.png`.
6. **Icon alignment:** centered sidebar icons and labels. Evidence: `app-home.png`, `battery-page.png`.
7. **Palette roles:** widget forms use palette roles. Evidence: four `colour-*.png` captures.
8. **Per-instance color:** Primary/Secondary/Tertiary/Neutral saves on the widget instance. Evidence: `acceptance.json`; model/UI regression tests pass.
9. **Descriptions:** concise translated descriptions fit the list and card. Evidence: `app-home.png`, `battery-page.png`.
10. **Switch rim:** removed the shared track outline; 16 Settings and 12 status-view goldens refreshed. Evidence: `toggle-close-up-4x.png`.

Visual comparison notes against `.lead/reference/App.dc.html` are in `docs/specs/evidence/0172/README.md`. The test host lacks `swaybg`; the app stage uses a private wallpaper fixture, and the outer desktop is a solid compositor background.

Checks: focused style-hit and unnamed-output fallback tests passed; Settings golden suite 47/47; status-view golden suite 4/4. Full Bead CI on dev-2, container `b0172-widgets-app-20261028-final-ci4`, exited 0; `acceptance.json` records the recipe and result.

Product source lines: 453 added, 127 removed. Fifteen task commits including this report use `[no-publish]`, author/committer `siverteh <92440237+Siverteh@users.noreply.github.com>`, and the Codex Luna trailer.
