#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0030 relative memory gate: the main shell with notices (`--notices`) against the same
shell without them (control), alternating arms on one host, CPU renderer, one private
nested niri output. Phases per arm: idle, one popup shown, popup closed. Each arm warms a
phase, then keeps five 1 s samples of the shell's smaps_rollup (Pss, Pss_Anon, Pss_File,
Pss_Shmem, SwapPss; DRM is not measured and stays unknown).

Run under the desktop heavy-job lock with SMOKE_ROOT holding src/, bin/{nacre-helper,
CONTROL,CANDIDATE}, nacre-assets/. Private bus, private HOME/state, never the real
notification name. Usage: b0030-memory.py PAIRS CONTROL_BIN CANDIDATE_BIN"""
import json, os, pathlib, shutil, signal, socket, statistics, subprocess, sys, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
SRC, BIN = ROOT / "src", ROOT / "bin"
RUN = pathlib.Path("/tmp/b0030-mem")
OUT = ROOT / "mem"
KEYS = ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss")
MIB = 1024.0


def meminfo():
    out = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":")
        out[k] = int(v.split()[0])
    return out


def rollup(pid):
    out = {}
    for line in open(f"/proc/{pid}/smaps_rollup"):
        k, _, v = line.partition(":")
        if k in KEYS:
            out[k] = int(v.split()[0]) / MIB
    return out


class Arm:
    def __init__(self, label, binary):
        self.label, self.binary, self.procs = label, binary, []

    def spawn(self, cmd, log, env, **kw):
        f = (self.raw / log).open("w")
        p = subprocess.Popen(cmd, env=env, stdout=f, stderr=subprocess.STDOUT, start_new_session=True, **kw)
        self.procs.append(p)
        return p

    def stop(self):
        for p in reversed(self.procs):
            if p.poll() is None:
                try: os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError: pass
        end = time.monotonic() + 8
        for p in self.procs:
            while p.poll() is None and time.monotonic() < end: time.sleep(.05)
            if p.poll() is None:
                try: os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError: pass
        shutil.rmtree(RUN, ignore_errors=True)

    def wait(self, cond, secs, what):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            v = cond()
            if v: return v
            time.sleep(.05)
        raise RuntimeError("timeout: " + what)

    def run(self, tag):
        mem = meminfo()
        swap_used = (mem["SwapTotal"] - mem["SwapFree"]) / MIB
        assert swap_used < 64, f"swap used {swap_used:.0f} MiB before the arm"
        assert mem["MemAvailable"] / MIB / MIB > swap_used / MIB + 8, "MemAvailable below swap used + 8 GiB"
        self.raw = OUT / tag
        shutil.rmtree(self.raw, ignore_errors=True); self.raw.mkdir(parents=True)
        (self.raw / "free-before.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)
        shutil.rmtree(RUN, ignore_errors=True)
        for d in ("r", "pr", "state", "config", "cache", "data", "home", "labwc"):
            (RUN / d).mkdir(parents=True, mode=0o700)
        os.chmod(RUN, 0o700)
        try:
            return self.measure()
        finally:
            self.stop()
            (self.raw / "free-after.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)

    def measure(self):
        base = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "DBUS_SESSION_BUS_ADDRESS")}
        base.update(XDG_CONFIG_HOME=str(RUN/"config"), XDG_CACHE_HOME=str(RUN/"cache"), XDG_STATE_HOME=str(RUN/"state"),
                    XDG_DATA_HOME=str(RUN/"data"), HOME=str(RUN/"home"), PIPEWIRE_REMOTE="bead-private-unavailable")
        bus = f"unix:path={RUN}/bus"
        self.spawn(["dbus-daemon", "--session", f"--address={bus}", "--nofork", "--nopidfile"], "dbus.log", base)
        self.wait(lambda: (RUN/"bus").exists(), 5, "private bus")
        env = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN/"r"))
        penv = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN/"pr"), WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman")
        (RUN/"labwc/autostart").write_text("")
        self.spawn(["labwc", "--config-dir", str(RUN/"labwc")], "labwc.log", penv)
        psock = self.wait(lambda: next((p for p in (RUN/"pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")
        env["WAYLAND_DISPLAY"] = str(psock)
        self.spawn(["niri", "--config", str(SRC/"spikes/niri.kdl")], "niri.log", env)
        nsock = self.wait(lambda: next((p for p in (RUN/"r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri")
        env["WAYLAND_DISPLAY"] = str(nsock)
        env["NIRI_SOCKET"] = str(self.wait(lambda: next(iter((RUN/"r").glob("niri.*.sock")), None), 10, "niri ipc"))
        sock = str(RUN/"d.sock")
        denv = dict(env, NACRE_DAEMON_SOCKET=sock, NACRE_HELPER_TOOLS=str(ROOT/"nacre-assets/nacre/shell-tools"), RUST_LOG="warn")
        self.spawn([str(BIN/"nacre-helper"), "daemon", "--notifications=daemon"], "daemon.log", denv)
        self.wait(lambda: os.path.exists(sock), 15, "daemon socket")

        def owner():
            return subprocess.run(["busctl", "--user", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.freedesktop.Notifications"],
                                  env=env, capture_output=True, text=True).stdout.strip()
        self.wait(lambda: owner() == "b true", 15, "private notification name")
        cmd = [str(BIN/self.binary), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu", "--daemon-socket", sock,
               "--ui-provider", "--command-socket", str(RUN/"cmd"), "--namespace", "bead-b0030-mem", "--palette", str(SRC/"shell/palette.json")]
        if self.label == "candidate" or os.environ.get("NOTICES_BOTH") == "1":
            cmd.append("--notices")
        shell = self.spawn(cmd, "shell.log", env, cwd=str(SRC))
        self.wait(lambda: "UI_SESSION_CREATED" in (self.raw/"shell.log").read_text(errors="replace"), 30, "bar session")
        pid = shell.pid

        fast = os.environ.get("B0030_FAST") == "1"  # crash hunting only: no measurement value

        def phase(name, warm):
            time.sleep(1 if fast else warm)
            samples = []
            for _ in range(1 if fast else 5):
                samples.append(rollup(pid)); time.sleep(1)
            return {k: statistics.median(s[k] for s in samples) for k in KEYS} | {"min_pss": min(s["Pss"] for s in samples), "max_pss": max(s["Pss"] for s in samples)}

        result = {"label": self.label, "binary": self.binary, "pid": pid}
        result["idle"] = phase("idle", 10)
        out = subprocess.run(["busctl", "--user", "call", "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "Notify",
                              "susssasa{sv}i", "Smoke", "0", "dialog-information", "Memory", "one popup", "2", "default", "Open", "1", "urgency", "y", "1", "60000"],
                             env=env, capture_output=True, text=True)
        assert out.returncode == 0, out.stderr
        result["popup"] = phase("popup", 5)
        s = socket.socket(socket.AF_UNIX); s.connect(sock)
        f = s.makefile("rwb", buffering=0)
        f.write((json.dumps({"t": "hello", "proto": 1, "min": 1, "client": "b0030-mem"}) + "\n").encode()); f.readline()
        f.write((json.dumps({"t": "call", "id": 1, "m": "notifications.clear", "p": {"scope": "popups"}, "timeout": 3000}) + "\n").encode())
        time.sleep(1.0); s.close()
        result["closed"] = phase("closed", 10)
        (self.raw / "result.json").write_text(json.dumps(result, indent=1))
        return result


def main():
    pairs, control, candidate = int(sys.argv[1]), sys.argv[2], sys.argv[3]
    if os.environ.get("B0030_FAST") == "1":
        OUT.mkdir(exist_ok=True)
        for i in range(pairs):
            Arm("candidate", candidate).run(f"fast{i}")
            print(f"FAST {i} ok", flush=True)
        return
    OUT.mkdir(exist_ok=True)
    runs = {"control": [], "candidate": []}
    for i in range(pairs):
        order = [("control", control), ("candidate", candidate)] if i % 2 == 0 else [("candidate", candidate), ("control", control)]
        for label, binary in order:
            print(f"ARM pair={i} {label}", flush=True)
            runs[label].append(Arm(label, binary).run(f"pair{i}-{label}"))
            time.sleep(3)
    report = {"pairs": pairs, "renderer": "cpu (tiny-skia)", "control": control, "candidate": candidate, "arms": runs}
    for phase in ("idle", "popup", "closed"):
        for key in ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss"):
            c = statistics.median(r[phase][key] for r in runs["control"])
            n = statistics.median(r[phase][key] for r in runs["candidate"])
            report.setdefault("delta", {}).setdefault(phase, {})[key] = {"control": round(c, 3), "candidate": round(n, 3), "delta": round(n - c, 3)}
    (OUT / "report.json").write_text(json.dumps(report, indent=1))
    for phase, keys in report["delta"].items():
        print(phase, {k: v["delta"] for k, v in keys.items()}, "control_pss", keys["Pss"]["control"], "candidate_pss", keys["Pss"]["candidate"])


if __name__ == "__main__":
    main()
