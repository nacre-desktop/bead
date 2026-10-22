# B0023 — Session menu and fail-closed lock hand-off

Status: **Review — contract published (Nacre N0031 `session.request`, pinned `652d6c4f`); schema aligned by B0022.**
Gap-list unit 8 (`docs/research/core-switch-gap-20261022.md`).

## Decision

Bead does not implement a session lock. Locking hands off to the lock the
daily shell already uses (hypridle `lock_cmd`, hyprlock, reached by
`loginctl lock-session`), through a daemon route. Bead only requests it and
fails closed: a lock is never presented as done unless the daemon says so.
A native `ext-session-lock` surface is a later, separately reviewed unit.

## QML reference (Nacre `origin/main`, `NacreSessionControls.qml`)

Right-side panel, 150 px wide, 180 ms slide, Escape/outside closes. Entries:
Lock (`loginctl lock-session`), Log out (compositor logout), Restart
(`systemctl reboot`), Power off (`systemctl poweroff`). QML has **no
confirmation, no timeout and no Suspend**. The only published confirmation
text is the launcher `power.reboot` row, "Restart the computer?".
Bead differences, deliberate: adds Suspend; Log out, Suspend, Restart and Power
off ask once inline (no timer, Escape or Cancel dismisses); Lock is immediate. The 0.18 s slide matches. Open owner question: keep the extra
confirmations?

## Contract (published by Nacre N0031)

Bead calls `session.request` v1 `{action: lock|logout|suspend|reboot|shutdown,
confirmed: bool}` only while the Welcome advertises exactly that method.
The reply is `{changed:true}` for every action (it records accepted execution:
`loginctl lock-session` exit 0, compositor logout, `systemctl` exit 0). The
daemon refuses every action except lock unless `confirmed:true`, so Bead asks
once inline for log out, suspend, restart and power off (suspend now confirms
too) and sends `confirmed:true` only after that step; lock sends no
`confirmed`. Any other outcome (error, timeout, disconnect, `changed:false`,
wrong or missing field, non-object) is a failure. Daemon error text is never
shown. The earlier assumed `locked`/`accepted` fields and the unconfirmed
suspend were wrong against the published schema and are removed.

## Threat model

- Lock bypass / false assurance: Bead keeps no "locked" state and shows none;
  the panel closes only on the explicit `locked:true`. Unconfirmed lock shows
  "Screen lock was not confirmed. Lock the screen another way." and stays open.
  No automatic retry, no local fallback lock, no hiding of surfaces as a stand-in.
- Request while locked / locked-screen input: the lock surface owns input via
  the compositor's session-lock protocol; the daemon decides whether to accept.
  Bead assumes this and does not claim to enforce it.
- Crash/restart during lock: Bead is stateless about locking. The lock lives in
  hyprlock/the compositor, so a Bead crash neither locks nor unlocks. A restarted
  panel starts idle. A dropped pending call is "unconfirmed", not success; a
  dispatched call is never replayed (existing ticket guard: boot + method v1 +
  transport generation).
- Compositor without hyprlock: the daemon returns an error, shown as unconfirmed.
  The method is absent entirely on an old daemon, which disables the menu.
- Multi-output: lock coverage of every output is the lock client's and
  compositor's job. Bead draws nothing lock-related per output.
- Double or racing requests: one pending call per panel; select/confirm are
  ignored while pending. Losing the link drops an unconfirmed confirmation prompt.

## Implementation

`crates/nacre-shell/src/session_menu.rs` (state machine, `verdict`, tests),
`popouts::session`, `Runtime::with_session_menu` (reload.rs), `shell/session/*`,
bar power glyph now opens the "session" view (only with a daemon client; without
one the button does nothing). Reuses `Client::begin_call` tickets. No change to
any `bead-*` crate, so ADR 0004 holds and the Generalization list gains nothing.

## Acceptance and non-goals

Unit tests: fail-closed verdict table, confirmation policy, disabled-without-
capability sends nothing, confirmed lock closes, unconfirmed lock (bad field,
false, error) never closes and never retries, confirm/cancel/busy behavior.
Full Bead CI equivalent on dev-2; nested smoke checks the panel opens disabled.
No real lock, logout, suspend or reboot is ever invoked. No live switch.

## Duplicates

None.

## Blocker

None in Bead. The menu is disabled against a daemon that does not advertise
`session.request` v1.
