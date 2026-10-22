# B0029 — Packaging, coexistence and rollback to QML

Status: **Review — switch tool, package recipe and rehearsal done in isolation; the live switch is an owner gate.**
Gap-list unit 10 (`docs/research/core-switch-gap-20261022.md`). Nacre side: branch
`task/b0029-nacre-side-20261022` (`tools/ui_shell_switch.py`, one `shell.qml` gate).

## Decisions

1. **Separate package, same layout rules.** Bead ships as its own Arch package
   `bead-shell` built from this repository (`packaging/arch/PKGBUILD`, in-tree
   candidate recipe; pinning a published recipe is an owner step). It is not part
   of the `nacre` package family: different licence mix, independent release
   cadence, and installing it must never change what runs. Nothing in a package
   scriptlet starts, enables or edits a unit. A user-prefix install is the same
   staging script: `packaging/stage.sh DESTDIR PREFIX`.
2. **Install layout** (all under the prefix, relocatable): `lib/bead/nacre-shell`
   and its companion `nacre-wallpaper-decode` (must sit side by side),
   `share/bead/shell/**` (the Luau bundle, minus `shell/tests`), `bin/bead-shell`
   (launcher: `start` runs the daily profile from the bundle directory against
   `$XDG_RUNTIME_DIR/nacre/daemon.sock`; `check` verifies the install with no
   display or daemon). The binary keeps its crate name inside `lib/bead`, so it
   never collides with Nacre's `~/.local/bin/nacre-shell` QML launcher.
3. **One unit, one drop-in.** `nacre-shell.service` stays the only UI unit. Bead
   mode is exactly one managed drop-in
   (`$XDG_CONFIG_HOME/systemd/user/nacre-shell.service.d/60-nacre-ui-shell.conf`)
   that replaces `ExecStart` with `bead-shell start`. The unit name, its
   `WantedBy=graphical-session.target`, the D-Bus activation file for
   `org.freedesktop.Notifications` (which starts this unit) and N0014's drop-in all
   keep working unchanged. A second always-on unit was rejected: it would need its
   own enablement and activation story and could be started beside the QML shell
   by D-Bus activation or a stale `enable`.
4. **Switch command**: `nacre ui-shell status | bead --notifications none|daemon
   [--launcher PATH] | qml | rollback` (`rollback` = `qml`). Routed through
   `releases.py` like `nacre f4`: ordinary user only, the existing `release.lock`
   held throughout, journals `0600` in `0700` directories under
   `$XDG_STATE_HOME/nacre/ui-shell/<id>/journal.json`, atomic writes with directory
   fsync, a write-ahead `phase` marker before every action, idempotent steps.
5. **Prerequisites are refused up front, nothing written**: ordinary user; N0014
   daemon-client mode on and `nacre-daemon.socket` active (Bead is daemon-only);
   current shell running; manager UnitPath contains the user unit dir; no foreign
   drop-in overriding `ExecStart`; managed files that exist carry our marker;
   launcher is a plain absolute path, root- or user-owned, not group/world
   writable, and `bead-shell check` exits 0; notification choice made; no
   incomplete journal.

## Companion QML windows (Settings, Colors, Welcome)

They live inside Nacre's single `shell.qml`, reached by `quickshell ipc -c nacre
call settingsApp open`. While Bead is active a companion unit
`nacre-qml-windows.service` runs the same QML launch command (read from the live unit,
not assumed) with `NACRE_QML_WINDOWS_ONLY=1`. `shell.qml` then skips the background,
desktop (bar, drawers, notification popups), dock guide, global shortcuts and the
UI-provider registration (`NacreShellIpc`) and keeps the headless services and the
three window hosts. The config name is unchanged, so `nacre-settings`, `nacre-welcome`
and `nacre-colors` need no change. The unit has `PartOf=nacre-shell.service` and
`ConditionPathExists=<drop-in>`: it cannot start unless Bead mode exists on disk, and
the switch stops it before the QML shell is restarted. Wanted from the drop-in only.
Not available under Bead (existing gaps, not caused here): the AI sidebar
(B0026 blocker C) and anything reached by `nacre left`.

## Notification ownership (N0017)

The switch never touches notification ownership. QML owns the bus name through its
`NotificationServer`; stopping it releases the name. `bead` therefore needs an
explicit choice: `--notifications none` (accepted knowingly: no desktop
notifications while Bead is active; D-Bus activation resolves to the already
running unit and callers get an error) or `--notifications daemon` (refused unless
`daemon.status.topics` advertises `notifications`, i.e. the N0017 managed producer
is up and owns the name). The choice is journaled and shown by `status`. Bead's own
notification service stays a private-bus preview (B0020). Returning to QML under a
daemon-owned bus is outside this switch: use N0017's own `legacy` mode first.

## Rollback and crash points

`qml` is the one-step rollback and also the recovery for every interrupted state:
remove the drop-in, reload, stop the windows companion, restart `nacre-shell.service`,
verify. Each step is idempotent and journaled; it refuses to delete a drop-in that
lacks our marker. `bead` runs the same convergence automatically when any step or
health check fails. Health for Bead: unit `running` and the same process
for 4 s, the drop-in is the loaded one, companion active, the daemon routes `ui.*`
to a registered provider (probe: a `ui.dispatch` with an unknown target must not come
back `unavailable`/`dispatched:false`), and N0014's daemon health passes. A unit that
exhausts its start limit (3 failures in 60 s) triggers
`OnFailure=nacre-ui-shell-fallback.service`, which runs `ui-shell qml --recover`
(returns at once, success, if a switch holds the lock: that switch restores QML itself).

| Interrupted at | State left | Shell running | Recovery |
| --- | --- | --- | --- |
| before `write-units` / `reload-units` | inert companion and fallback units at most | QML | `qml` (or nothing) |
| after drop-in written, before reload | disk says Bead, manager caches QML | QML | `qml` |
| after reload, before restart | manager loaded Bead, QML process still up | QML | `qml` |
| restart done, health unfinished | Bead running | Bead | `qml` |
| any `qml` phase | rest of the convergence | QML or Bead | rerun `qml` |

TTY fallback without the tool: delete the drop-in file, `systemctl --user
daemon-reload`, `systemctl --user stop nacre-qml-windows.service`,
`systemctl --user restart nacre-shell.service`.
The companion and fallback unit files stay installed after `qml` (inert by their
condition / `OnFailure` wiring); only the drop-in decides the shell.

## Threat model

- **Half-switched state**: one commit point (the drop-in's atomic rename/unlink);
  journal pending blocks another `bead`; `qml` always converges; tests crash before
  each phase and after each of the first three manager mutations, then recover.
- **Two shells fighting over layer-shell surfaces**: one unit means systemd never
  runs two UI processes; the QML windows host creates no layer surfaces except its
  own on-demand windows; its start is gated by the drop-in's presence. The test
  manager asserts "QML full shell never beside the windows host" at every mutation.
  Not covered: a QML shell or Bead started by hand outside systemd.
- **Notification ownership**: see above; no silent loss, no takeover.
- **Wrong or hostile paths**: absolute plain-character paths only, no symlink
  ancestry or group/world-writable parents, unit text is generated from validated
  values; no shell is involved in `ExecStart`.
- **Lock screen**: unchanged. hyprlock is not part of either shell (B0023).
- **Not proven by this unit**: real systemd `OnFailure`/start-limit behaviour and
  the real daemon's reply shape for the provider probe (modelled; the
  rehearsal used a modelled manager with real processes, see Evidence).

## Temporary duplicates (expiry)

- `NACRE_QML_WINDOWS_ONLY` mode and the companion unit: delete the full-shell QML
  surfaces and this mode 30 days after Bead is the trusted daily shell and the
  windows are ported; the companion stays until then.
- Old dev unit `packaging/systemd/user/bead-shell.service`: removed in this unit.
- `rollback` alias of `qml`: delete 30 days after first use of `qml`.

## Owner gates and procedure (not done here)

Install the package (or stage under a user prefix), then as the ordinary user:
`nacre ui-shell status --launcher <path>` (expect `ready_for_bead: []`), then
`nacre ui-shell bead --notifications none --launcher <path>`; return with
`nacre ui-shell qml`. Live memory and perf acceptance (25 MiB laptop cap, typing
latency) remain open gates and are independent of this unit.
