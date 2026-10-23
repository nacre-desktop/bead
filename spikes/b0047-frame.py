#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0047 desktop frame: private nested proof (headless labwc > niri > shell), CPU renderer, no daemon.

  b0047-frame.py smoke  CAND_BIN POINTER_BIN [CTL_BIN]   reserved edges, lips, hover and click
  b0047-frame.py memory PAIRS CTL_BIN CAND_BIN           relative memory gate, alternating arms

SMOKE_ROOT holds the binaries plus cand-src/shell and ctl-src/shell (each shell root is the
shell's working directory). Private HOME, runtime dirs and buses only; never the live session.
Memory: candidate - control per phase (idle, closed), the frame being on in both arms is the
control's own (the control build has no frame lips or reserved edges)."""
import json, os, pathlib, shutil, signal, statistics, subprocess, sys, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
RUN = pathlib.Path("/tmp/b0047-frame")
OUT = ROOT / "out"
KEYS = ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss")
MIB = 1024.0
NS = "bead-b0047"


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
            o[k] = int(v.split()[0]) / MIB
    return o


class Arm:
    def __init__(s, label, binary, src):
        s.label, s.binary, s.src, s.procs = label, binary, src, []

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
            time.sleep(.05)
        raise RuntimeError("timeout: " + what)

    def start(s, tag):
        mem = meminfo()
        swap = (mem["SwapTotal"] - mem["SwapFree"]) / MIB
        assert swap < 64, f"swap used {swap:.0f} MiB"
        assert mem["MemAvailable"] / MIB / MIB > swap / MIB + 8
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
        s.parent_socket = s.wait(lambda: next((p for p in (RUN / "pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")
        s.env["WAYLAND_DISPLAY"] = str(s.parent_socket)
        (RUN / "niri.kdl").write_text('input { keyboard { xkb { layout "us"; }; }; }\nprefer-no-csd\nanimations { off; }\n'
                                      'hotkey-overlay { skip-at-startup; }\n')
        s.spawn(["niri", "--config", str(RUN / "niri.kdl")], "niri.log", s.env)
        nsock = s.wait(lambda: next((p for p in (RUN / "r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri")
        s.env["WAYLAND_DISPLAY"] = str(nsock)
        s.env["NIRI_SOCKET"] = str(s.wait(lambda: next(iter((RUN / "r").glob("niri.*.sock")), None), 10, "niri ipc"))
        s.cmdsock = str(RUN / "cmd")
        cmd = [str(ROOT / s.binary), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu",
               "--command-socket", s.cmdsock, "--namespace", NS, "--palette", str(s.src / "shell/palette.json")]
        s.shell = s.spawn(cmd, "shell.log", s.env, cwd=str(s.src))
        s.wait(lambda: "UI_SESSION_CREATED" in (s.raw / "shell.log").read_text(errors="replace"), 30, "bar session")

    def niri(s, *a):
        return subprocess.run(["niri", "msg", "--json", *a], env=s.env, capture_output=True, text=True).stdout

    def finish(s):
        s.stop()
        (s.raw / "free-after.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)

    def memory(s, tag):
        s.start(tag)
        try:
            def phase(warm):
                time.sleep(warm)
                smp = []
                for _ in range(5):
                    smp.append(rollup(s.shell.pid))
                    time.sleep(1)
                return {k: statistics.median(x[k] for x in smp) for k in KEYS} | {"min_pss": min(x["Pss"] for x in smp), "max_pss": max(x["Pss"] for x in smp)}
            r = {"label": s.label, "binary": s.binary, "pid": s.shell.pid, "layers": s.layers()}
            r["idle"] = phase(10)
            # Open and close a panel through the command socket, then settle.
            subprocess.run([str(ROOT / s.binary), "ipc", "--socket", s.cmdsock, "drawers", "toggle", "sound"], env=s.env,
                           capture_output=True, text=True, cwd=str(s.src))
            time.sleep(2)
            subprocess.run([str(ROOT / s.binary), "ipc", "--socket", s.cmdsock, "drawers", "toggle", "sound"], env=s.env,
                           capture_output=True, text=True, cwd=str(s.src))
            r["closed"] = phase(12)
            (s.raw / "result.json").write_text(json.dumps(r, indent=1))
            return r
        finally:
            s.finish()

    def layers(s):
        try:
            raw = json.loads(s.niri("layers") or "[]")
        except json.JSONDecodeError:
            return []
        return [{"ns": l.get("namespace"), "layer": l.get("layer"), "kb": l.get("keyboard_interactivity")} for l in raw]

    def window_size(s):
        """Open a terminal and report the tile it was given: the work area after reserved zones."""
        s.spawn(["alacritty", "--title", "b0047-probe"], "alacritty.log", s.env)
        s.wait(lambda: "b0047-probe" in s.niri("windows"), 15, "probe window")
        time.sleep(1)
        w = [w for w in json.loads(s.niri("windows")) if w.get("title") == "b0047-probe"][0]
        return {"window_size": w["layout"]["window_size"], "tile_pos": w["layout"]["tile_pos_in_workspace_view"]}


class Pointer:
    def __init__(s, arm, binary):
        env = dict(arm.env, BEAD_PRIVATE_INPUT_PROOF="1")
        s.proc = subprocess.Popen([binary], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1,
                                  start_new_session=True)
        assert "POINTER_READY" in s.proc.stdout.readline()

    def send(s, line):
        s.proc.stdin.write(line + "\n")
        s.proc.stdin.flush()
        s.proc.stdout.readline()

    def stop(s):
        try:
            s.send("stop")
        except Exception:
            pass
        if s.proc.poll() is None:
            os.killpg(s.proc.pid, signal.SIGTERM)


def smoke(cand, pointer_bin, ctl):
    OUT.mkdir(exist_ok=True)
    report = {"renderer": "cpu (tiny-skia), no daemon"}
    if ctl:
        a = Arm("control", ctl, ROOT / "ctl-src")
        a.start("smoke-control")
        try:
            report["control_window"] = a.window_size()
            report["control_layers"] = a.layers()
        finally:
            a.finish()
    arm = Arm("candidate", cand, ROOT / "cand-src")
    arm.start("smoke-candidate")
    pointer = None
    try:
        out = json.loads(arm.niri("outputs"))
        mode = next(iter(out.values())) if isinstance(out, dict) else out[0]
        cur = mode["modes"][mode["current_mode"]]
        W, H = cur["width"], cur["height"]
        report["output"] = [W, H]
        report["layers"] = arm.layers()
        report["window"] = arm.window_size()
        pointer = Pointer(arm, str(ROOT / pointer_bin))

        def move(x, y):
            pointer.send(f"move {x} {y} {W} {H}")

        def bar_layers():
            return [l for l in arm.layers() if l["ns"] == NS]

        def screenshot(name):
            subprocess.run(["grim", str(arm.raw / name)], env=arm.env, capture_output=True, timeout=10)

        steps = {}
        base = len(bar_layers())
        move(W // 2, H // 2)
        time.sleep(.5)
        screenshot("idle.png")
        # A quick pass over the right lip opens nothing.
        move(W - 4, H // 2)
        time.sleep(.12)
        move(W // 2, H // 2)
        time.sleep(.8)
        steps["quick_pass_opens_nothing"] = len(bar_layers()) == base
        # Resting on the lip opens the panel after the dwell, and the cue fills first.
        move(W - 4, H // 2)
        time.sleep(.2)
        screenshot("dwell.png")
        time.sleep(.9)
        steps["hover_opens_after_dwell"] = len(bar_layers()) > base
        screenshot("open.png")
        # Leaving both the lip and the panel closes a hover-opened panel.
        move(W // 2, H - 40)
        time.sleep(1.2)
        steps["leaving_closes_hover_panel"] = len(bar_layers()) == base
        # A click opens at once; a second click closes.
        move(W - 4, H // 2 + 20)
        pointer.send("click")
        time.sleep(.8)
        steps["click_opens"] = len(bar_layers()) > base
        pointer.send("click")
        time.sleep(1.0)
        steps["second_click_closes"] = len(bar_layers()) == base
        # The top lip lives in the bar: resting on its empty middle opens the dashboard.
        move(W // 2, 20)
        time.sleep(1.2)
        steps["top_lip_hover_opens"] = len(bar_layers()) > base
        report["steps"] = steps
        report["log_errors"] = [l for l in (arm.raw / "shell.log").read_text(errors="replace").splitlines()
                                if "EDGE_" in l or "ERROR" in l][:20]
        (arm.raw / "result.json").write_text(json.dumps(report, indent=1))
        print(json.dumps(report, indent=1))
    finally:
        if pointer:
            pointer.stop()
        arm.finish()


def memory(pairs, ctl, cand):
    OUT.mkdir(exist_ok=True)
    runs = {"control": [], "candidate": []}
    for i in range(pairs):
        order = [("control", ctl), ("candidate", cand)] if i % 2 == 0 else [("candidate", cand), ("control", ctl)]
        for label, b in order:
            print(f"ARM pair={i} {label}", flush=True)
            runs[label].append(Arm(label, b, ROOT / ("ctl-src" if label == "control" else "cand-src")).memory(f"pair{i}-{label}"))
            time.sleep(3)
    rep = {"pairs": pairs, "renderer": "cpu (tiny-skia), no daemon, nested niri on headless labwc", "arms": runs, "delta": {}}
    for ph in ("idle", "closed"):
        for k in KEYS:
            c = statistics.median(r[ph][k] for r in runs["control"])
            n = statistics.median(r[ph][k] for r in runs["candidate"])
            rep["delta"].setdefault(ph, {})[k] = {"control": round(c, 3), "candidate": round(n, 3), "delta": round(n - c, 3)}
    (OUT / "report.json").write_text(json.dumps(rep, indent=1))
    for ph, ks in rep["delta"].items():
        print(ph, {k: v["delta"] for k, v in ks.items()}, "ctl", ks["Pss"]["control"], "cand", ks["Pss"]["candidate"])


if __name__ == "__main__":
    if sys.argv[1] == "smoke":
        smoke(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
    elif sys.argv[1] == "memory":
        memory(int(sys.argv[2]), sys.argv[3], sys.argv[4])
    else:
        raise SystemExit(__doc__)
