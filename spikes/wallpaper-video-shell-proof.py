#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0046 relative memory/CPU gate: the shell with a playing video wallpaper against a control
shell with the same still poster, in a private headless sway. Both run the same timeline
(steady, user-paused, held until the decoder is released, resumed); the shell's own PSS must
not grow by more than 0.5 MiB at the median per phase. The renderer process is measured
separately (PSS and CPU while playing, paused and released).
"""
import argparse, json, os, shutil, signal, statistics, subprocess, sys, tempfile, threading, time
from pathlib import Path
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("--shell", default="target/release/nacre-shell")
ap.add_argument("--runs", type=int, default=3)
ap.add_argument("--size", default="1920x1080")
ap.add_argument("--keep", action="store_true")
ap.add_argument("--hold-seconds", type=float, default=33.0, help="paused time before the decoder is released (policy: 30 s)")
args = ap.parse_args()
here = Path(__file__).resolve().parent
shell = str(Path(args.shell).resolve())
root = Path(tempfile.mkdtemp(prefix="bead-wps-"))
root.chmod(0o700)
run_dir = root / "run"
run_dir.mkdir(mode=0o700)
env = dict(os.environ, XDG_RUNTIME_DIR=str(run_dir), WLR_BACKENDS="headless", WLR_RENDERER="pixman",
           WLR_LIBINPUT_NO_DEVICES="1", HOME=str(root / "home"))
(root / "home").mkdir()
env.pop("WAYLAND_DISPLAY", None)
env.pop("DISPLAY", None)
w, h = map(int, args.size.split("x"))
Image.linear_gradient("L").resize((w, h)).convert("RGB").save(root / "poster.png")
(root / "poster.png").chmod(0o600)
subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", f"testsrc2=s={w}x{h}:r=30", "-t", "20",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "30", str(root / "clip.mp4")], check=True)
(root / "clip.mp4").chmod(0o600)
(root / "sway.conf").write_text(f"output * resolution {args.size}\n")
procs = []
sway = subprocess.Popen(["sway", "--unsupported-gpu", "-c", str(root / "sway.conf")], env=env,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
procs.append(sway)
for _ in range(100):
    found = [p.name for p in run_dir.glob("wayland-*") if not p.name.endswith(".lock")]
    if found:
        env["WAYLAND_DISPLAY"] = found[0]
        break
    time.sleep(0.1)
else:
    sys.exit("sway did not start")
time.sleep(0.5)


def rollup(pid):
    values = {}
    for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        key, _, rest = line.partition(":")
        if rest.strip().endswith("kB"):
            values[key] = int(rest.split()[0])
    return values


def ticks(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])


def cpu(pid, seconds):
    a, t = ticks(pid), time.time()
    time.sleep(seconds)
    return (ticks(pid) - a) / os.sysconf("SC_CLK_TCK") / (time.time() - t) * 100


def sample(pid, count=5):
    pss, anon = [], []
    for _ in range(count):
        r = rollup(pid)
        pss.append(r["Pss"] / 1024)
        anon.append(r.get("Pss_Anon", 0) / 1024)
        time.sleep(1)
    return statistics.median(pss), statistics.median(anon)


def one_run(label, motion, name):
    control = root / f"{name}.json"
    control.write_text(json.dumps({"motion": motion, "picker": {"paused": False, "motionMode": "full"}}))
    sock = root / f"{name}.sock"
    daemon = subprocess.Popen([sys.executable, str(here / "fake-wallpaper-daemon.py"), str(sock), str(control), "HEADLESS-1",
                               str(root / "poster.png"), str(root / "clip.mp4")])
    procs.append(daemon)
    for _ in range(50):
        if sock.exists():
            break
        time.sleep(0.1)
    log = root / f"{name}.log"
    shell_proc = subprocess.Popen([shell, "--daemon-socket", str(sock)], env=env, stdout=open(log, "w"), stderr=subprocess.STDOUT)
    procs.append(shell_proc)

    def tokens():
        return log.read_text(errors="replace")

    def wait_for(text, seconds=40):
        end = time.time() + seconds
        while time.time() < end:
            if text in tokens():
                return True
            if shell_proc.poll() is not None:
                return False
            time.sleep(0.2)
        return False

    result = {"label": label}
    result["poster_committed"] = wait_for("WALLPAPER_SURFACE_COMMITTED")
    renderer_pid = None
    if motion:
        result["first_frame"] = wait_for("WALLPAPER_VIDEO_FIRST_FRAME")
        for line in tokens().splitlines():
            if "WALLPAPER_VIDEO_READY pid=" in line:
                renderer_pid = int(line.split("pid=")[1])
            if "WALLPAPER_VIDEO_HWDEC" in line:
                result["hwdec"] = line.split("mode=")[1]
    time.sleep(6)
    spid = shell_proc.pid
    phases = {}
    phases["steady"] = sample(spid)
    if renderer_pid:
        result["renderer_playing_cpu_pct"] = round(cpu(renderer_pid, 3), 1)
        result["renderer_playing_pss_mib"] = round(rollup(renderer_pid)["Pss"] / 1024, 1)
    t_pause = time.time()
    control.write_text(json.dumps({"motion": motion, "picker": {"paused": True, "motionMode": "full"}}))
    time.sleep(3)
    phases["paused"] = sample(spid, 3)
    if renderer_pid:
        result["renderer_paused_cpu_pct"] = round(cpu(renderer_pid, 3), 1)
        result["renderer_paused_pss_mib"] = round(rollup(renderer_pid)["Pss"] / 1024, 1)
    time.sleep(max(0, args.hold_seconds - (time.time() - t_pause)))
    phases["released"] = sample(spid, 3)
    if renderer_pid:
        result["renderer_released_cpu_pct"] = round(cpu(renderer_pid, 3), 1)
        result["renderer_released_pss_mib"] = round(rollup(renderer_pid)["Pss"] / 1024, 1)
    control.write_text(json.dumps({"motion": motion, "picker": {"paused": False, "motionMode": "full"}}))
    time.sleep(3)
    phases["resumed"] = sample(spid, 3)
    if renderer_pid:
        result["renderer_resumed_cpu_pct"] = round(cpu(renderer_pid, 3), 1)
        result["renderer_resumed_pss_mib"] = round(rollup(renderer_pid)["Pss"] / 1024, 1)
    result["shell_pss_mib"] = {k: round(v[0], 2) for k, v in phases.items()}
    result["shell_anon_mib"] = {k: round(v[1], 2) for k, v in phases.items()}
    result["shell_alive"] = shell_proc.poll() is None
    # Exactly the pids this script started.
    shell_proc.terminate()
    try:
        shell_proc.wait(10)
    except subprocess.TimeoutExpired:
        shell_proc.kill()
    daemon.terminate()
    return result


results = {"control": [], "video": []}
try:
    for i in range(args.runs):
        results["control"].append(one_run("control", False, f"control{i}"))
        results["video"].append(one_run("video", True, f"video{i}"))
finally:
    for p in reversed(procs):
        if p.poll() is None:
            p.terminate()
            try:
                p.wait(5)
            except subprocess.TimeoutExpired:
                p.kill()
    if args.keep:
        print("kept", root, file=sys.stderr)
    else:
        shutil.rmtree(root, ignore_errors=True)

summary = {"backend": "shell CPU wl_shm poster; renderer libmpv sw render", "output": args.size, "runs": args.runs}
gate = {}
for phase in ("steady", "paused", "released", "resumed"):
    c = statistics.median(r["shell_pss_mib"][phase] for r in results["control"])
    v = statistics.median(r["shell_pss_mib"][phase] for r in results["video"])
    ca = statistics.median(r["shell_anon_mib"][phase] for r in results["control"])
    va = statistics.median(r["shell_anon_mib"][phase] for r in results["video"])
    gate[phase] = {"control_pss": round(c, 2), "video_pss": round(v, 2), "delta_pss": round(v - c, 2),
                   "delta_anon": round(va - ca, 2), "pass": (v - c) <= 0.5 and (va - ca) <= 1.0}
summary["shell_gate"] = gate
for key in ("renderer_playing_cpu_pct", "renderer_playing_pss_mib", "renderer_paused_cpu_pct", "renderer_paused_pss_mib",
            "renderer_released_cpu_pct", "renderer_released_pss_mib", "renderer_resumed_cpu_pct", "renderer_resumed_pss_mib"):
    values = [r[key] for r in results["video"] if key in r]
    summary[key] = statistics.median(values) if values else None
summary["hwdec"] = [r.get("hwdec") for r in results["video"]]
summary["all_alive_and_framed"] = all(r["shell_alive"] and r["poster_committed"] for r in results["control"] + results["video"]) and all(r.get("first_frame") for r in results["video"])
print(json.dumps(summary))
sys.exit(0 if summary["all_alive_and_framed"] and all(g["pass"] for g in gate.values()) else 1)
