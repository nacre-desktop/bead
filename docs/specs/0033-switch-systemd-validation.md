# B0033 — `nacre ui-shell` validated under a real systemd user manager

Status: **Done — validation record.** 71/71 checks pass after three Nacre fixes found by this unit.
Nacre side: branch `task/b0033-switch-fixes-20261023` (three commits, listed below). Follows B0029
(`0029-packaging-coexistence-rollback.md`), whose rehearsal used a *modelled* supervisor.

## Goal

B0029 could not prove how real systemd behaves (`OnFailure=`, start limits, `PartOf=`, drop-in model,
the companion unit, interaction with `nacre-daemon.service` and the F4 drop-in). This unit repeats the
switch, the rollback and the failure paths against a real PID 1 and a real `user@1000` manager, and fixes
what that exposes. Nothing ran on the owner's laptop; nothing was installed anywhere but a throwaway
container.

## Environment (disposable, removed afterwards)

- Docker container on a build host (`--privileged --cgroupns=private`, 12 CPU, 12 GiB), image
  `archlinux:latest` + the Nacre CI package list + `labwc`; **systemd 262.1 as PID 1**, `dbus-broker 37`,
  lingering test user `tester` (uid 1000) so `user@1000.service` is a real user manager.
- Session: `b33-session.target` (`BindsTo=graphical-session.target`) starts headless `labwc`
  (`WLR_BACKENDS=headless`, pixman) and a nested `niri 26.04`; the compositor variables are imported into
  the manager like a real login does. Software GL, no GPU, no PipeWire, no network.
- Installed in the test user's HOME as on a source install: Nacre `nacre-helper` (release build, shell
  QML source, `control.sh`/`launch.sh`/`cli-bridge.sh`, `nacre-shell.service`, `nacre-daemon.socket` and
  `.service` copied from the repository, `nacre-daemon.socket` enabled, the D-Bus activation file for
  `org.freedesktop.Notifications`), then `nacre f4 enable` (daemon client mode, the real F4 tool). Real
  quickshell 0.3.2 runs the real QML shell. The packaged Bead is `makepkg` of `packaging/arch/PKGBUILD`,
  installed with `pacman -U` to `/usr/bin/bead-shell`.
- Nacre = `origin/main` (N0037 era) + the three fix commits; Bead = this repository. The Nacre tools ran
  from `~/src/nacre-src/tools/releases.py` (so the fallback unit's `ExecStart` is that path).
- Not the full `install.py`: it needs a Hyprland profile, fonts, Dolphin/Thunar, power-key units and more.
  The hand-staged layout copies the same files to the same places (units verbatim from the repo).
- Files: `scripts/systemd-validation/` (`Dockerfile`, `build.sh`, `install.sh`, `fixtures.sh`,
  `crash_switch.py`, `validate.py`, `run.sh`). `run.sh up` + `run.sh test` reproduce the run; the sources
  are supplied as `nacre.tar`, `orient.tar`, `bead.tar`. `run.sh build` was run piecewise, not end to end.

## Method

`validate.py` runs inside the container as the test user and drives the **real** `releases.py ui-shell`
and the real `systemctl --user`. A sampler thread classifies processes every 0.2 s (QML full shell =
`quickshell -c nacre -n` without `NACRE_QML_WINDOWS_ONLY`, QML windows host = same with it, Bead =
`nacre-shell --ui-provider`) and fails any sample with more than one UI provider process, or a full QML
shell beside the windows host. "Exactly one provider" is also checked through the daemon
(`ui.dispatch` probe: `UI provider is unavailable` = none) at every settled state. Switch interruption is a
real `SIGKILL` of the switch process just before/after each journaled phase (`crash_switch.py` hooks the
tool's own `phase` callback); recovery is always plain `ui-shell qml`.

## Results

71 checks, all passing in the final run on the fixed code (`results.json` from the run is summarised here).
Where a check first failed, the failing result and the fix are in "Defects found".

| Area | Checks (ids) | Result |
| --- | --- | --- |
| Baseline QML, `status`, `systemd-analyze --user verify`, QML owns Notifications | S0a-d | PASS |
| `bead --notifications none`: real restart, health, companion, layers are Bead's, `status`, verify clean, nobody owns the bus | S2a-f | PASS |
| Companion stays up 50 s with no restart (regression for defect 3) | S2m | PASS |
| `PartOf`: shell restart restarts the companion; shell stop stops it; D-Bus activation with the shell stopped starts Bead + companion; activation with Bead running errors for the caller and changes nothing | S2g, S2h, S2i, S2j | PASS |
| SIGKILL of the companion: revived by `Restart=on-failure`, Bead untouched; 8 kills in a row: Bead stays, `qml` still converges | S2k, S12 | PASS |
| Repeat `bead` idempotent; `qml` one-step rollback; repeat `qml` no-op; `rollback` alias; companion cannot start in QML mode (`ConditionPathExists`); QML owns the bus again | S2l, S3a-f | PASS |
| Provider probe: true with a provider, false with none | S1a | PASS (after fix 1) |
| Bead that dies at start; Bead that stays up but never registers: both rolled back to QML by the tool | S4a, S4b | PASS (S4b after fix 1) |
| One SIGKILL: systemd restarts Bead, no fallback | S5a | PASS (after fix 2) |
| Crash loop: start limit trips, real `OnFailure=` runs the fallback, QML restored, companion stopped, fallback unit `Result=success`, journal clean, never two providers | S5b-f | PASS |
| SIGKILL of the `bead` switch before/after each of its 6 phases, then `qml` | S6-bead-* (12) | PASS, `bead` refused while the journal is incomplete |
| SIGKILL of the `qml` switch before/after each of its 5 phases, then `qml` | S6-qml-* (10) | PASS |
| `qml --recover` while a switch holds the lock (stopped with SIGSTOP): returns `busy` at once | S7a, S7b | PASS |
| Daemon `restart`, `SIGKILL`, stop + socket activation with Bead active: provider re-registers, Bead pid and companion untouched, `bead` health re-run passes, never two providers | S8a-f | PASS |
| F4 drop-in coexists with the Bead drop-in; `f4 disable` refuses; `f4 enable` idempotent; `qml` afterwards healthy | S9a-d | PASS |
| Session stop/start with the drop-in on disk: nothing left, then Bead + companion | S10 | PASS |
| Observations (below) | S5g, S11 | PASS (as observations) |

States left by each interruption (from S6; all converged by one `qml`):

| Interrupted | State left | Shell running |
| --- | --- | --- |
| `bead` before/after `write-units`, `reload-units`, before `write-dropin` | inert units or nothing | QML |
| `bead` after `write-dropin`, before/after `reload`, before `restart-shell` | drop-in on disk, manager reloaded (after `reload`) | QML (old process) |
| `bead` after `restart-shell`, before/after `health` | Bead + companion running | Bead |
| `qml` before `remove-dropin` | unchanged | Bead |
| `qml` after `remove-dropin`, before/after `reload`, before `stop-windows` | drop-in gone, manager may still cache Bead | Bead |
| `qml` after `stop-windows`, before `restart-shell` | companion stopped | Bead (alone) |
| `qml` after `restart-shell`, before/after `health` | converged but journal open | QML |

At no sampled instant in any scenario were two UI provider processes running, and the QML full shell never
ran beside the windows host.

## Defects found and fixed (all three are in Nacre)

1. **Health probe never detected "no provider"** (`tools/ui_shell_switch.py`, `provider_ready`). The real
   `nacrectl call ui.dispatch` prints plain text on stderr, `UI provider is unavailable` (daemon hub) or
   `UI method is unavailable` (a provider answered the unknown probe target), or `Nacre daemon is
   unavailable`. The check looked for JSON markers (`"dispatched":false`) that never appear, so any
   non-empty output counted as "provider registered". Proven by S4b: a Bead stand-in that stayed up but
   never registered passed health and `bead` reported success, leaving a desktop without a UI provider.
   Fixed (Nacre commit "provider probe now recognises the real output"): the three real messages are recognised; the test manager now emits the real
   CLI output; new unit test for provider / none / daemon down.
2. **`OnFailure=` fires on every crash** (systemd 262), not only when the start limit is exhausted, so one
   `SIGKILL` of Bead ran `ui-shell qml --recover` at once and dropped the user to QML while a restart was
   still scheduled. `systemd` logs `Failed with result 'signal'` / `Triggering OnFailure=` / `Scheduled
   restart job` in that order. B0029 designed for "3 failures in 60 s". Fixed (Nacre commit "OnFailure fallback acts only after the start limit is exhausted"):
   `qml --recover` returns `{"status": "ignored"}` unless the shell unit's `Result` is `start-limit-hit`.
   S5a (one kill keeps Bead) and S5b (four kills reach the fallback) now both hold. Unit test added.
3. **The QML windows companion restarted every ~22 s** while Bead was active. `shell-supervisor` decides the
   QML host is ready when `quickshell -c nacre ipc call nacre state` answers; in windows-only mode that IPC
   target does not exist by design, so after 20 s the supervisor terminated the host and exited 1 and
   systemd restarted it. Any open Settings/Colors/Welcome window would have been killed. B0029's modelled
   supervisor could not show this. Fixed (Nacre commit "windows-only QML host health checks a window host", `crates/nacre-helper/src/shell_supervisor.rs`,
   about 15 lines of code plus two tests): with `NACRE_QML_WINDOWS_ONLY=1` health asks `settingsApp state`.
   S2m now checks 50 s without a restart. Two Rust unit tests with a fake `qs`.

Two intermittent failures during development were harness-side or by design, not tool defects: process
miscounts (the supervisor's `quickshell --version` / `ipc call` helpers and `bead-shell check` briefly look
like a second shell, so the sampler now matches the exact command lines) and Bead's own safe mode (below).

## Observations (not fixed; for the owner or follow-up units)

- **Bead safe mode after repeated unclean runs.** Bead records unclean runs in
  `$XDG_STATE_HOME/bead/ui-health.json`; three within 120 s make its next start a safe-mode bar that does
  not register as UI provider (`RECOVERY code=safe_mode`). After a crash loop or fallback, `ui-shell bead`
  within about two minutes therefore fails health and rolls back to QML with the generic message
  `UI provider did not register`. That is the correct fail-safe, but the message should mention safe mode
  (S5g). The harness clears the file between scenarios, stated here so the results are not read as
  "Bead is always healthy after SIGKILL".
- **dbus-broker keeps a per-name activation pending.** With Bead running and `--notifications none`, the
  first activation of `org.freedesktop.Notifications` (unit already active, name never taken) leaves the
  broker waiting; a later activation, after the shell is stopped, does nothing until the bus configuration
  is reloaded. Harmless for the switch (the unit is started by systemd anyway) but a notification sender
  will wait for its timeout. S2j reloads the broker config first and says so.
- **`--notifications none` really silences the bus** (S2f/S2g): callers get a timeout.
- **A second shell started by hand beside QML draws too.** The daemon keeps one UI provider (the QML shell
  keeps answering `ui nacre state`, S11), but a hand-started `bead-shell start` still creates surfaces.
  The single-unit design prevents this only for systemd/D-Bus-started shells; unchanged from B0029's
  threat model.
- **`release.lock` must already exist.** `ui-shell` opens it read-only without creating it; `releases.py
  deploy` creates it. On an install that never ran `deploy` the first `ui-shell` call fails with
  `FileNotFoundError`. The harness creates it like `deploy` does.
- **`f4 disable` is refused while the daemon unit exists independently** (`Independent baseline daemon
  owner: nacre-daemon.service`), so F4's own rollback is not exercised here; it never touches Bead's
  drop-in.
- **`nacre-session-watch` and the daemon's session policy** also call `ipc call nacre state` and get an
  empty answer while Bead is active (windows-only QML). Not changed here; whether lock/unlock bookkeeping
  needs a Bead-aware source belongs to the lock-screen work.
- **Windows companion has no `OnFailure=`/own fallback.** Its default start limit (5 in 10 s) was not
  reached by 8 kills at `RestartSec=2` (S12); if it ever fails permanently `status` shows
  `windows: failed` and a repeated `bead` reports it without repairing. `qml` or restarting the unit does.
- QML mode itself now waits for the QML provider after the fix to the probe (daemon mode only); it
  registered well inside the 20 s limit in every run.

## Temporary duplicates (expiry)

Nothing new that must outlive the validation. The container scripts under `scripts/systemd-validation/`
are a test harness: delete them 30 days after Bead is the trusted daily shell or earlier if a CI job
replaces them. The B0029 items (windows-only mode, `rollback` alias) keep their expiries.

## Evidence and reproduction

Final run `71/71 passed` on a 12-CPU container (systemd 262.1, dbus-broker 37, quickshell 0.3.2, niri 26.04,
labwc 0.20.2, Qt 6.12.0, software rendering). The sampler reported at most one provider process in every
scenario. Nacre unit tests: `tools/tests/test_ui_shell_switch.py` 14 tests; `cargo test -p nacre-helper
shell_supervisor` 2 tests. The Nacre `desktop` CI job was replicated in the CI container (see the
unit report). The container, image and work directories were removed afterwards.
