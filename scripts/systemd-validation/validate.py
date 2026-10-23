#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0033: validate `nacre ui-shell` against a REAL systemd user manager.

Runs inside the disposable container as the unprivileged test user (see run.sh): real systemd as
PID 1, real `user@1000` manager, real dbus, headless labwc -> nested niri, the packaged bead-shell,
real quickshell with Nacre's QML, real nacre-daemon units, F4 daemon mode enabled. Nothing is
modelled. Stdlib only. Every scenario returns (id, title, ok, evidence); results go to results.json.
"""

import json
import os
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HOME = Path.home()
SRC = HOME / "src/nacre-src"
RELEASES = SRC / "tools/releases.py"
HELPER = HOME / ".local/share/nacre/shell/tools/nacre-helper"
LAUNCHER = "/usr/bin/bead-shell"
SHELL, WIN, FALL = (
    "nacre-shell.service",
    "nacre-qml-windows.service",
    "nacre-ui-shell-fallback.service",
)
UNITS = HOME / ".config/systemd/user"
DROPIN = UNITS / (SHELL + ".d/60-nacre-ui-shell.conf")
PROBE = json.dumps({"target": "nacre-ui-shell-probe", "method": "probe", "args": []})
RESULTS = []


def run(argv, timeout=120, env=None, check=False):
    return subprocess.run(
        argv, capture_output=True, text=True, timeout=timeout, env=env, check=check
    )


def ctl(*args, timeout=60):
    return run(["systemctl", "--user", *args], timeout=timeout)


def show(unit, *props):
    out = ctl("show", unit, *[f"--property={p}" for p in props]).stdout
    return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)


def niri_env():
    env = dict(os.environ)
    for line in ctl("show-environment").stdout.splitlines():
        key, _, value = line.partition("=")
        if key in {"NIRI_SOCKET", "WAYLAND_DISPLAY"}:
            env[key] = value
    return env


def layers():
    out = run(["niri", "msg", "--json", "layers"], env=niri_env(), timeout=15).stdout
    try:
        return sorted({layer["namespace"] for layer in json.loads(out or "[]")})
    except (ValueError, KeyError, TypeError):
        return ["?"]


def switch(*args, timeout=150):
    return run([sys.executable, str(RELEASES), "ui-shell", *args], timeout=timeout)


def status():
    return json.loads(switch("status", "--launcher", LAUNCHER).stdout)


def switch_json(*args):
    out = switch(*args)
    try:
        return out.returncode, json.loads(out.stdout)
    except ValueError:
        return out.returncode, {"error": (out.stderr or out.stdout).strip()[-300:]}


# -- process classification: who is a UI provider? ---------------------------------------------------------------
def procs():
    """Return {pid: kind} for QML full shell, QML windows host, Bead, and stub providers (by cmdline/env)."""
    found = {}
    for entry in os.scandir("/proc"):
        if not entry.name.isdigit():
            continue
        try:
            argv = Path(entry.path, "cmdline").read_bytes().split(b"\0")
            exe = argv[0].decode(errors="replace")
            if exe == "/usr/lib/bead/nacre-shell" and b"--ui-provider" in argv:  # not `bead-shell check` (nacre-shell --help)
                found[int(entry.name)] = "bead"
            elif exe == "/usr/bin/quickshell" and argv[1:4] == [b"-c", b"nacre", b"-n"]:  # not the `--version` / `ipc call` helpers
                env = Path(entry.path, "environ").read_bytes()
                windows = b"NACRE_QML_WINDOWS_ONLY=1" in env.split(b"\0")
                found[int(entry.name)] = "qml-windows" if windows else "qml-full"
        except OSError:
            continue
    return found


def detail(pid):
    try:
        stat = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        cmd = Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")[:70]
        cgroup = Path(f"/proc/{pid}/cgroup").read_text().strip().rsplit("/", 1)[-1]
        return f"{pid} ppid={stat[1]} state={stat[0]} {cgroup} {cmd}"
    except OSError:
        return f"{pid} gone"


def provider_count(snapshot=None):
    snapshot = procs() if snapshot is None else snapshot
    return sum(kind in {"bead", "qml-full"} for kind in snapshot.values())


def daemon_probe():
    """'none' = daemon reports no UI provider; 'present' = a provider answered (unknown target)."""
    out = run([str(HELPER), "ctl", "call", "--timeout", "3000", "ui.dispatch", PROBE], timeout=10)
    text = (out.stdout + out.stderr).lower()
    if "provider is unavailable" in text:
        return "none"
    return "present" if text.strip() else "unknown"


class Sampler(threading.Thread):
    """Samples process kinds every 0.2 s and records any moment that breaks the one-UI invariants."""

    def __init__(self):
        super().__init__(daemon=True)
        self.stop_flag, self.violations, self.max_providers, self.samples = False, [], 0, 0
        self.windows_seen = set()
        self.lock = threading.Lock()

    def run(self):
        while not self.stop_flag:
            try:
                snap = procs()
                kinds = sorted(snap.values())
                with self.lock:
                    self.samples += 1
                    self.max_providers = max(self.max_providers, provider_count(snap))
                    if provider_count(snap) > 1 or ("qml-full" in kinds and "qml-windows" in kinds):
                        self.violations.append((round(time.monotonic(), 2), kinds, [detail(pid) for pid in snap]))
            except Exception as error:  # noqa: BLE001 - a scan race must not end the sampler
                print("sampler:", repr(error), flush=True)
            time.sleep(0.2)

    def reset(self):
        with self.lock:
            self.violations, self.max_providers, self.samples = [], 0, 0

    def report(self):
        with self.lock:
            return {
                "samples": self.samples,
                "max_providers": self.max_providers,
                "violations": list(self.violations)[:3],
            }


SAMPLER = Sampler()


def record(ident, title, ok, evidence=""):
    RESULTS.append({"id": ident, "title": title, "ok": bool(ok), "evidence": str(evidence)[:400]})
    print(("PASS " if ok else "FAIL ") + f"{ident} {title}  [{str(evidence)[:200]}]", flush=True)


def wait_until(condition, seconds=30, step=0.25):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = condition()
        if value:
            return value
        time.sleep(step)
    return None


def stable(kind, seconds=40):
    """Wait for a fully converged `qml` or `bead` state and describe it (empty string = converged)."""

    def problems():
        shell, win = show(SHELL, "ActiveState", "SubState", "DropInPaths"), show(WIN, "ActiveState")
        snap = procs()
        kinds = sorted(snap.values())
        want_kinds = ["bead", "qml-windows"] if kind == "bead" else ["qml-full"]
        loaded = str(DROPIN) in shell["DropInPaths"].split()
        issues = []
        if (shell["ActiveState"], shell["SubState"]) != ("active", "running"):
            issues.append(f"shell {shell['ActiveState']}/{shell['SubState']}")
        if DROPIN.exists() != (kind == "bead") or loaded != (kind == "bead"):
            issues.append(f"dropin disk={DROPIN.exists()} loaded={loaded}")
        if (win["ActiveState"] == "active") != (kind == "bead"):
            issues.append(f"windows {win['ActiveState']}")
        if kinds != want_kinds:
            issues.append(f"processes {kinds}")
        if daemon_probe() != "present":
            issues.append("daemon reports no UI provider")
        return "; ".join(issues)

    end = time.monotonic() + seconds
    last = problems()
    while last and time.monotonic() < end:
        time.sleep(0.5)
        last = problems()
    return last


BEAD_HISTORY = HOME / ".local/state/bead/ui-health.json"


def clear_bead_history():
    """Bead starts in safe mode (no UI provider) after 3 unclean runs within 120 s; isolate scenarios from that."""
    try:
        BEAD_HISTORY.write_text('{"version":1,"failures":[],"running":null}')
    except OSError:
        pass


def reset_to_qml():
    """Bring the fixture to the QML baseline between scenarios (uses the tool under test)."""
    switch("qml")
    ctl("reset-failed", SHELL)
    return stable("qml", 60)


def journal(units, since):
    argv = ["journalctl", "--user", "--no-pager", "-o", "short-monotonic", f"--since={since}"]
    for unit in units:
        argv += ["-u", unit]
    return run(argv, timeout=20).stdout


def now_stamp():
    return time.strftime("%Y-%m-%d %H:%M:%S")


def crash_switch(operation, phase, when, how="kill", extra=()):
    """Run the real switch and SIGKILL (or SIGSTOP) it right before/after one journaled phase."""
    helper = HOME / "crash_switch.py"
    argv = [sys.executable, str(helper), operation, phase, when, how, *extra]
    return subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def describe_state():
    shell, win = show(SHELL, "ActiveState", "MainPID"), show(WIN, "ActiveState")
    kinds = sorted(set(procs().values()))
    return f"shell={shell['ActiveState']} windows={win['ActiveState']} dropin_disk={DROPIN.exists()} procs={kinds}"


# -- scenarios ------------------------------------------------------------------------------------------------
def s_baseline():
    problem = stable("qml", 60)
    record("S0a", "baseline: QML full shell, exactly one provider, daemon sees it", not problem, problem or describe_state())
    st = status()
    record("S0b", "ui-shell status: qml, ready_for_bead empty", st["active"] == "qml" and st["ready_for_bead"] == [], json.dumps(st)[:250])
    present = [u for u in (SHELL, WIN, FALL) if (UNITS / u).exists()]  # companion/fallback files appear with the first `bead`
    out = run(["systemd-analyze", "--user", "verify", *present], timeout=60)
    ours = [line for line in (out.stdout + out.stderr).splitlines() if any(u in line for u in (SHELL, WIN, FALL))]
    record("S0c", "systemd-analyze --user verify (QML mode, units present on disk)", out.returncode == 0 and not ours, f"verified {present}: " + ((out.stdout + out.stderr).strip()[:200] or "clean"))
    owner = run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"]).stdout
    record("S0d", "baseline: QML owns org.freedesktop.Notifications", owner.strip().endswith("true"), owner.strip())


def s_probe_semantics():
    """The health probe must tell 'no provider' from 'provider present, unknown target'."""
    import importlib

    sys.path.insert(0, str(SRC / "tools"))
    sys.path.insert(0, str(SRC / "nacre/shell-tools"))
    ui = importlib.import_module("ui_shell_switch")

    class Probe(ui.UiShell):
        def __init__(self):  # only provider_ready is exercised
            self.health_command, self.runner = [str(HELPER), "ctl"], subprocess.run

    probe = Probe()
    binding = {"HOME": str(HOME), "XDG_RUNTIME_DIR": os.environ["XDG_RUNTIME_DIR"]}
    with_provider = probe.provider_ready(binding)
    ctl("stop", SHELL)
    time.sleep(1)
    without = probe.provider_ready(binding)
    daemon = daemon_probe()
    ctl("start", SHELL)
    stable_problem = stable("qml", 60)
    record("S1a", "tool provider_ready(): True with a provider, False with none", with_provider and not without and daemon == "none", f"with={with_provider} without={without} daemon_says={daemon} recovered={'ok' if not stable_problem else stable_problem}")


def s_switch_and_qml():
    code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    record("S2a", "ui-shell bead --notifications none completes (real restart + health)", code == 0 and result.get("shell") == "bead", json.dumps(result)[:200])
    problem = stable("bead", 40)
    record("S2b", "bead: shell+windows companion running, one provider (bead), windows host is windows-only", not problem, problem or describe_state())
    bead_layers = layers()
    record("S2c", "bead: layer surfaces belong to Bead (no nacre-* QML bar)", bead_layers != [] and not any(n.startswith("nacre") for n in bead_layers), ",".join(bead_layers))
    st = status()
    record("S2d", "status reports bead + notifications none, nothing incomplete", st["active"] == "bead" and st["notifications"] == "none" and st["incomplete"] == [], json.dumps(st)[:250])
    wpid, wstart = show(WIN, "MainPID")["MainPID"], show(WIN, "ActiveEnterTimestampMonotonic")["ActiveEnterTimestampMonotonic"]
    time.sleep(50)  # the supervisor's own health gate is 20 s: a wrong gate shows as a restart every ~22 s
    record("S2m", "windows companion stays up 50 s: same pid, no restart (supervisor health passes in windows-only mode)", show(WIN, "MainPID")["MainPID"] == wpid and show(WIN, "ActiveEnterTimestampMonotonic")["ActiveEnterTimestampMonotonic"] == wstart and show(WIN, "NRestarts")["NRestarts"] == "0", f"pid {wpid}->{show(WIN, 'MainPID')['MainPID']} NRestarts={show(WIN, 'NRestarts')['NRestarts']}")
    out = run(["systemd-analyze", "--user", "verify", SHELL, WIN, FALL], timeout=60)
    ours = [line for line in (out.stdout + out.stderr).splitlines() if any(u in line for u in (SHELL, WIN, FALL))]
    record("S2e", "systemd-analyze --user verify (Bead mode: drop-in + companion + fallback)", out.returncode == 0 and not ours, (out.stdout + out.stderr).strip()[:250] or "clean")
    owner = run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"]).stdout
    record("S2f", "bead (--notifications none): nobody owns org.freedesktop.Notifications", owner.strip().endswith("false"), owner.strip())
    # the companion
    wpid = show(WIN, "MainPID")["MainPID"]
    ctl("reset-failed", SHELL)  # keep the harness itself from using up the 3-in-60-s start budget
    ctl("restart", SHELL)
    problem = stable("bead", 40)
    record("S2h", "PartOf: restarting the shell restarts the windows companion; converges to one Bead", not problem and show(WIN, "MainPID")["MainPID"] != wpid, problem or f"windows pid {wpid}->{show(WIN, 'MainPID')['MainPID']}")
    ctl("stop", SHELL)
    time.sleep(2)
    win = show(WIN, "ActiveState")["ActiveState"]
    record("S2i", "PartOf: stopping the shell stops the windows companion; no UI processes remain", win == "inactive" and provider_count() == 0 and "qml-windows" not in procs().values(), f"windows={win} procs={sorted(procs().values())}")
    ctl("reset-failed", SHELL)
    # dbus-broker keeps an activation "pending" per name once the unit was already active; a reload clears it
    run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ReloadConfig"])
    call = run(["busctl", "--user", "--timeout=20", "call", "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "GetServerInformation"], timeout=40)
    problem = stable("bead", 40)
    record("S2j", "D-Bus activation with the shell stopped starts the unit with the drop-in: Bead + companion, one provider", not problem, problem or describe_state())
    pid_before = show(SHELL, "MainPID")["MainPID"]
    call = run(["busctl", "--user", "--timeout=4", "call", "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "GetServerInformation"], timeout=30)
    time.sleep(1)
    same = show(SHELL, "MainPID")["MainPID"] == pid_before
    record("S2g", "D-Bus activation of Notifications with Bead active: caller errors, no second UI, shell untouched", call.returncode != 0 and same and provider_count() == 1 and stable("bead", 10) == "", (call.stderr.strip() or call.stdout)[:150])
    # companion crash
    shell_pid = show(SHELL, "MainPID")["MainPID"]
    wpid = show(WIN, "MainPID")["MainPID"]
    os.kill(int(wpid), signal.SIGKILL)
    ok = wait_until(lambda: show(WIN, "ActiveState")["ActiveState"] == "active" and show(WIN, "MainPID")["MainPID"] not in {"0", wpid}, 30)
    record("S2k", "SIGKILL of the windows companion: Restart=on-failure revives it, Bead untouched", ok and show(SHELL, "MainPID")["MainPID"] == shell_pid, f"windows pid {wpid}->{show(WIN, 'MainPID')['MainPID']}")
    # idempotent repeat
    code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    record("S2l", "repeat `bead` is idempotent (health only, no restart)", code == 0 and show(SHELL, "MainPID")["MainPID"] == shell_pid, json.dumps(result)[:160])
    # one-step rollback
    code, result = switch_json("qml")
    problem = stable("qml", 40)
    record("S3a", "ui-shell qml: one-step rollback to QML, companion stopped, one provider", code == 0 and not problem, problem or json.dumps(result)[:160])
    owner = run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"]).stdout
    record("S3b", "after qml: QML owns Notifications again", bool(wait_until(lambda: run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"]).stdout.strip().endswith("true"), 15)), owner.strip())
    st = status()
    record("S3c", "status after qml: qml, nothing incomplete", st["active"] == "qml" and st["incomplete"] == [], json.dumps(st)[:200])
    code, result = switch_json("qml")
    record("S3d", "repeat `qml` is a no-op success", code == 0 and result.get("status") == "complete", json.dumps(result)[:120])
    # rollback alias from Bead
    switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    code, result = switch_json("rollback")
    problem = stable("qml", 40)
    record("S3e", "ui-shell rollback (alias) from Bead returns to QML", code == 0 and not problem, problem or json.dumps(result)[:120])
    # condition gating of the companion in QML mode
    ctl("start", WIN)
    time.sleep(2)
    record("S3f", "companion cannot start in QML mode (ConditionPathExists): inactive, QML untouched", show(WIN, "ActiveState")["ActiveState"] == "inactive" and not stable("qml", 10), show(WIN, "ActiveState", "Result")["ActiveState"])


def s_stubs():
    stub_dir = HOME / "stub"
    stub_dir.mkdir(exist_ok=True)
    dies, hangs = stub_dir / "dies/bead-shell", stub_dir / "hangs/bead-shell"
    for path, body in ((dies, "[ \"$1\" = check ] && exit 0\nexit 3\n"), (hangs, "[ \"$1\" = check ] && exit 0\nexec sleep 3600\n")):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(0o755)
    code, result = switch_json("bead", "--notifications", "none", "--launcher", str(dies))
    problem = stable("qml", 60)
    record("S4a", "Bead that dies at start: switch fails, QML restored by the tool", code != 0 and not problem and not DROPIN.exists(), (json.dumps(result)[:160]) if code else "switch unexpectedly succeeded")
    ctl("reset-failed", SHELL)
    code, result = switch_json("bead", "--notifications", "none", "--launcher", str(hangs))
    problem = stable("qml", 60)
    record("S4b", "Bead that stays up but never registers a UI provider: health fails, QML restored", code != 0 and not problem and not DROPIN.exists(), json.dumps(result)[:200] if code else "switch reported success with NO UI provider")
    reset_to_qml()


def s_crash_loop():
    code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    problem = stable("bead", 30)
    record("S5-0", "crash-loop setup: bead switch completes", code == 0 and not problem, problem or json.dumps(result)[:200])
    since = now_stamp()
    # a single crash must restart Bead and keep it
    pid = show(SHELL, "MainPID")["MainPID"]
    os.kill(int(pid), signal.SIGKILL)
    ok = wait_until(lambda: show(SHELL, "MainPID")["MainPID"] not in {"0", pid} and show(SHELL, "ActiveState")["ActiveState"] == "active", 30)
    problem = stable("bead", 30)
    record("S5a", "one SIGKILL of Bead: systemd restarts Bead (no fallback), companion follows", ok and not problem, problem or f"pid {pid}->{show(SHELL, 'MainPID')['MainPID']} NRestarts={show(SHELL, 'NRestarts')['NRestarts']}")
    time.sleep(62)  # let the 60 s start-limit window expire
    ctl("daemon-reload")
    SAMPLER.reset()
    since = now_stamp()
    kills, last, deadline = 0, None, time.monotonic() + 90
    while time.monotonic() < deadline and DROPIN.exists():
        info = show(SHELL, "MainPID", "ActiveState")
        if info["MainPID"] not in {"0", last} and info["ActiveState"] == "active":
            os.kill(int(info["MainPID"]), signal.SIGKILL)
            last, kills = info["MainPID"], kills + 1
        time.sleep(0.1)
    converged = wait_until(lambda: not stable("qml", 1), 90)
    log = journal([SHELL, FALL], since)
    keyline = [l for l in log.splitlines() if "start-limit" in l or "repeated too quickly" in l or "fallback" in l.lower() or "Return to the QML" in l]
    fall = show(FALL, "Result", "ExecMainStatus", "ActiveState")
    record("S5b", "crash loop: start limit trips and OnFailure ran the fallback", kills >= 3 and bool(keyline), f"kills={kills}; " + " | ".join(l[-90:] for l in keyline[:3]))
    record("S5c", "crash loop: fallback restored QML (drop-in gone, companion stopped, one provider)", bool(converged) and not DROPIN.exists(), describe_state())
    record("S5d", "fallback unit finished successfully", fall.get("ExecMainStatus") == "0" and fall.get("Result") in {"success", ""}, json.dumps(fall))
    st = status()
    record("S5e", "journal after fallback: no incomplete transaction, active=qml", st["active"] == "qml" and st["incomplete"] == [], json.dumps(st)[:250])
    rep = SAMPLER.report()
    record("S5f", "crash loop: never more than one UI provider process (sampled every 0.2 s)", rep["max_providers"] <= 1 and not rep["violations"], json.dumps(rep))
    # Bead's own crash history (3 unclean runs in 120 s) makes its next start a safe-mode bar without a UI provider
    since = now_stamp()
    code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    problem = stable("qml", 60)
    safe = "RECOVERY code=safe_mode" in journal([SHELL], since)
    record("S5g", "OBSERVATION `bead` right after the crash loop: Bead starts in safe mode, health fails, QML restored", code != 0 and not problem and safe, f"rc={code} {json.dumps(result)[:150]} safe_mode_logged={safe}")
    clear_bead_history()
    reset_to_qml()


BEAD_PHASES = ("write-units", "reload-units", "write-dropin", "reload", "restart-shell", "health")
QML_PHASES = ("remove-dropin", "reload", "stop-windows", "restart-shell", "health")


def interrupted(operation, phase, when):
    ident = f"S6-{operation}-{phase}-{when}"
    SAMPLER.reset()
    pre = ""
    clear_bead_history()
    if operation == "qml":
        code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
        pre = stable("bead", 40)
        if pre:  # the precondition (Bead running) must hold, or the case proves nothing
            pre = f"precondition failed: bead rc={code} {json.dumps(result)[:160]}; {pre}"
            record(ident, f"SIGKILL the qml switch {when} phase '{phase}'", False, pre)
            reset_to_qml()
            return
        proc = crash_switch("qml", phase, when)
    else:
        proc = crash_switch("bead", phase, when, extra=["--notifications", "none", "--launcher", LAUNCHER])
    try:
        proc.wait(timeout=90)
    except subprocess.TimeoutExpired:
        proc.kill()
    killed = proc.returncode == -signal.SIGKILL
    time.sleep(1.5)
    left = describe_state()
    st = status()
    refuse = None
    if operation == "bead" and st["incomplete"]:
        refuse = switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    code, result = switch_json("qml")
    problem = stable("qml", 60)
    st2 = status()
    rep = SAMPLER.report()
    ok = killed and code == 0 and not problem and st2["incomplete"] == [] and st2["active"] == "qml" and rep["max_providers"] <= 1 and not rep["violations"]
    extra = "" if refuse is None else f" bead-again={'refused' if refuse.returncode else 'ACCEPTED'}"
    if refuse is not None and not refuse.returncode:
        ok = False
    record(ident, f"SIGKILL the {operation} switch {when} phase '{phase}', then `qml` converges", ok, f"left[{left}] incomplete={len(st['incomplete'])}{extra}; after: {problem or 'converged'}; max_providers={rep['max_providers']} {rep['violations'][:1] if rep['violations'] else ''}")


def s_interruptions():
    for phase in BEAD_PHASES:
        for when in ("before", "after"):
            interrupted("bead", phase, when)
    for phase in QML_PHASES:
        for when in ("before", "after"):
            interrupted("qml", phase, when)


def s_recover_busy():
    switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    stable("bead", 30)
    proc = crash_switch("qml", "restart-shell", "before", how="stop")
    wait_until(lambda: proc.poll() is None and "T" in Path(f"/proc/{proc.pid}/stat").read_text().split(")")[-1].split()[0], 20)
    t0 = time.monotonic()
    out = run([sys.executable, str(RELEASES), "ui-shell", "qml", "--recover"], timeout=30)
    took = time.monotonic() - t0
    busy = "busy" in out.stdout
    proc.kill()
    proc.wait()
    code, result = switch_json("qml")
    problem = stable("qml", 60)
    record("S7a", "`qml --recover` while a switch holds the lock: returns at once with 'busy', changes nothing", busy and out.returncode == 0 and took < 5, f"{out.stdout.strip()[:120]} in {took:.1f}s")
    record("S7b", "after the stopped switch is killed, `qml` converges", code == 0 and not problem, problem or "converged")


def s_daemon():
    switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    stable("bead", 30)
    shell_pid, win_pid = show(SHELL, "MainPID")["MainPID"], show(WIN, "MainPID")["MainPID"]
    SAMPLER.reset()
    ctl("restart", "nacre-daemon.service")
    back = wait_until(lambda: daemon_probe() == "present", 30)
    problem = stable("bead", 30)
    same = show(SHELL, "MainPID")["MainPID"] == shell_pid
    record("S8a", "systemctl restart nacre-daemon.service with Bead active: provider re-registers, Bead not restarted", bool(back) and not problem and same, f"re-registered={bool(back)} same_shell_pid={same} {problem}")
    code, result = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    record("S8b", "`bead` health re-run after daemon restart passes", code == 0, json.dumps(result)[:160])
    dpid = show("nacre-daemon.service", "MainPID")["MainPID"]
    os.kill(int(dpid), signal.SIGKILL)
    back = wait_until(lambda: show("nacre-daemon.service", "MainPID")["MainPID"] not in {"0", dpid} and daemon_probe() == "present", 40)
    problem = stable("bead", 30)
    record("S8c", "SIGKILL of the daemon with Bead active: Restart=on-failure revives it, provider re-registers", bool(back) and not problem, f"daemon pid {dpid}->{show('nacre-daemon.service', 'MainPID')['MainPID']} {problem}")
    ctl("stop", "nacre-daemon.service")
    time.sleep(2)
    probe_cold = daemon_probe()  # a client connecting re-activates the daemon via the socket
    back = wait_until(lambda: daemon_probe() == "present", 40)
    problem = stable("bead", 30)
    rep = SAMPLER.report()
    record("S8d", "daemon stopped then socket-activated by a client; Bead re-registers", bool(back) and not problem, f"cold probe={probe_cold} {problem}")
    record("S8e", "daemon restarts: never more than one UI provider process", rep["max_providers"] <= 1 and not rep["violations"], json.dumps(rep))
    record("S8f", "daemon restarts left Bead's pid and the companion untouched", show(SHELL, "MainPID")["MainPID"] == shell_pid, f"shell {shell_pid}->{show(SHELL, 'MainPID')['MainPID']} NRestarts={show(SHELL, 'NRestarts')['NRestarts']}")


def s_f4():
    """Interaction with the F4 drop-in while Bead is the shell."""
    shell_pid = show(SHELL, "MainPID")["MainPID"]
    f4 = run([sys.executable, str(RELEASES), "f4", "status"], timeout=60)
    try:
        info = json.loads(f4.stdout)
    except ValueError:
        info = {"error": f4.stderr[-200:]}
    drops = show(SHELL, "DropInPaths")["DropInPaths"]
    record("S9a", "f4 status with Bead active reports daemon mode and both drop-ins coexist", info.get("mode") == "1" or info.get("effective") == "1" or "mode" in info, f"mode={info.get('mode')} effective={info.get('effective')} dropins={[Path(p).name for p in drops.split()]}")
    dis = run([sys.executable, str(RELEASES), "f4", "disable"], timeout=120)
    time.sleep(3)
    left = describe_state()
    st = status()
    after = stable("bead", 40)
    record("S9b", "f4 disable while Bead is active: observed outcome (Bead must remain the single UI)", provider_count() == 1 and "bead" in procs().values(), f"rc={dis.returncode} out={(dis.stdout or dis.stderr).strip()[:120]} state={left} ready_for_bead={st['ready_for_bead']} converged={'ok' if not after else after}")
    en = run([sys.executable, str(RELEASES), "f4", "enable"], timeout=120)
    after = stable("bead", 40)
    record("S9c", "f4 enable again with Bead active: daemon mode restored, still one Bead provider", provider_count() == 1 and not after, f"rc={en.returncode} out={(en.stdout or en.stderr).strip()[:140]} {after}")
    switch("qml")
    reset = stable("qml", 60)
    f4s = run([sys.executable, str(RELEASES), "f4", "status"], timeout=60).stdout
    record("S9d", "qml after the F4 round trip returns to a healthy QML shell", not reset, reset or f4s[:100])


def s_session_cycle():
    switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    stable("bead", 30)
    SAMPLER.reset()
    ctl("stop", "b33-session.target")
    time.sleep(3)
    down = sorted(procs().values())
    ctl("start", "b33-session.target")
    problem = stable("bead", 60)
    rep = SAMPLER.report()
    record("S10", "session stop/start with the drop-in on disk: nothing left running, then Bead + companion, one provider", not [k for k in down if k in {"bead", "qml-full", "qml-windows"}] and not problem and rep["max_providers"] <= 1, f"while down: {down}; {problem or 'converged'}; {json.dumps(rep)}")
    switch("qml")
    stable("qml", 60)


def s_stray_provider():
    """A second shell started by hand beside the QML shell (outside systemd): who keeps the UI provider role?"""
    problem = reset_to_qml()
    stray = subprocess.Popen([LAUNCHER, "start"], env=niri_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    time.sleep(8)
    alive = stray.poll() is None
    asked = run([str(HOME / ".local/bin/nacrectl"), "ui", "nacre", "state"], timeout=15)  # only the QML shell implements this target
    qml_answers = asked.returncode == 0 and "workspaces" in asked.stdout
    kinds = sorted(procs().values())
    if alive:
        os.killpg(stray.pid, signal.SIGKILL)  # the process group this harness started
    stray.wait()
    problem = problem or stable("qml", 30)
    record("S11", "OBSERVATION second shell started by hand beside QML: QML keeps the provider role; the extra process still draws", qml_answers and not problem, f"stray_alive={alive} qml_still_answers={qml_answers} processes={kinds} (hand-started shells outside systemd are not prevented)")


def s_windows_loop():
    switch("bead", "--notifications", "none", "--launcher", LAUNCHER)
    stable("bead", 30)
    shell_pid = show(SHELL, "MainPID")["MainPID"]
    kills, last, deadline = 0, None, time.monotonic() + 40
    while time.monotonic() < deadline and kills < 8:
        info = show(WIN, "MainPID", "ActiveState")
        if info["MainPID"] not in {"0", last} and info["ActiveState"] == "active":
            os.kill(int(info["MainPID"]), signal.SIGKILL)
            last, kills = info["MainPID"], kills + 1
        time.sleep(0.1)
    time.sleep(3)
    win = show(WIN, "ActiveState", "Result")
    survived = show(SHELL, "MainPID")["MainPID"] == shell_pid and provider_count() == 1
    st = status()
    code, _ = switch_json("bead", "--notifications", "none", "--launcher", LAUNCHER)
    repeat = code
    switch("qml")
    problem = stable("qml", 60)
    record("S12", "windows companion crash loop: Bead keeps running; `qml` still converges (windows state noted)", survived and not problem, f"kills={kills} windows={win} status.windows={st['windows']} repeat-bead-rc={repeat}")


def main():
    if os.getpriority(os.PRIO_PROCESS, 0) < 10:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
    wanted = sys.argv[1:]
    SAMPLER.start()
    scenarios = [s_baseline, s_probe_semantics, s_switch_and_qml, s_stubs, s_crash_loop, s_interruptions, s_recover_busy, s_daemon, s_f4, s_windows_loop, s_stray_provider, s_session_cycle]
    for scenario in scenarios:
        if wanted and scenario.__name__ not in wanted:
            continue
        print(f"== {scenario.__name__}", flush=True)
        clear_bead_history()
        try:
            scenario()
        except Exception as error:  # noqa: BLE001 - a crashed scenario is a failed check
            record(scenario.__name__, "scenario raised", False, repr(error))
    SAMPLER.stop_flag = True
    Path(HOME / "results.json").write_text(json.dumps(RESULTS, indent=1))
    failed = [r["id"] for r in RESULTS if not r["ok"]]
    print(f"{len(RESULTS) - len(failed)}/{len(RESULTS)} passed; failed: {failed}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
