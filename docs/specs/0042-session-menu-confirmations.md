# B0042 — Session menu: inline confirmations and Suspend

Status: **Review.** Owner decision 2026-10-23: Bead keeps its own session menu behavior (confirmations + Suspend) instead of strictly matching the QML, which has neither.
Builds on B0023; the Nacre route is unchanged.

## Behavior

- **Suspend** is a published route: `session.request` v1 `{action:"suspend", confirmed:true}` (N0031). No Nacre contract ask is needed. The whole menu stays disabled (entries carry no action) while the Welcome does not advertise `session.request` v1, so Suspend is not usable on a daemon without the route. Suspend asks nothing (reversible) but sends `confirmed:true`, which the daemon requires for every non-lock action. This supersedes B0023's "suspend confirms".
- **Log out, Restart, Power off** confirm inline: the activated item turns into "Confirm log out / restart / power off" (error colour, same row, same glyph) and takes focus. Enter or a click confirms and only then sends the call (with `confirmed:true`). Esc, 4 s without an answer (`CONFIRM_TIMEOUT`), selecting another entry, or losing the daemon link reverts it; nothing is sent. A late Enter after the deadline is inert. Esc returns focus to the item. Lock acts at once (no flag).
- Accessibility: the confirm button's accessible name is the same translated "Confirm ..." text; every entry keeps its translated label as `aria_label`.
- i18n: `bead.panel.session-confirm-{logout,reboot,shutdown}` became the short inline labels; `bead.common.confirm` and `session-confirm-suspend` were removed (unused). English is the only catalog today.

## Checks

Fake-daemon tests (`session_menu.rs`, `reload/session_menu_runtime_tests.rs`): no call before confirmation for logout/reboot/shutdown, only confirm sends, switching item replaces the prompt, suspend and lock send immediately with the right params, timeout reverts and a late confirm is inert, Enter via keyboard confirms, Esc cancels, labels. Goldens `shell/tests/goldens/session-{idle,confirm,disabled}-{1,1.5}.png`.

## Size and duplicates

Product code about +60 lines (session_menu.rs, reload.rs, Luau); the separate question + Confirm + Cancel buttons were removed. No temporary duplicates.
