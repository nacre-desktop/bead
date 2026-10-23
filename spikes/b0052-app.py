#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0052 on-demand app mode: private nested proof (headless labwc > niri > shell), CPU renderer.

  b0052-app.py smoke  BIN [N]          open/deep-link/close both apps, cold-open times over N runs
  b0052-app.py memory PAIRS BIN        shell PSS after app open/close versus the same binary left idle

SMOKE_ROOT holds the binary and src/shell (the shell's working directory). A tiny fake daemon
(no authority, private socket) answers `ai.chat`/settings topics. Private HOME, runtime dirs and buses
only; never the live session. The fake daemon is a test double, not a Nacre daemon."""
import json, os, pathlib, shutil, signal, socket, statistics, subprocess, sys, threading, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
RUN = pathlib.Path("/tmp/b0052-app")
OUT = ROOT / "out"
KEYS = ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss")
NS = "bead-b0052"
CHAT = {"available": True, "channels": {"": {"state": {
    "defaultProvider": "claude", "inWorkspace": False, "provider": "claude", "account": "Default", "title": "Fix the build",
    "threadId": "t1", "model": "", "features": [], "composerKey": "k", "busy": False, "status": "", "error": ""},
    "messages": {"0000000001": {"id": "m1", "role": "user", "text": "hello"},
                 "0000000002": {"id": "m2", "role": "assistant", "text": "Hi, **ready**."}}}}}
METHODS = ["ai.chat.open", "ai.chat.send", "ai.chat.cancel", "ai.chat.new", "ai.chat.workspace", "ai.chat.provider",
           "ai.chat.resume", "ai.chat.approve", "settings.set"]
TOPICS = {"ai.chat": 1, **{f"settings.store.{s}": 1 for s in ("desktop", "dashboard", "profile", "launcher-layout", "colors")}}


def meminfo():
    o = {}
    for l in open("/proc/meminfo"):
        k, v = l.split(":")
        o[k] = int(v.split()[0])
    return o


def rollup(pid):
    o = {}
    for l in open(f"/proc/{pid}/smaps_rollup"):
        k, _, v = l.partition(":")
        if k in KEYS:
            o[k] = int(v.split()[0]) / 1024.0
    return o


def fake_daemon(path, log):
    if os.path.exists(path):
        os.unlink(path)
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(path)
    os.chmod(path, 0o600)
    listener.listen(8)

    def handle(conn):
        reader = conn.makefile("rb")
        send = lambda v: conn.sendall((json.dumps(v) + "\n").encode())
        hello = json.loads(reader.readline() or "null")
        if not hello or hello.get("t") != "hello":
            return
        with open(log, "a") as f:
            f.write(json.dumps({"hello": hello.get("client")}) + "\n")
        send({"t": "welcome", "proto": 1, "min": 1, "boot": "b0052", "topics": TOPICS,
              "methods": {m: {"v": 1, "class": "desktop", "timeout": 10000, "params": {"type": "object"}, "result": {}} for m in METHODS}})
        for line in reader:
            m = json.loads(line)
            if m.get("t") == "sub":
                for t in m.get("topics", []):
                    if t == "ai.chat":
                        data = CHAT
                    elif t.startswith("settings.store."):
                        data = {"store": t[15:], "rev": 1, "exists": True, "data": {}, "dirty": False, "error": ""}
                    else:
                        continue
                    send({"t": "snap", "topic": t, "seq": 0, "data": data})
            elif m.get("t") == "call":
                send({"t": "ret", "id": m["id"], "ok": {"channel": ""}})

    def serve():
        while True:
            c, _ = listener.accept()
            threading.Thread(target=lambda c=c: (handle(c), c.close()), daemon=True).start()

    threading.Thread(target=serve, daemon=True).start()


class Arm:
    def __init__(s, label, binary):
        s.label, s.binary, s.procs = label, binary, []

    def spawn(s, cmd, log, env, **kw):
        f = (s.raw / log).open("w")
        p = subprocess.Popen(cmd, env=env, stdout=f, stderr=subprocess.STDOUT, start_new_session=True, **kw)
        s.procs.append(p)
        return p

    def stop(s):
        for p in reversed(s.procs):
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        end = time.monotonic() + 8
        for p in s.procs:
            while p.poll() is None and time.monotonic() < end:
                time.sleep(.05)
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        shutil.rmtree(RUN, ignore_errors=True)

    def wait(s, cond, secs, what):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            v = cond()
            if v:
                return v
            time.sleep(.02)
        raise RuntimeError("timeout: " + what)

    def start(s, tag):
        mem = meminfo()
        swap = (mem["SwapTotal"] - mem["SwapFree"]) / 1024.0
        assert swap < 64, f"swap used {swap:.0f} MiB"
        assert mem["MemAvailable"] / 1024.0 > swap + 8 * 1024
        s.raw = OUT / tag
        shutil.rmtree(s.raw, ignore_errors=True)
        s.raw.mkdir(parents=True)
        (s.raw / "free-before.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)
        shutil.rmtree(RUN, ignore_errors=True)
        for d in ("r", "pr", "state", "config", "cache", "data", "home", "labwc"):
            (RUN / d).mkdir(parents=True, mode=0o700)
        os.chmod(RUN, 0o700)
        base = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "DBUS_SESSION_BUS_ADDRESS")}
        base.update(XDG_CONFIG_HOME=str(RUN / "config"), XDG_CACHE_HOME=str(RUN / "cache"), XDG_STATE_HOME=str(RUN / "state"),
                    XDG_DATA_HOME=str(RUN / "data"), HOME=str(RUN / "home"), PIPEWIRE_REMOTE="bead-private-unavailable")
        bus = f"unix:path={RUN}/bus"
        s.spawn(["dbus-daemon", "--session", f"--address={bus}", "--nofork", "--nopidfile"], "dbus.log", base)
        s.wait(lambda: (RUN / "bus").exists(), 5, "bus")
        s.env = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "r"))
        penv = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "pr"), WLR_BACKENDS="headless",
                    WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman")
        (RUN / "labwc/autostart").write_text("")
        s.spawn(["labwc", "--config-dir", str(RUN / "labwc")], "labwc.log", penv)
        s.wait(lambda: next((p for p in (RUN / "pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")
        s.env["WAYLAND_DISPLAY"] = str(next(p for p in (RUN / "pr").glob("wayland-*") if not str(p).endswith(".lock")))
        (RUN / "niri.kdl").write_text('input { keyboard { xkb { layout "us"; }; }; }\nprefer-no-csd\nanimations { off; }\n'
                                      'hotkey-overlay { skip-at-startup; }\n')
        s.spawn(["niri", "--config", str(RUN / "niri.kdl")], "niri.log", s.env)
        nsock = s.wait(lambda: next((p for p in (RUN / "r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri")
        s.env["WAYLAND_DISPLAY"] = str(nsock)
        s.env["NIRI_SOCKET"] = str(s.wait(lambda: next(iter((RUN / "r").glob("niri.*.sock")), None), 10, "niri ipc"))
        s.cmdsock = str(RUN / "cmd")
        s.daemon = str(RUN / "daemon.sock")
        s.daemon_log = RUN / "daemon.log"
        fake_daemon(s.daemon, str(s.daemon_log))
        src = ROOT / "src"
        cmd = [str(ROOT / s.binary), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu", "--command-socket",
               s.cmdsock, "--daemon-socket", s.daemon, "--namespace", NS, "--palette", str(src / "shell/palette.json")]
        s.shell = s.spawn(cmd, "shell.log", s.env, cwd=str(src))
        s.wait(lambda: "UI_SESSION_CREATED" in (s.raw / "shell.log").read_text(errors="replace"), 30, "bar session")

    def niri(s, *a):
        return subprocess.run(["niri", "msg", "--json", *a], env=s.env, capture_output=True, text=True).stdout

    def jlist(s, what):
        try:
            v = json.loads(s.niri(what) or "[]")
        except json.JSONDecodeError:
            return []
        return v if isinstance(v, list) else list(v.values())

    def ipc(s, target, method, *args):
        """One command-socket call (newline-delimited JSON, as `nacre-shell ipc`; the CLI refuses `settings PAGE`)."""
        c = socket.socket(socket.AF_UNIX)
        c.settimeout(5)
        c.connect(s.cmdsock)
        c.sendall((json.dumps({"version": 1, "id": 1, "target": target, "method": method, "args": list(args)}) + "\n").encode())
        reply = json.loads(c.makefile("rb").readline())
        c.close()
        return type("R", (), {"returncode": 0 if "error" not in reply or reply["error"] is None else 1, "stderr": str(reply)})

    def app_pids(s, name):
        out = []
        for d in os.listdir("/proc"):
            if d.isdigit():
                try:
                    argv = open(f"/proc/{d}/cmdline", "rb").read().split(b"\0")
                except OSError:
                    continue
                if b"--app" in argv and name.encode() in argv and str(RUN).encode() in b" ".join(argv):
                    out.append(int(d))
        return out

    def settings_windows(s):
        return [w for w in s.jlist("windows") if w.get("app_id") == "nacre-settings"]

    def chat_layers(s):
        return [l for l in s.jlist("layers") if l.get("namespace") == "nacre-app-chat"]

    def open_ms(s, call, visible):
        t = time.monotonic()
        r = s.ipc(*call)
        assert r.returncode == 0, r.stderr
        s.wait(visible, 10, "app visible")
        return (time.monotonic() - t) * 1000

    def finish(s):
        s.stop()
        (s.raw / "free-after.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)

    def shot(s, name):
        subprocess.run(["grim", str(s.raw / name)], env=s.env, capture_output=True, timeout=10)

    def close_settings(s):
        for w in s.settings_windows():
            s.niri("action", "close-window", "--id", str(w["id"]))  # json flag ignored by actions

    def cycle(s, shots=False):
        """One open/close of each app; returns the cold-open times (ms)."""
        t = {}
        t["settings"] = s.open_ms(["nacre", "settings", "appearance"], lambda: s.settings_windows())
        if shots:
            time.sleep(.8)
            s.shot("settings.png")
        s.close_settings()
        s.wait(lambda: not s.app_pids("settings"), 8, "settings process gone")
        t["chat"] = s.open_ms(["nacre", "left"], lambda: s.chat_layers())
        if shots:
            time.sleep(.8)
            s.shot("chat.png")
        s.ipc("nacre", "left")
        s.wait(lambda: not s.app_pids("chat"), 8, "chat process gone")
        return t


def smoke(binary, runs):
    OUT.mkdir(exist_ok=True)
    arm = Arm("candidate", binary)
    arm.start("smoke")
    rep = {"renderer": "cpu (tiny-skia), fake daemon"}
    try:
        steps, times = {}, {"settings": [], "chat": []}
        steps["no_app_before"] = not arm.app_pids("settings") and not arm.app_pids("chat")
        first = arm.cycle(shots=True)
        for k, v in first.items():
            times[k].append(v)
        # Single instance: a second open deep-links the running window and starts nothing.
        arm.ipc("nacre", "settings", "appearance")
        arm.wait(lambda: arm.settings_windows(), 10, "settings")
        pid = arm.app_pids("settings")
        arm.ipc("nacre", "settings", "notifications")
        time.sleep(.5)
        steps["second_open_same_process"] = len(pid) == 1 and arm.app_pids("settings") == pid and len(arm.settings_windows()) == 1
        arm.shot("settings-deeplink.png")
        steps["settings_client_hello"] = "nacre-app-settings" in arm.daemon_log.read_text()
        arm.close_settings()
        arm.wait(lambda: not arm.app_pids("settings"), 8, "settings gone")
        steps["settings_process_gone_after_close"] = True
        # A crash never affects the shell: kill only the app process we started via the shell.
        arm.ipc("nacre", "left")
        arm.wait(lambda: arm.chat_layers(), 10, "chat")
        pids = arm.app_pids("chat")
        steps["chat_client_hello"] = "nacre-app-chat" in arm.daemon_log.read_text()
        os.kill(pids[0], signal.SIGKILL)
        time.sleep(.5)
        steps["shell_survives_app_crash"] = arm.shell.poll() is None
        steps["bar_layer_still_present"] = any(l.get("namespace") == NS for l in arm.jlist("layers"))
        for _ in range(runs - 1):
            for k, v in arm.cycle().items():
                times[k].append(v)
        steps["chat_process_gone_after_toggle"] = not arm.app_pids("chat")
        rep["cold_open_ms"] = {k: {"n": len(v), "p50": round(statistics.median(v), 1), "max": round(max(v), 1), "all": [round(x) for x in v]}
                               for k, v in times.items()}
        rep["steps"] = steps
        rep["method"] = "ipc client start -> first niri poll that sees the window/layer (poll cost about 5-10 ms included)"
        (arm.raw / "result.json").write_text(json.dumps(rep, indent=1))
        print(json.dumps(rep, indent=1))
    finally:
        arm.finish()


def memory_arm(binary, label, tag, cycles):
    arm = Arm(label, binary)
    arm.start(tag)
    try:
        def phase(warm):
            time.sleep(warm)
            smp = []
            for _ in range(5):
                smp.append(rollup(arm.shell.pid))
                time.sleep(1)
            return {k: statistics.median(x[k] for x in smp) for k in KEYS}
        r = {"label": label, "pid": arm.shell.pid, "idle": phase(10)}
        for _ in range(cycles if label != "control" else 0):
            if label in ("candidate", "settings"):
                arm.open_ms(["nacre", "settings", "appearance"], lambda: arm.settings_windows())
                arm.close_settings()
                arm.wait(lambda: not arm.app_pids("settings"), 8, "settings process gone")
            if label in ("candidate", "chat"):
                arm.open_ms(["nacre", "left"], lambda: arm.chat_layers())
                arm.ipc("nacre", "left")
                arm.wait(lambda: not arm.app_pids("chat"), 8, "chat process gone")
            if label == "extwin":
                # A foreign window the shell merely observes: separates app mode from window tracking.
                p = arm.spawn(["alacritty", "--title", "b0052-ext"], "alacritty.log", arm.env)
                arm.wait(lambda: any(w.get("title") == "b0052-ext" for w in arm.jlist("windows")), 15, "ext window")
                time.sleep(1)
                os.killpg(p.pid, signal.SIGTERM)
                arm.wait(lambda: not any(w.get("title") == "b0052-ext" for w in arm.jlist("windows")), 10, "ext closed")
        r["closed"] = phase(12)
        (arm.raw / "result.json").write_text(json.dumps(r, indent=1))
        return r
    finally:
        arm.finish()


def memory(pairs, binary, labels=("control", "candidate")):
    OUT.mkdir(exist_ok=True)
    runs = {k: [] for k in labels}
    for i in range(pairs):
        for label in (labels if i % 2 == 0 else labels[::-1]):
            print(f"ARM pair={i} {label}", flush=True)
            runs[label].append(memory_arm(binary, label, f"pair{i}-{label}", 3))
            time.sleep(3)
    rep = {"pairs": pairs, "cycles": "3 x (Settings open/close + sidebar open/close)", "renderer": "cpu (tiny-skia), fake daemon, nested niri", "delta": {}}
    for ph in ("idle", "closed"):
        for lab in labels[1:]:
            for k in KEYS:
                c = statistics.median(r[ph][k] for r in runs["control"])
                n = statistics.median(r[ph][k] for r in runs[lab])
                rep["delta"].setdefault(ph + "/" + lab, {})[k] = {"control": round(c, 3), "candidate": round(n, 3), "delta": round(n - c, 3)}
    rep["arms"] = runs
    (OUT / "report.json").write_text(json.dumps(rep, indent=1))
    for ph, ks in rep["delta"].items():
        print(ph, {k: v["delta"] for k, v in ks.items()}, "ctl", ks["Pss"]["control"], "cand", ks["Pss"]["candidate"])


if __name__ == "__main__":
    if sys.argv[1] == "smoke":
        smoke(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 5)
    elif sys.argv[1] == "memory":
        memory(int(sys.argv[2]), sys.argv[3], tuple(sys.argv[4].split(",")) if len(sys.argv) > 4 else ("control", "candidate"))
    else:
        raise SystemExit(__doc__)
