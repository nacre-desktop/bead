#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0085 first-key latency gate for a build profile: standalone lane and bar Home->search lane, alternating arms.

  b0085-gate.py run PAIRS CTL_BIN CAND_BIN SRC POINTER_BIN [KEYS]

SMOKE_ROOT as for b0047-frame.py (binaries relative to it); SRC is a checkout holding shell/ and spikes/ (both arms use it, so only the binary differs).
Private nested session per arm (headless labwc > niri > shell), CPU renderer, clocks-only input tracing (BEAD_PROFILE_INPUT=clocks).
  standalone: spikes/typing-proof.py (320 fixture apps, serial a/BackSpace, 40 ms pause); first key and later keys, UI CPU ms.
  home:       `--bar --core-views --menu-fixture` shell, launcher opened over the command socket (starts on Home with 12 favourites and 8 recents),
              then the same serial a/BackSpace keys; the key's search result is a 64-row prefix of 320.
Per key: INPUT_RECEIVED cpu_ns to the UI_PRESENTED cpu_ns of the commit that follows it. Gate: first key <= 8 ms in both lanes.
Never the live session."""
import importlib.util, json, os, pathlib, re, signal, statistics, subprocess, sys, tempfile, time

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("b0065", HERE / "b0065-smaps.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
b = m.b
spec = importlib.util.spec_from_file_location("menu_fixtures", HERE / "menu-fixtures.py")
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)


def stop(proc):
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)


class Sess(b.Arm):
    """b0047-frame.py's Arm session (private dbus, headless labwc, niri), with the shell command line chosen by the caller (None: no shell)."""

    def start(s, tag, shell=None, extra_env=None):
        RUN = b.RUN
        s.raw = b.OUT / tag
        b.shutil.rmtree(s.raw, ignore_errors=True)
        s.raw.mkdir(parents=True)
        mem = b.meminfo()
        assert (mem["SwapTotal"] - mem["SwapFree"]) / 1024 < 64, "swap used >= 64 MiB"
        (s.raw / "free-before.txt").write_text(subprocess.run(["free", "-h"], capture_output=True, text=True).stdout)
        b.shutil.rmtree(RUN, ignore_errors=True)
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
        penv = dict(s.env, XDG_RUNTIME_DIR=str(RUN / "pr"), WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman")
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
        if shell:
            s.shell = s.spawn(shell, "shell.log", dict(s.env, **(extra_env or {})), cwd=str(s.src))
            s.wait(lambda: "START pid=" in (s.raw / "shell.log").read_text(errors="replace"), 30, "shell start")


def fake_target(pointer_bin):
    """typing-proof.py looks for $CARGO_TARGET_DIR/release/examples/private-pointer."""
    root = pathlib.Path(tempfile.gettempdir()) / "b0085-target"
    link = root / "release/examples/private-pointer"
    link.parent.mkdir(parents=True, exist_ok=True)
    link.unlink(missing_ok=True)
    link.symlink_to(pathlib.Path(pointer_bin).resolve())
    return root


def standalone(arm, binary, src, pointer_bin, keys):
    spec = importlib.util.spec_from_file_location("typing_proof", src / "spikes/typing-proof.py")
    tp = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tp)
    os.environ.update(BEAD_HARDENING_BINARY=str(b.ROOT / binary), BEAD_TYPING_PROFILE="clocks", BEAD_TYPING_KEYS=str(keys),
                      CARGO_TARGET_DIR=str(fake_target(pointer_bin)))
    r = tp.sample(dict(arm.env), arm.raw, "niri", stop)
    return [x["cpu_ms"] for x in r["rows"]]


def fixture_file(path, src):
    rows = [{"id": f"proof-{i:04}", "name": f"Application {i:04}", "label": f"Application {i:04}", "icon": str(src / "shell/sample.svg"),
             "kind": "app", "description": "Application fixture", "breadcrumb": "All apps"} for i in range(320)]
    prefs = {"favorites": [r["id"] for r in rows[:12]], "hidden": [], "recent": [r["id"] for r in rows[20:28]], "usage": {}}
    top = lambda q: dict(fixtures.reply(q, rows[:64] if q else rows), total=320)
    path.write_text(json.dumps({"queries": {"": top(""), "a": top("a")}, "preferences": prefs}))


def home(arm, binary, src, pointer_bin, keys):
    log = arm.raw / "shell.log"
    ipc = lambda *a: subprocess.run([str(b.ROOT / binary), "ipc", "--socket", arm.cmdsock, *a], env=arm.env, capture_output=True, text=True,
                                    cwd=str(src), timeout=10)
    ipc("--json", "bead.panel", "open", "launcher")
    end = time.monotonic() + 8
    while "PANEL_PRESENTED" not in log.read_text(errors="replace") and time.monotonic() < end:
        time.sleep(.05)
    time.sleep(1.5)
    pointer = b.Pointer(arm, str(pointer_bin))
    out = []
    try:
        pointer.send("move 1000 300")
        time.sleep(.3)
        for i in range(keys):
            off = len(log.read_text(errors="replace"))
            subprocess.run(["wtype", "-s", "20", *(["-k", "BackSpace"] if i % 2 else ["a"])], env=arm.env, check=True, timeout=5)
            seg = ""
            end = time.monotonic() + 6
            while time.monotonic() < end:
                seg = log.read_text(errors="replace")[off:]
                if "INPUT_RECEIVED" in seg and re.search(r"UI_PRESENTED .*update=\d+ cpu_ns=\d+", seg):
                    break
                time.sleep(.01)
            rec = re.search(r"INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)", seg)
            commits = re.findall(r"UI_COMMIT .*update=(\d+).*source_error=false", seg)
            found = [p for p in re.findall(r"UI_PRESENTED generation=\d+ monotonic_ns=(\d+) refresh_ns=\d+ companion=\w+ update=(\d+) cpu_ns=(\d+)", seg)
                     if p[1] in commits and rec and int(p[0]) >= int(rec[1])]
            if not (rec and found):
                raise RuntimeError(f"home key {i}: no matched presentation")
            out.append((int(found[0][2]) - int(rec[2])) / 1e6)
            time.sleep(.04)
    finally:
        pointer.stop()
    return out


def arm_run(lane, label, binary, src, pointer_bin, keys, tag):
    arm = Sess(label, binary, src)
    try:
        if lane == "standalone":
            arm.start(tag)
            return standalone(arm, binary, src, pointer_bin, keys)
        fx = pathlib.Path(tempfile.mkdtemp(prefix="b0085-fx-")) / "fixture.json"
        fixture_file(fx, src)
        cmd = [str(b.ROOT / binary), "--bar", "--core-views", "--renderer", "cpu", "--panel-renderer", "cpu", "--trace-state", "--command-socket",
               str(b.RUN / "cmd"), "--namespace", b.NS, "--palette", str(src / "shell/palette.json"), "--menu-fixture", str(fx)]
        arm.start(tag, cmd, dict(BEAD_PROFILE_INPUT="clocks", BEAD_PRIVATE_INPUT_PROOF="1", BEAD_MATCHED_ICONS="1",
                                 BEAD_ICON_PATH=str(src / "shell/sample.svg")))
        return home(arm, binary, src, pointer_bin, keys)
    finally:
        arm.finish()


def run(pairs, ctl, cand, src, pointer_bin, keys=60):
    src = pathlib.Path(src).resolve()
    rows = []
    for lane in ("standalone", "home"):
        for i in range(pairs):
            for label, binary in ([("control", ctl), ("candidate", cand)] if i % 2 == 0 else [("candidate", cand), ("control", ctl)]):
                cpu = arm_run(lane, label, binary, src, pointer_bin, keys, f"b0085-{lane}-{i}-{label}")
                rows.append({"lane": lane, "pair": i, "label": label, "first": cpu[0], "later": statistics.median(cpu[1:]), "cpu_ms": cpu})
                print(lane, i, label, "first %.2f later %.2f" % (cpu[0], rows[-1]["later"]), flush=True)
    rep = {}
    for lane in ("standalone", "home"):
        row = {}
        for label in ("control", "candidate"):
            rs = [r for r in rows if r["lane"] == lane and r["label"] == label]
            row[label] = {"first_median": round(statistics.median(r["first"] for r in rs), 3), "first": [round(r["first"], 2) for r in rs],
                          "later_median": round(statistics.median(r["later"] for r in rs), 3)}
        rep[lane] = row
    (b.OUT / "b0085-gate.json").write_text(json.dumps({"summary": rep, "rows": rows}, indent=1))
    print(json.dumps(rep, indent=1))


if __name__ == "__main__" and sys.argv[1] == "run":
    run(int(sys.argv[2]), *sys.argv[3:7], *(int(x) for x in sys.argv[7:8]))
