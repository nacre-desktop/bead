#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private rehearsal of `nacre ui-shell bead|qml` (B0029): real processes, modelled user manager.

Runs entirely inside a private HOME, runtime dir, session bus and nested compositor chain
(headless labwc -> niri). Real: the packaged bead-shell, quickshell with Nacre's QML, nacre-helper's
daemon, the real ui_shell_switch.py/releases.py. Modelled: the systemd user manager (a small
supervisor behind a `systemctl` shim: Restart=, start limits, OnFailure=, PartOf=, Wants=,
ConditionPathExists=, daemon-reload caching). systemd itself is not exercised.
Never touches the live session: refuses the real runtime dir and the real bus.
"""

import argparse
import json
import os
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

SHELL, WINDOWS, FALLBACK = (
    "nacre-shell.service",
    "nacre-qml-windows.service",
    "nacre-ui-shell-fallback.service",
)


class Manager:
    """Supervises unit processes it started; every signal goes to a pid group it created."""

    def __init__(self, cfg, env, sock):
        self.dir, self.env, self.lock = cfg / "systemd/user", env, threading.RLock()
        self.cfg, self.proc, self.starts, self.nrestarts = {}, {}, {}, {}
        self.stopping, self.failed, self.log = set(), set(), []
        self.reload()
        self.server = socket.socket(socket.AF_UNIX)
        self.server.bind(str(sock))
        self.server.listen(8)
        for target in (self.serve, self.monitor):
            threading.Thread(target=target, daemon=True).start()

    # -- unit files -----------------------------------------------------------
    def parse(self, name):
        fragment = self.dir / name
        paths = ([fragment] if fragment.exists() else []) + sorted((self.dir / (name + ".d")).glob("*.conf"))
        unit = {"paths": [str(p) for p in paths], "loaded": bool(paths), "Environment": [], "Wants": [], "OnFailure": []}
        for path in paths:
            for raw in path.read_text().splitlines():
                key, eq, value = raw.partition("=")
                if not eq or raw.startswith("#"):
                    continue
                if key == "ExecStart":
                    unit["ExecStart"] = shlex.split(value) if value else None
                elif key in {"Environment", "Wants", "OnFailure"}:
                    unit[key] += shlex.split(value)
                else:
                    unit[key] = value
        return unit

    def reload(self):
        with self.lock:
            self.cfg = {n: self.parse(n) for n in (SHELL, WINDOWS, FALLBACK)}

    # -- processes ------------------------------------------------------------
    def start(self, name, restart=False):
        with self.lock:
            unit = self.cfg[name]
            condition = unit.get("ConditionPathExists")
            if not unit.get("ExecStart") or (condition and not Path(condition).exists()):
                self.log.append(f"skip {name}")
                return
            now = time.monotonic()
            window = float(unit.get("StartLimitIntervalSec", 10))
            history = [t for t in self.starts.get(name, []) if now - t < window] + [now]
            self.starts[name] = history
            if len(history) > int(unit.get("StartLimitBurst", 5)):
                self.fail(name)
                return
            env = dict(self.env)
            env.update(item.split("=", 1) for item in unit["Environment"])
            self.failed.discard(name)
            self.stopping.discard(name)
            self.proc[name] = subprocess.Popen(
                unit["ExecStart"], env=env, start_new_session=True,
                stdout=open(self.env["REHEARSAL_LOGS"] + f"/{name}.log", "ab"), stderr=subprocess.STDOUT,
            )
            if restart:
                self.nrestarts[name] = self.nrestarts.get(name, 0) + 1
            self.log.append(f"start {name} pid={self.proc[name].pid}")

    def stop(self, name):
        with self.lock:
            process = self.proc.pop(name, None)
            self.stopping.add(name)
        if process and process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)  # pid group this manager created
            try:
                process.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        self.log.append(f"stop {name}")

    def fail(self, name):
        self.failed.add(name)
        self.log.append(f"failed {name}")
        for target in self.cfg[name]["OnFailure"]:
            threading.Thread(target=self.run_fallback, args=(target,), daemon=True).start()

    def run_fallback(self, name):
        argv = self.cfg[name].get("ExecStart")
        self.log.append(f"on-failure {name}")
        if argv:
            subprocess.run(argv, env=self.env, stdout=open(self.env["REHEARSAL_LOGS"] + f"/{name}.log", "ab"), stderr=subprocess.STDOUT)

    def monitor(self):
        while True:
            time.sleep(0.2)
            with self.lock:
                for name, process in list(self.proc.items()):
                    code = process.poll()
                    if code is None or name in self.stopping:
                        continue
                    del self.proc[name]
                    self.log.append(f"exit {name} code={code}")
                    if code != 0 and self.cfg[name].get("Restart") == "on-failure":
                        threading.Timer(float(self.cfg[name].get("RestartSec", 1)), self.start, (name, True)).start()
                    elif code != 0:
                        self.fail(name)

    # -- systemctl --user -----------------------------------------------------
    def state(self, name):
        process = self.proc.get(name)
        if process and process.poll() is None:
            return "active", "running", process.pid
        return ("failed", "failed", 0) if name in self.failed else ("inactive", "dead", 0)

    def props(self, name):
        unit = self.cfg.get(name)
        if unit is None:  # nacre-daemon units: the daemon is started by the harness itself
            return {"FragmentPath": str(self.dir / name), "LoadState": "loaded", "ActiveState": "active", "SubState": "running",
                    "UnitFileState": "static", "RequiredBy": "nacre-daemon.service" if name.endswith(".socket") else ""}
        active, sub, pid = self.state(name)
        argv = unit.get("ExecStart") or []
        return {
            "FragmentPath": str(self.dir / name) if (self.dir / name).exists() else "",
            "DropInPaths": " ".join(p for p in unit["paths"] if ".d/" in p),
            "LoadState": "loaded" if unit["loaded"] else "not-found", "ActiveState": active, "SubState": sub,
            "MainPID": str(pid), "NRestarts": str(self.nrestarts.get(name, 0)), "UnitFileState": "enabled",
            "Environment": " ".join(unit["Environment"]),
            "ExecStart": f"{{ path={argv[0] if argv else ''} ; argv[]={' '.join(argv)} ; ignore_errors=no ; }}",
        }

    def handle(self, argv):
        action, *args = argv
        if action == "show-environment":
            return "\n".join(f"{k}={v}" for k, v in self.env.items() if k in {"HOME", "XDG_RUNTIME_DIR"})
        if action == "show":
            if args[0].startswith("--"):
                return f"UnitPath={self.dir}"
            names = {n for a in args if a.startswith("--property=") for n in a[11:].split(",")}
            return "\n".join(f"{k}={v}" for k, v in self.props(args[0]).items() if k in names or len(names) >= 12)
        if action == "daemon-reload":
            self.reload()
        elif action == "stop":
            self.stop(args[-1])
        elif action == "reset-failed":
            self.failed.discard(args[-1])
            self.starts[args[-1]] = []
        elif action == "restart" and args[-1] == SHELL:
            for name in (WINDOWS, SHELL):  # PartOf= propagates the stop
                self.stop(name)
            self.start(SHELL)
            for name in self.cfg[SHELL]["Wants"]:
                self.start(name)
        else:
            raise RuntimeError(f"unmodelled systemctl {argv}")
        return ""

    def serve(self):
        while True:
            connection, _ = self.server.accept()
            with connection:
                try:
                    out, rc = self.handle(json.loads(connection.makefile().readline())), 0
                except Exception as error:  # noqa: BLE001 - reported to the caller like systemctl does
                    out, rc = str(error), 1
                try:
                    connection.sendall(json.dumps({"rc": rc, "out": out}).encode())
                except OSError:  # the caller was killed (crash rehearsal)
                    pass


SHIM = """#!/usr/bin/env python3
import json, os, socket, sys
s = socket.socket(socket.AF_UNIX); s.connect(os.environ["REHEARSAL_MGR_SOCK"])
s.sendall((json.dumps(sys.argv[2:]) + "\\n").encode()); s.shutdown(socket.SHUT_WR)
reply = json.loads(s.makefile().read()); print(reply["out"]); sys.exit(reply["rc"])
"""


class Rehearsal:
    def __init__(self, args):
        self.args, self.owned, self.results = args, [], []
        self.work = Path(tempfile.mkdtemp(prefix="b0029-", dir=args.workdir))
        runtime = Path(tempfile.mkdtemp(prefix="b0029r-", dir=f"/run/user/{os.getuid()}"))
        self.rt = runtime
        self.home = self.work / "home"
        self.logs = self.work / "logs"
        for path in (self.home, self.logs, self.work / "bin"):
            path.mkdir(mode=0o700)
        self.cfg = self.home / ".config"
        self.env = {
            "PATH": f"{self.work / 'bin'}:{os.environ['PATH']}", "HOME": str(self.home), "XDG_RUNTIME_DIR": str(runtime),
            "XDG_CONFIG_HOME": str(self.cfg), "XDG_STATE_HOME": str(self.home / ".local/state"),
            "XDG_DATA_HOME": str(self.home / ".local/share"), "XDG_CACHE_HOME": str(self.home / ".cache"),
            "REHEARSAL_LOGS": str(self.logs), "REHEARSAL_MGR_SOCK": str(runtime / "mgr.sock"),
            "NACRE_DAEMON_SOCKET": str(runtime / "nacre/daemon.sock"), "NACRE_COMPOSITOR": "niri",
            "QT_QPA_PLATFORM": "wayland", "PIPEWIRE_REMOTE": "bead-private-unavailable",
            "PULSE_SERVER": "unix:" + str(runtime / "no-pulse"), "BEAD_PRIVATE_SERVICES_TEST": "1",
        }

    def spawn(self, argv, name, **env):
        process = subprocess.Popen(argv, env={**self.env, **env}, start_new_session=True,
                                   stdout=open(self.logs / f"{name}.log", "wb"), stderr=subprocess.STDOUT)
        self.owned.append(process)
        return process

    def wait_for(self, what, test, seconds=30):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            value = test()
            if value:
                return value
            time.sleep(0.25)
        raise SystemExit(f"timed out: {what}")

    def check(self, name, ok, detail=""):
        self.results.append((name, bool(ok), detail))
        print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)

    # -- environment ----------------------------------------------------------
    def compositors(self):
        rt = self.rt
        (rt / "cfg").mkdir()
        (rt / "cfg/autostart").write_text("")
        self.spawn(["labwc", "--config-dir", str(rt / "cfg")], "labwc", WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman")
        self.wait_for("labwc", lambda: (rt / "wayland-0").exists())
        self.spawn(["niri", "--config", str(self.args.bead_src / "spikes/niri.kdl")], "niri", WAYLAND_DISPLAY="wayland-0")
        self.wait_for("niri", lambda: (rt / "wayland-1").exists() and list(rt.glob("niri.*.sock")))
        self.env.update(WAYLAND_DISPLAY="wayland-1", NIRI_SOCKET=str(next(rt.glob("niri.*.sock"))))

    def session_bus(self):
        out = subprocess.run(["dbus-daemon", "--session", "--fork", "--print-address=1", "--print-pid=2"], capture_output=True, text=True, check=True, env=self.env)
        self.env["DBUS_SESSION_BUS_ADDRESS"] = out.stdout.strip()
        self.env["DBUS_SYSTEM_BUS_ADDRESS"] = out.stdout.strip()
        self.dbus_pid = int(out.stderr.strip())  # started by this script

    def install_nacre(self):
        a = self.args
        tools = self.home / ".local/share/nacre/shell/tools"
        tools.mkdir(parents=True)
        shutil.copy2(a.helper, tools / "nacre-helper")
        (self.home / ".local/state/nacre/releases").mkdir(parents=True, mode=0o700)
        lock = self.home / ".local/state/nacre/releases/release.lock"
        lock.write_text("")
        lock.chmod(0o600)
        (self.work / "bin/systemctl").write_text(SHIM)
        (self.work / "bin/systemctl").chmod(0o755)
        units = self.cfg / "systemd/user"
        (units / (SHELL + ".d")).mkdir(parents=True)
        qml = f"quickshell -n -p {a.nacre_src}/nacre/shell/shell.qml"
        qs = shutil.which("quickshell")
        (units / SHELL).write_text(f"[Service]\nExecStart={qs} -n -p {a.nacre_src}/nacre/shell/shell.qml\nRestart=on-failure\nRestartSec=1\n")
        (units / (SHELL + ".d/50-nacre-f4.conf")).write_text("[Service]\nEnvironment=NACRE_DAEMON_CLIENT=1\n")
        for name in ("nacre-daemon.service", "nacre-daemon.socket"):
            (units / name).write_text("[Unit]\nDescription=harness-owned daemon\n")
        self.qml_command = qml

    # -- scenarios ------------------------------------------------------------
    def ui(self, *args, check=True):
        return subprocess.run([sys.executable, str(self.args.nacre_src / "tools/releases.py"), "ui-shell", *args],
                              env=self.env, capture_output=True, text=True, timeout=120)

    def layers(self):
        out = subprocess.run(["niri", "msg", "--json", "layers"], env=self.env, capture_output=True, text=True).stdout
        return sorted({layer["namespace"] for layer in json.loads(out or "[]")})

    def shell_state(self):
        return self.manager.props(SHELL)["ActiveState"], self.manager.props(WINDOWS)["ActiveState"]

    def provider(self):
        reply = subprocess.run([str(self.home / ".local/share/nacre/shell/tools/nacre-helper"), "ctl", "call", "--timeout", "3000", "ui.dispatch",
                                json.dumps({"target": "nacre-ui-shell-probe", "method": "probe", "args": []})],
                               env=self.env, capture_output=True, text=True)
        return "".join((reply.stdout + reply.stderr).split())[:160]

    def registered(self):
        text = self.provider().lower()
        return bool(text) and '"dispatched":false' not in text and '"code":"unavailable"' not in text

    def dropin(self):
        return self.cfg / "systemd/user" / (SHELL + ".d/60-nacre-ui-shell.conf")

    def owner(self):
        out = subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"],
                             env=self.env, capture_output=True, text=True).stdout
        return out.strip().endswith("true")

    def run(self):
        a = self.args
        self.compositors()
        self.session_bus()
        self.install_nacre()
        self.manager = Manager(self.cfg, self.env, Path(self.env["REHEARSAL_MGR_SOCK"]))
        self.spawn([str(a.helper), "daemon"], "daemon")
        self.wait_for("daemon socket", lambda: (self.rt / "nacre/daemon.sock").exists())
        launcher = str(a.package_root / "usr/bin/bead-shell")
        self.manager.start(SHELL)
        self.wait_for("QML shell layers", lambda: self.layers() or None, 60)
        qml_layers = self.layers()
        self.check("baseline: QML shell on the private compositor", qml_layers, ",".join(qml_layers))
        self.check("baseline: QML owns notifications", self.wait_for("qml notifications", self.owner, 30))
        time.sleep(3)
        self.check("baseline: QML registered as UI provider", self.registered(), self.provider())
        status = json.loads(self.ui("status", "--launcher", launcher).stdout)
        self.check("status: ready_for_bead is empty", status["ready_for_bead"] == [] and status["active"] == "qml", json.dumps(status)[:200])

        # 1. stub that passes check but dies at start: automatic fall back to QML
        stub = self.work / "stub/bin/bead-shell"
        stub.parent.mkdir(parents=True)
        stub.write_text("#!/bin/sh\n[ \"$1\" = check ] && exit 0\nexit 3\n")
        stub.chmod(0o755)
        result = self.ui("bead", "--notifications", "none", "--launcher", str(stub))
        self.check("crashing Bead: switch fails and QML is restored", result.returncode != 0 and not self.dropin().exists() and self.shell_state() == ("active", "inactive"), (result.stderr or result.stdout)[-160:])
        self.wait_for("QML layers back", lambda: self.layers() == qml_layers, 40)

        # 2. real Bead
        result = self.ui("bead", "--notifications", "none", "--launcher", launcher)
        self.check("bead: switch completes", result.returncode == 0, (result.stdout + result.stderr)[-200:])
        self.wait_for("bead layers", lambda: self.layers() and self.layers() != qml_layers, 40)
        bead_layers = self.layers()
        self.check("bead: layer surfaces are Bead's, QML bar/panels gone", not set(bead_layers) & set(qml_layers), ",".join(bead_layers))
        self.check("bead: shell and QML windows companion running", self.shell_state() == ("active", "active"), str(self.shell_state()))
        self.check("bead: Bead is the registered UI provider", self.registered(), self.provider())
        self.check("bead: nobody owns notifications (--notifications none)", not self.owner())
        ipc = lambda *x: subprocess.run(["quickshell", "ipc", "-p", f"{a.nacre_src}/nacre/shell/shell.qml", "call", *x], env=self.env, capture_output=True, text=True, timeout=30)
        ipc("settingsApp", "open", "notifications")
        state = ipc("settingsApp", "state").stdout
        self.check("bead: QML Settings opens via the unchanged IPC", '"visible":true' in state.replace(" ", ""), state.strip()[:120])
        self.check("bead: opening Settings did not take the notification bus", not self.owner())
        self.check("bead: status reports bead", json.loads(self.ui("status", "--launcher", launcher).stdout)["active"] == "bead")

        # 3. one-step rollback
        result = self.ui("qml")
        self.check("qml: one-step rollback", result.returncode == 0 and self.shell_state() == ("active", "inactive") and not self.dropin().exists(), (result.stdout + result.stderr)[-160:])
        self.wait_for("QML layers after rollback", lambda: self.layers() == qml_layers, 40)
        self.check("qml: QML surfaces back, Bead's gone", self.layers() == qml_layers, ",".join(self.layers()))

        # 4. Bead dies repeatedly after a good switch: start limit trips OnFailure, real fallback returns to QML
        self.ui("bead", "--notifications", "none", "--launcher", launcher)
        for _ in range(8):
            pid = self.manager.props(SHELL)["MainPID"]
            if pid != "0":
                os.kill(int(pid), signal.SIGKILL)  # a Bead process this rehearsal's manager started
            time.sleep(1.5)
            if not self.dropin().exists():
                break
        self.wait_for("fallback", lambda: not self.dropin().exists(), 90)
        self.wait_for("QML back after fallback", lambda: self.shell_state()[0] == "active" and self.layers() == qml_layers, 60)
        self.check("crash loop: OnFailure fallback restored QML", self.layers() == qml_layers and self.shell_state() == ("active", "inactive"), "; ".join(self.manager.log[-4:]))

        # 5. real crash of the switch process mid-transaction, then recovery
        process = subprocess.Popen([sys.executable, str(a.nacre_src / "tools/releases.py"), "ui-shell", "bead", "--notifications", "none", "--launcher", launcher],
                                   env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        self.wait_for("drop-in written", self.dropin().exists, 30)
        os.killpg(process.pid, signal.SIGKILL)  # the switch process this script started
        process.wait()
        time.sleep(1)
        self.check("killed switch: a shell is still running", self.shell_state()[0] == "active")
        result = self.ui("qml")
        self.wait_for("QML layers after recovery", lambda: self.layers() == qml_layers, 40)
        self.check("killed switch: `qml` converges to QML", result.returncode == 0 and self.shell_state() == ("active", "inactive") and not self.dropin().exists(), (result.stdout + result.stderr)[-160:])
        journal = json.loads(self.ui("status", "--launcher", launcher).stdout)
        self.check("no incomplete transactions remain", journal["incomplete"] == [], json.dumps(journal["incomplete"]))

    def verify_units(self):
        env = dict(self.env, SYSTEMD_UNIT_PATH=f"{self.cfg / 'systemd/user'}:")
        self.ui("bead", "--notifications", "none", "--launcher", str(self.args.package_root / "usr/bin/bead-shell"))
        out = subprocess.run(["systemd-analyze", "--user", "verify", WINDOWS, FALLBACK, SHELL], env=env, capture_output=True, text=True, cwd=self.cfg / "systemd/user")
        self.check("systemd-analyze verify accepts the generated units", out.returncode == 0 and "Unknown" not in out.stderr, (out.stdout + out.stderr).strip()[:300])
        self.ui("qml")

    def cleanup(self):
        for process in reversed(self.owned):  # only processes this script spawned
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
        with self.manager.lock if hasattr(self, "manager") else threading.RLock():
            for process in list(getattr(getattr(self, "manager", None), "proc", {}).values()):
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
        if hasattr(self, "dbus_pid"):
            os.kill(self.dbus_pid, signal.SIGTERM)
        time.sleep(1.5)
        shutil.rmtree(self.rt, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nacre-src", type=Path, required=True, help="Nacre checkout containing this change")
    parser.add_argument("--bead-src", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--package-root", type=Path, required=True, help="extracted bead-shell package")
    parser.add_argument("--helper", type=Path, required=True, help="built nacre-helper")
    parser.add_argument("--workdir", type=Path, required=True)
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args()
    if os.getpriority(os.PRIO_PROCESS, 0) < 10:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
    rehearsal = Rehearsal(args)
    try:
        rehearsal.run()
        rehearsal.verify_units()
    finally:
        rehearsal.cleanup()
    failed = [name for name, ok, _ in rehearsal.results if not ok]
    print(f"{len(rehearsal.results) - len(failed)}/{len(rehearsal.results)} checks passed; logs {rehearsal.logs}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
