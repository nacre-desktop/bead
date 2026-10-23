#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0048 pearls: private nested proof (headless labwc > niri > shell), CPU renderer, no daemon.

  b0048-pearls.py smoke  CAND_BIN POINTER_BIN     detach, drag with guides, dock, close, restore
  b0048-pearls.py memory PAIRS CTL_BIN CAND_BIN   relative memory gate, alternating arms
  b0048-pearls.py probe BIN [LABEL]               one arm per phase: PSS, glibc heap, cache budgets, smaps (B0054)
  b0048-pearls.py drag BIN [ROUNDS]               guide/pearl frame timing while dragging (B0054)

B54_WLR_RENDERER=gles2 (and B54_RENDER_DEVICE=/dev/dri/renderD129) makes the outer compositor a GL one, so the
nested niri presents like a real output; the default is the pixman one the first B0048 numbers came from.

SMOKE_ROOT holds the binaries plus src/ (the shell's working directory, with shell/ and spikes/).
Private HOME, runtime dirs and buses only; never the live session. The launcher view runs from
spikes/launcher-fixture.json (no daemon). The pointer actuator holds no keys, so the candidate runs
with BEAD_PRIVATE_INPUT_PROOF=1, which lets a plain press start the pull-out gesture (Super in use)."""
import json, os, pathlib, shutil, signal, statistics, subprocess, sys, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
SRC = ROOT / "src"
RUN = pathlib.Path("/tmp/b0048-pearls")
OUT = ROOT / "out"
KEYS = ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss")
MIB = 1024.0
NS = "bead-b0048"


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
    def __init__(s, label, binary):
        s.label, s.binary, s.procs = label, binary, []

    def spawn(s, cmd, log, env, **kw):
        f = (s.raw / log).open("a")
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
        s.procs = []

    def wait(s, cond, secs, what):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            v = cond()
            if v:
                return v
            time.sleep(.05)
        raise RuntimeError("timeout: " + what)

    def start(s, tag, fresh=True):
        mem = meminfo()
        swap = (mem["SwapTotal"] - mem["SwapFree"]) / MIB
        assert swap < 64, f"swap used {swap:.0f} MiB"
        assert mem["MemAvailable"] / MIB / MIB > swap / MIB + 8
        s.raw = OUT / tag
        if fresh:
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
        if fresh:
            s.spawn(["dbus-daemon", "--session", f"--address={bus}", "--nofork", "--nopidfile"], "dbus.log", base)
            s.wait(lambda: (RUN / "bus").exists(), 5, "bus")
            s.env = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "r"))
            penv = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "pr"), WLR_BACKENDS="headless",
                        WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER=os.environ.get("B54_WLR_RENDERER", "pixman"))
            if os.environ.get("B54_RENDER_DEVICE"):
                penv["WLR_RENDER_DRM_DEVICE"] = os.environ["B54_RENDER_DEVICE"]
            (RUN / "labwc/autostart").write_text("")
            s.spawn(["labwc", "--config-dir", str(RUN / "labwc")], "labwc.log", penv)
            parent = s.wait(lambda: next((p for p in (RUN / "pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")
            s.env["WAYLAND_DISPLAY"] = str(parent)
            (RUN / "niri.kdl").write_text('input { keyboard { xkb { layout "us"; }; }; }\nprefer-no-csd\nanimations { off; }\n'
                                          'hotkey-overlay { skip-at-startup; }\n')
            s.spawn(["niri", "--config", str(RUN / "niri.kdl")], "niri.log", s.env)
            nsock = s.wait(lambda: next((p for p in (RUN / "r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri")
            s.env["WAYLAND_DISPLAY"] = str(nsock)
            s.env["NIRI_SOCKET"] = str(s.wait(lambda: next(iter((RUN / "r").glob("niri.*.sock")), None), 10, "niri ipc"))
        s.cmdsock = str(RUN / "cmd")
        s.start_shell()

    def start_shell(s):
        env = dict(s.env, BEAD_PRIVATE_INPUT_PROOF="1") if s.label == "candidate" else s.env
        cmd = [str(ROOT / s.binary), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu",
               "--trace-state", "--command-socket", s.cmdsock, "--namespace", NS, "--palette", str(SRC / "shell/palette.json"),
               "--menu-fixture", str(s.raw / "menu.json")]
        s.fixture()
        mark = (s.raw / "shell.log").stat().st_size if (s.raw / "shell.log").exists() else 0
        s.shell = s.spawn(cmd, "shell.log", env, cwd=str(SRC))
        s.wait(lambda: "UI_SESSION_CREATED" in (s.raw / "shell.log").read_bytes()[mark:].decode(errors="replace"), 30, "bar session")

    def fixture(s):
        import importlib.util
        spec = importlib.util.spec_from_file_location("menu_fixtures", SRC / "spikes/menu-fixtures.py")
        mf = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mf)
        data = [{"id": f"app-{i:04}", "name": f"Application {i:04}", "kind": "app", "description": "Application fixture",
                 "icon": "fixture"} for i in range(320)]
        f = s.raw / "menu.json"
        f.write_text(json.dumps({"queries": {q: mf.reply(q, data) for q in ["", "a", "ap"]}}))
        f.chmod(0o600)

    def ipc(s, target, method, *args, ok=True):
        r = subprocess.run([str(ROOT / s.binary), "ipc", "--socket", s.cmdsock, "--json", target, method, *map(str, args)],
                           env=s.env, capture_output=True, text=True, cwd=str(SRC), timeout=8)
        if (r.returncode == 0) != ok:
            raise RuntimeError((method, r.stdout, r.stderr))
        return json.loads(r.stdout) if r.stdout.strip() else {}

    def niri(s, *a):
        return subprocess.run(["niri", "msg", "--json", *a], env=s.env, capture_output=True, text=True).stdout

    def layers(s):
        try:
            raw = json.loads(s.niri("layers") or "[]")
        except json.JSONDecodeError:
            return []
        return [l for l in raw if l.get("namespace") == NS]

    def shot(s, name):
        subprocess.run(["grim", str(s.raw / name)], env=s.env, capture_output=True, timeout=10)

    def finish(s):
        s.stop()
        (s.raw / "free-after.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)
        shutil.rmtree(RUN, ignore_errors=True)


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


def pearls(arm):
    return arm.ipc("bead.panel", "pearls")["result"]


def wait_for(arm, cond, secs, what):
    return arm.wait(cond, secs, what)


def presented_intervals(log, since):
    """Compositor presentation intervals (ms) of panel surfaces after byte offset `since`."""
    times = []
    for line in log[since:].decode(errors="replace").splitlines():
        if line.startswith("UI_PRESENTED") and "companion=true" in line:
            for field in line.split():
                if field.startswith("monotonic_ns="):
                    times.append(int(field.split("=")[1]))
    gaps = sorted((b - a) / 1e6 for a, b in zip(times, times[1:]))
    if not gaps:
        return {}
    return {"frames": len(times), "p50": gaps[len(gaps) // 2], "p99": gaps[min(len(gaps) - 1, len(gaps) * 99 // 100)], "max": gaps[-1]}


def smoke(cand, pointer_bin):
    OUT.mkdir(exist_ok=True)
    report = {"renderer": "cpu (tiny-skia), no daemon, launcher from fixture"}
    arm = Arm("candidate", cand)
    arm.start("smoke")
    pointer = None
    steps = {}
    try:
        out = json.loads(arm.niri("outputs"))
        mode = next(iter(out.values())) if isinstance(out, dict) else out[0]
        cur = mode["modes"][mode["current_mode"]]
        W, H = cur["width"], cur["height"]
        report["output"] = [W, H]
        pointer = Pointer(arm, str(ROOT / pointer_bin))
        base = len(arm.layers())
        arm.ipc("bead.panel", "open", "launcher")
        wait_for(arm, lambda: len(arm.layers()) > base, 10, "launcher surface")
        time.sleep(1.2)
        arm.shot("1-docked.png")
        docked = arm.ipc("bead.panel", "state")["result"]
        report["docked_state"] = {k: docked.get(k) for k in ("target_open", "view", "size", "origin", "edge")}
        # Detach through the API.
        pearl_id = arm.ipc("bead.panel", "detach")["result"]
        time.sleep(1.0)
        listing = pearls(arm)
        steps["detach_lists_one_pearl"] = len(listing) == 1 and listing[0]["id"] == pearl_id
        steps["docked_panel_closed"] = not arm.ipc("bead.panel", "state")["result"]["target_open"]
        arm.shot("2-detached.png")
        x, y, w, h = listing[0]["rect"]
        report["pearl_rect"] = [x, y, w, h]
        # Drag it: press on its body, move in small steps (about 125 Hz), release away from every edge.
        log = arm.raw / "shell.log"
        gx, gy = int(x + w / 2), int(y + 40)
        pointer.send(f"move {gx} {gy} {W} {H}")
        time.sleep(.3)
        mark = log.stat().st_size
        pointer.send("press")
        steps_n, dx, dy = 40, -300, 160
        for i in range(1, steps_n + 1):
            pointer.send(f"move {gx + dx * i // steps_n} {gy + dy * i // steps_n} {W} {H}")
            time.sleep(.008)
            if i == steps_n // 2:
                arm.shot("3-dragging.png")
        pointer.send("release")
        time.sleep(.6)
        report["drag_presented_ms"] = presented_intervals(log.read_bytes(), mark)
        moved = pearls(arm)[0]["rect"]
        report["pearl_after_drag"] = moved
        steps["drag_moves_the_pearl"] = abs((moved[0] - x) - dx) <= 24 and abs((moved[1] - y) - dy) <= 24
        steps["guides_released_after_drag"] = len(arm.layers()) == base + 1
        arm.shot("4-moved.png")
        # Persisted.
        store = RUN / "state/bead/pearls.json"
        steps["layout_saved_private"] = store.exists() and (store.stat().st_mode & 0o777) == 0o600
        # Pull it against the bottom edge (the launcher's) and let go: it docks.
        gx, gy = int(moved[0] + moved[2] / 2), int(moved[1] + 40)
        pointer.send(f"move {gx} {gy} {W} {H}")
        time.sleep(.2)
        pointer.send("press")
        goal = H - 10 - int(moved[3]) + 30
        for i in range(1, 31):
            pointer.send(f"move {gx} {gy + (goal + 40 - gy) * i // 30} {W} {H}")
            time.sleep(.008)
        time.sleep(.2)
        arm.shot("5-armed.png")
        pointer.send("release")
        time.sleep(1.0)
        steps["release_near_edge_docks"] = pearls(arm) == [] and arm.ipc("bead.panel", "state")["result"]["target_open"]
        arm.shot("6-redocked.png")
        # Close paths.
        arm.ipc("bead.panel", "detach")
        time.sleep(.6)
        steps["second_detach"] = len(pearls(arm)) == 1
        arm.ipc("bead.panel", "pearl-close")
        time.sleep(.8)
        steps["pearl_close_removes_surface"] = pearls(arm) == [] and len(arm.layers()) == base
        # Restore after a restart: detach, stop the shell, start it again.
        arm.ipc("bead.panel", "open", "launcher")
        time.sleep(.8)
        arm.ipc("bead.panel", "detach")
        time.sleep(.8)
        before = pearls(arm)[0]["rect"]
        arm.shell.terminate()
        arm.shell.wait(timeout=8)
        arm.start_shell()
        wait_for(arm, lambda: pearls(arm), 15, "pearl restored")
        after = pearls(arm)[0]["rect"]
        steps["restored_where_it_was"] = all(abs(a - b) <= 1 for a, b in zip(before, after))
        arm.shot("7-restored.png")
        report["steps"] = steps
        report["log_errors"] = [l for l in (arm.raw / "shell.log").read_text(errors="replace").splitlines()
                                if "PEARL_" in l and "ERROR" in l or "panicked" in l][:20]
        (arm.raw / "result.json").write_text(json.dumps(report, indent=1))
        print(json.dumps(report, indent=1))
    finally:
        if pointer:
            pointer.stop()
        arm.finish()


def percentile(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, len(values) * q // 100)] if values else None


def frame_costs(log, since):
    """Shell-side processing time (ms) of each panel frame after byte offset `since`."""
    return [float(l.split("processing_ms=")[1]) for l in log[since:].decode(errors="replace").splitlines()
            if l.startswith("PANEL_FRAME processing_ms=")]


def drag(binary, pointer_bin, rounds):
    """Pull a pearl across the output, down to the docking edge and back, `rounds` times; report the presented
    cadence of the panel surfaces (pearl and guides) and what each frame cost the shell."""
    OUT.mkdir(exist_ok=True)
    arm = Arm("candidate", binary)
    arm.start("drag")
    pointer = None
    try:
        out = json.loads(arm.niri("outputs"))
        mode = next(iter(out.values())) if isinstance(out, dict) else out[0]
        cur = mode["modes"][mode["current_mode"]]
        W, H = cur["width"], cur["height"]
        pointer = Pointer(arm, str(ROOT / pointer_bin))
        arm.ipc("bead.panel", "open", "launcher")
        time.sleep(1.5)
        arm.ipc("bead.panel", "detach")
        time.sleep(1.0)
        x, y, w, h = pearls(arm)[0]["rect"]
        log = arm.raw / "shell.log"
        gx, gy = int(x + w / 2), int(y + 40)
        # Down to the edge the launcher docks to (so the film grows), then back to the middle.
        waypoints = [(gx, gy), (gx - 120, H // 2), (gx - 120, H - 80), (gx - 120, H // 2)]
        pointer.send(f"move {gx} {gy} {W} {H}")
        time.sleep(.3)
        report = {"output": [W, H], "compositor": os.environ.get("B54_WLR_RENDERER", "pixman"), "rounds": []}
        for r in range(rounds):
            mark = log.stat().st_size
            pointer.send("press")
            n = 0
            for (ax, ay), (bx, by) in zip(waypoints, waypoints[1:]):
                for i in range(1, 61):
                    pointer.send(f"move {ax + (bx - ax) * i // 60} {ay + (by - ay) * i // 60} {W} {H}")
                    time.sleep(.008)
                    n += 1
            pointer.send("release")
            time.sleep(.8)
            data = log.read_bytes()
            presented = presented_intervals(data, mark)
            cost = frame_costs(data, mark)
            report["rounds"].append({"steps": n, "presented_ms": presented, "frames": len(cost),
                                     "frame_cost_ms": {"p50": percentile(cost, 50), "p99": percentile(cost, 99), "max": max(cost, default=None)}})
            state = pearls(arm)
            if not state:
                break
            x, y, w, h = state[0]["rect"]
            gx, gy = int(x + w / 2), int(y + 40)
            pointer.send(f"move {gx} {gy} {W} {H}")
            waypoints = [(gx, gy), (gx - 120, H // 2), (gx - 120, H - 80), (gx - 120, H // 2)]
            time.sleep(.3)
        allp = [r["presented_ms"] for r in report["rounds"] if r["presented_ms"]]
        report["presented_p50_median"] = statistics.median(p["p50"] for p in allp) if allp else None
        report["presented_p99_median"] = statistics.median(p["p99"] for p in allp) if allp else None
        (arm.raw / "result.json").write_text(json.dumps(report, indent=1))
        print(json.dumps(report, indent=1))
    finally:
        if pointer:
            pointer.stop()
        arm.finish()


def probe(binary, label="candidate"):
    """One arm, a snapshot per phase: PSS split, glibc heap, cache budgets (state trace), smaps per phase."""
    OUT.mkdir(exist_ok=True)
    arm = Arm(label, binary)
    arm.start("probe")
    out = []

    def snap(name):
        time.sleep(3)
        st = arm.ipc("bead.panel", "state")["result"]
        (arm.raw / f"smaps-{name}.txt").write_text(open(f"/proc/{arm.shell.pid}/smaps").read())
        caches = {}
        for c in st["cache_bytes"]["caches"]:
            caches[c["name"]] = caches.get(c["name"], 0) + c["used_bytes"]
        out.append({"phase": name, "rollup": rollup(arm.shell.pid), "heap": st.get("heap_bytes"), "caches": caches})
        print(name, {k: round(v, 2) for k, v in out[-1]["rollup"].items()}, "heap", out[-1]["heap"], "caches",
              {k: v for k, v in caches.items() if v > 100_000}, flush=True)

    try:
        time.sleep(8)
        snap("idle")
        arm.ipc("bead.panel", "open", "launcher")
        time.sleep(2)
        snap("docked_open")
        if label == "candidate":
            arm.ipc("bead.panel", "detach")
            time.sleep(1)
            snap("pearl_open")
            arm.ipc("bead.panel", "pearl-close")
        else:
            arm.ipc("bead.panel", "close")
        time.sleep(8)
        snap("closed")
    finally:
        (arm.raw / "probe.json").write_text(json.dumps(out, indent=1))
        arm.finish()


def memory_arm(label, binary, tag):
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
        r = {"label": label, "binary": binary, "pid": arm.shell.pid}
        r["idle"] = phase(10)
        arm.ipc("bead.panel", "open", "launcher")
        r["docked_open"] = phase(4)
        if label == "candidate":
            arm.ipc("bead.panel", "detach")
            r["pearl_open"] = phase(4)
            arm.ipc("bead.panel", "pearl-close")
        else:
            arm.ipc("bead.panel", "close")
        r["closed"] = phase(12)
        (arm.raw / "result.json").write_text(json.dumps(r, indent=1))
        return r
    finally:
        arm.finish()


def memory(pairs, ctl, cand):
    OUT.mkdir(exist_ok=True)
    runs = []
    for i in range(pairs):
        for label, binary in ((("control", ctl), ("candidate", cand)) if i % 2 == 0 else (("candidate", cand), ("control", ctl))):
            runs.append(memory_arm(label, binary, f"memory-{i}-{label}"))
    rep = {"renderer": "cpu (tiny-skia), no daemon"}
    med = lambda label, phase, key="Pss": statistics.median(r[phase][key] for r in runs if r["label"] == label and phase in r)
    for phase in ("idle", "docked_open", "closed"):
        rep[phase] = {"control": med("control", phase), "candidate": med("candidate", phase)}
        rep[phase]["delta"] = rep[phase]["candidate"] - rep[phase]["control"]
    rep["pearl_open"] = med("candidate", "pearl_open")
    rep["pearl_minus_docked_open"] = rep["pearl_open"] - med("candidate", "docked_open")
    rep["closed_anon_delta"] = med("candidate", "closed", "Pss_Anon") - med("control", "closed", "Pss_Anon")
    (OUT / "memory.json").write_text(json.dumps({"summary": rep, "runs": runs}, indent=1))
    print(json.dumps(rep, indent=1))


if __name__ == "__main__":
    if sys.argv[1] == "smoke":
        smoke(sys.argv[2], sys.argv[3])
    elif sys.argv[1] == "memory":
        memory(int(sys.argv[2]), sys.argv[3], sys.argv[4])
    elif sys.argv[1] == "probe":
        probe(sys.argv[2], *sys.argv[3:4])
    elif sys.argv[1] == "drag":
        drag(sys.argv[2], "pointer", int(sys.argv[3]) if len(sys.argv) > 3 else 3)
