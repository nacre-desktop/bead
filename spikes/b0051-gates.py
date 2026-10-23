#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0051 gates in a private nested niri (labwc headless parent), CPU renderer, one host.

  motion  the real Nacre daemon publishes `wallpaper.active.motion` from a private media.json
          and a generated clip; the shell must start the renderer, show moving frames, and fall
          back to the still poster when the record goes away. Nothing outside the temp tree is read.
  memory  relative gate, alternating control/candidate arms of the shell (same flags, same
          backend): idle, OSD shown, OSD closed; Pss/Pss_Anon medians per phase.

Environment: SMOKE_ROOT holds src/ (the candidate tree), bin/nacre-helper, bin/<arm>/{nacre-shell,
bead-wallpaper-video,nacre-wallpaper-decode} and nacre/ (a Nacre checkout for shell-tools).
Needs niri, labwc, dbus-daemon, grim, ffmpeg, python3-pillow. Usage:
  b0051-gates.py motion CAND_DIR
  b0051-gates.py osdshot CAND_DIR   (screenshots of the night-light OSD from a daemon event)
  b0051-gates.py memory PAIRS CONTROL_DIR CAND_DIR"""
import json, os, pathlib, shutil, signal, socket, statistics, subprocess, sys, time

ROOT = pathlib.Path(os.environ["SMOKE_ROOT"])
SRC, BIN = ROOT / "src", ROOT / "bin"
RUN = pathlib.Path("/tmp/b0051-run")
OUT = ROOT / "out"
KEYS = ("Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "SwapPss")


def rollup(pid):
    out = {}
    for line in open(f"/proc/{pid}/smaps_rollup"):
        k, _, v = line.partition(":")
        if k in KEYS:
            out[k] = int(v.split()[0]) / 1024.0
    return out


class Session:
    """labwc (headless) > niri (nested) > Nacre daemon with a private HOME; the shell is started by the caller."""

    def __init__(self, tag):
        self.tag, self.procs = tag, []
        self.raw = OUT / tag
        shutil.rmtree(self.raw, ignore_errors=True)
        self.raw.mkdir(parents=True)

    def spawn(self, cmd, log, env, **kw):
        p = subprocess.Popen(cmd, env=env, stdout=(self.raw / log).open("w"), stderr=subprocess.STDOUT,
                             start_new_session=True, **kw)
        self.procs.append(p)
        return p

    def stop(self):
        for p in reversed(self.procs):  # exactly the process groups this session started
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        end = time.monotonic() + 8
        for p in self.procs:
            while p.poll() is None and time.monotonic() < end:
                time.sleep(.05)
            if p.poll() is None:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        shutil.rmtree(RUN, ignore_errors=True)

    def wait(self, cond, secs, what):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            v = cond()
            if v:
                return v
            time.sleep(.05)
        raise RuntimeError("timeout: " + what)

    def start(self, prepare=None):
        shutil.rmtree(RUN, ignore_errors=True)
        for d in ("r", "pr", "state/nacre", "config", "cache", "data", "home", "labwc"):
            (RUN / d).mkdir(parents=True, mode=0o700)
        os.chmod(RUN, 0o700)
        if prepare:
            prepare(RUN / "state/nacre")
        base = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "DBUS_SESSION_BUS_ADDRESS")}
        base.update(XDG_CONFIG_HOME=str(RUN / "config"), XDG_CACHE_HOME=str(RUN / "cache"), XDG_STATE_HOME=str(RUN / "state"),
                    XDG_DATA_HOME=str(RUN / "data"), HOME=str(RUN / "home"), PIPEWIRE_REMOTE="bead-private-unavailable")
        bus = f"unix:path={RUN}/bus"
        self.spawn(["dbus-daemon", "--session", f"--address={bus}", "--nofork", "--nopidfile"], "dbus.log", base)
        self.wait(lambda: (RUN / "bus").exists(), 5, "private bus")
        self.env = env = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "r"))
        penv = dict(base, DBUS_SESSION_BUS_ADDRESS=bus, XDG_RUNTIME_DIR=str(RUN / "pr"), WLR_BACKENDS="headless",
                    WLR_HEADLESS_OUTPUTS="1", WLR_RENDERER="pixman", WLR_LIBINPUT_NO_DEVICES="1")
        (RUN / "labwc/autostart").write_text("")
        self.spawn(["labwc", "--config-dir", str(RUN / "labwc")], "labwc.log", penv)
        self.parent = dict(penv, WAYLAND_DISPLAY=str(self.wait(
            lambda: next((p for p in (RUN / "pr").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "labwc")))
        env["WAYLAND_DISPLAY"] = self.parent["WAYLAND_DISPLAY"]
        env.update(LIBGL_ALWAYS_SOFTWARE="1")
        self.spawn(["niri", "--config", str(SRC / "spikes/niri.kdl")], "niri.log", env)
        env["WAYLAND_DISPLAY"] = str(self.wait(
            lambda: next((p for p in (RUN / "r").glob("wayland-*") if not str(p).endswith(".lock")), None), 20, "niri"))
        env["NIRI_SOCKET"] = str(self.wait(lambda: next(iter((RUN / "r").glob("niri.*.sock")), None), 10, "niri ipc"))
        self.sock = str(RUN / "d.sock")
        # No audio server in the container: `pactl subscribe` would exit at once and the daemon's `osd`
        # service (all kinds) restarts until disabled. A silent stand-in keeps the other kinds alive.
        (RUN / "fakebin").mkdir(mode=0o700)
        (RUN / "fakebin/pactl").write_text("#!/bin/sh\nexec sleep 100000\n")
        (RUN / "fakebin/pactl").chmod(0o700)
        denv = dict(env, NACRE_DAEMON_SOCKET=self.sock, NACRE_HELPER_TOOLS=str(ROOT / "nacre/nacre/shell-tools"),
                    NACRE_COMPOSITOR="niri", RUST_LOG="warn",
                    # The `osd` producer needs a system bus (power profile); the private session bus stands in.
                    DBUS_SYSTEM_BUS_ADDRESS=bus, PATH=f"{RUN}/fakebin:" + os.environ["PATH"])
        self.spawn([str(BIN / "nacre-helper"), "daemon", "--notifications=daemon"], "daemon.log", denv)
        self.wait(lambda: os.path.exists(self.sock), 15, "daemon socket")

    def shell(self, arm):
        cmd = [str(BIN / arm / "nacre-shell"), "--bar", "--core-views", "--notices", "--renderer", "cpu", "--panel-renderer", "cpu",
               "--daemon-socket", self.sock, "--ui-provider", "--command-socket", str(RUN / "cmd"),
               "--namespace", "bead-b0051", "--palette", str(SRC / "shell/palette.json")]
        p = self.spawn(cmd, "shell.log", self.env, cwd=str(SRC))
        self.wait(lambda: "UI_SESSION_CREATED" in self.log(), 40, "bar session")
        return p

    def log(self, name="shell.log"):
        return (self.raw / name).read_text(errors="replace")

    def daemon_call(self, topic):
        """First snapshot of a daemon topic over the raw protocol."""
        s = socket.socket(socket.AF_UNIX)
        s.connect(self.sock)
        f = s.makefile("rwb", buffering=0)
        f.write((json.dumps({"t": "hello", "proto": 1, "min": 1, "client": "b0051-gates"}) + "\n").encode())
        f.readline()
        f.write((json.dumps({"t": "sub", "topics": [topic]}) + "\n").encode())
        end = time.monotonic() + 5
        while time.monotonic() < end:
            msg = json.loads(f.readline())
            if msg.get("t") == "snap" and msg.get("topic") == topic:
                s.close()
                return msg["data"]
        s.close()
        raise RuntimeError("no snapshot of " + topic)

    def grim(self, name):
        path = self.raw / name
        subprocess.run(["grim", str(path)], env=self.parent, check=True, timeout=20)
        return path


def changed(diff):
    return diff.convert("L").point(lambda v: 255 if v > 24 else 0).histogram()[255]


def motion(arm):
    from PIL import Image, ImageChops
    work = RUN.parent / "b0051-media"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(mode=0o700)
    poster, clip = work / "poster.png", work / "clip.mp4"
    Image.linear_gradient("L").resize((960, 540)).convert("RGB").save(poster)
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=960x540:r=15", "-t", "40",
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15", str(clip)], check=True)
    for p in (poster, clip):
        p.chmod(0o600)

    def prepare(state):
        record = json.load(open(ROOT / "nacre/nacre/shell-tools/reference-style.json"))
        record.update(poster=str(poster), mode="dark")
        (state / "presentation.json").write_text(json.dumps(record))
        (state / "presentation.json").chmod(0o600)
        (state / "wallpaper").mkdir(mode=0o700)
        media = state / "wallpaper/media.json"
        media.write_text(json.dumps({"path": str(clip), "poster": str(poster), "dynamic": True, "animated": False}))
        media.chmod(0o600)

    s = Session("motion")
    result = {"arm": arm, "renderer": "cpu (libmpv software render, wl_shm); shell CPU tiny-skia"}
    try:
        s.start(prepare)
        snap = s.wait(lambda: (lambda d: d if d.get("available") and d.get("current") else None)(s.daemon_call("wallpaper.active")), 20, "wallpaper.active")
        result["daemon_motion"] = (snap["current"] or {}).get("motion", {}).get("kind")
        result["daemon_outputs"] = sorted(snap["outputs"])
        s.shell(arm)
        s.wait(lambda: "WALLPAPER_VIDEO_FIRST_FRAME" in s.log(), 40, "first video frame")
        result["first_frame"] = True
        time.sleep(2)
        a, b = s.grim("play-a.png"), None
        time.sleep(1.2)
        b = s.grim("play-b.png")
        diff = ImageChops.difference(Image.open(a).convert("RGB"), Image.open(b).convert("RGB"))
        result["moving_pixels"] = changed(diff)
        # The record goes away: the daemon republishes without motion and the poster stays.
        (RUN / "state/nacre/wallpaper/media.json").unlink()
        s.wait(lambda: not (s.daemon_call("wallpaper.active")["current"] or {}).get("motion"), 15, "motion removed")
        result["daemon_motion_removed"] = True
        time.sleep(6)
        c = s.grim("still-a.png")
        time.sleep(1.2)
        d = s.grim("still-b.png")
        diff = ImageChops.difference(Image.open(c).convert("RGB"), Image.open(d).convert("RGB"))
        result["still_moving_pixels"] = changed(diff)
        result["pass"] = bool(result["daemon_motion"] == "video" and result["moving_pixels"] > 2000 and result["still_moving_pixels"] < 200)
        result["shell_alive"] = s.procs[-1].poll() is None
    finally:
        s.stop()
        shutil.rmtree(work, ignore_errors=True)
    (OUT / "motion.json").write_text(json.dumps(result, indent=1))
    print(json.dumps(result))
    return 0 if result.get("pass") else 1


def osd_shot(arm):
    """Night light toggled through the daemon's private control record: the OSD must appear on screen."""
    s = Session("osdshot")
    try:
        s.start()
        night_light(False)
        s.shell(arm)
        time.sleep(4)
        for i in range(2):
            night_light(i == 0)
            time.sleep(2.0)
            s.grim(f"osd-{i}.png")
        print("saved", s.raw)
    finally:
        s.stop()
    return 0


def night_light(on):
    path = RUN / "state/nacre/control-tools.json"
    path.write_text(json.dumps({"enabled": on, "owner": "b0051" if on else ""}))
    path.chmod(0o600)


def command(arm, method, args):
    s = socket.socket(socket.AF_UNIX)
    s.connect(str(RUN / "cmd"))
    s.sendall((json.dumps({"version": 1, "id": 1, "target": "bead.osd", "method": method, "args": args}) + "\n").encode())
    reply = s.makefile().readline()
    s.close()
    return reply


def memory_arm(label, arm, tag):
    s = Session(tag)
    try:
        s.start()
        night_light(False)
        shell = s.shell(arm)
        pid = shell.pid

        def phase(warm):
            time.sleep(warm)
            samples = []
            for _ in range(5):
                samples.append(rollup(pid))
                time.sleep(1)
            return {k: statistics.median(x[k] for x in samples) for k in KEYS}

        out = {"label": label, "arm": arm}
        out["idle"] = phase(10)
        # Keep the OSD up: a new change every second restarts its 2 s hide timer.
        samples = []
        for i in range(8):
            if label == "candidate":
                night_light(i % 2 == 0)  # daemon `osd` event: night_light on/off
            else:
                command(arm, "show", ["brightness", 0.4, False])  # control: the old key-route feedback
            time.sleep(1)
            if i >= 3:
                samples.append(rollup(pid))
        out["shown"] = {k: statistics.median(x[k] for x in samples) for k in KEYS}
        out["closed"] = phase(8)
        out["shell_alive"] = shell.poll() is None
        return out
    finally:
        s.stop()


def memory(pairs, control, candidate):
    runs = {"control": [], "candidate": []}
    for i in range(pairs):
        order = [("control", control), ("candidate", candidate)] if i % 2 == 0 else [("candidate", candidate), ("control", control)]
        for label, arm in order:
            print(f"ARM pair={i} {label}", flush=True)
            runs[label].append(memory_arm(label, arm, f"pair{i}-{label}"))
            time.sleep(3)
    report = {"pairs": pairs, "renderer": "cpu (tiny-skia)", "arms": runs, "delta": {}}
    for phase in ("idle", "shown", "closed"):
        for key in KEYS:
            c = statistics.median(r[phase][key] for r in runs["control"])
            n = statistics.median(r[phase][key] for r in runs["candidate"])
            report["delta"].setdefault(phase, {})[key] = {"control": round(c, 3), "candidate": round(n, 3), "delta": round(n - c, 3)}
    (OUT / "report.json").write_text(json.dumps(report, indent=1))
    for phase, keys in report["delta"].items():
        print(phase, {k: v["delta"] for k, v in keys.items()}, "control_pss", keys["Pss"]["control"], "candidate_pss", keys["Pss"]["candidate"])
    return 0


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    mode = sys.argv[1]
    sys.exit(motion(sys.argv[2]) if mode == "motion" else osd_shot(sys.argv[2]) if mode == "osdshot" else memory(int(sys.argv[2]), sys.argv[3], sys.argv[4]))
