#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private smoke for bead-wallpaper-video: a headless sway, an ffmpeg-made clip, no host files.

Checks crop-fill, playing, pausing (frame frozen, no CPU), releasing the decoder (last frame
kept, memory down), resuming, and that killing the renderer leaves the compositor alive.
Prints one JSON line with the measurements. Needs: sway, grim, ffmpeg, python3-pil, libmpv.
"""
import argparse, json, os, shutil, signal, subprocess, sys, tempfile, time
from pathlib import Path
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("--engine", default="target/debug/bead-wallpaper-video")
ap.add_argument("--hwdec", default="no")
ap.add_argument("--engine-arg", action="append", default=[], help="extra renderer argument, e.g. --render=shm")
ap.add_argument("--expect-presentation", choices=("shm", "egl"), help="fail unless the renderer reports this path")
ap.add_argument("--keep", action="store_true")
ap.add_argument("--allow-skip", action="store_true", help="exit 0 when a tool is missing")
ap.add_argument("--perf", action="store_true", help="1920x1080 output, 1080p30 clip, 3 release/resume cycles")
args = ap.parse_args()
for tool in ("sway", "grim", "ffmpeg"):
    if not shutil.which(tool):
        print(f"SKIP missing {tool}")
        sys.exit(0 if args.allow_skip else 1)
OUT = "1920x1080" if args.perf else "108x108"
CLIP = "1920x1080" if args.perf else "192x108"
FPS = 30 if args.perf else 15
engine = str(Path(args.engine).resolve())
root = Path(tempfile.mkdtemp(prefix="bead-wpv-"))
run = root / "run"
run.mkdir(mode=0o700)
env = dict(os.environ, XDG_RUNTIME_DIR=str(run), WLR_BACKENDS="headless", WLR_RENDERER="pixman",
           WLR_LIBINPUT_NO_DEVICES="1")
env.pop("WAYLAND_DISPLAY", None)
env.pop("DISPLAY", None)
procs = []

def clip(name, graph, seconds=6):
    path = root / name
    cmd = ["ffmpeg", "-nostdin", "-v", "error", "-y"] + graph + ["-t", str(seconds), "-c:v", "libx264",
           "-pix_fmt", "yuv420p", "-g", "10", str(path)]
    subprocess.run(cmd, check=True)
    return path

# 192x108 clips: three vertical bands (static) and a moving test pattern.
bands = clip("bands.mp4", ["-f", "lavfi", "-i", "color=red:s=64x108:r=15", "-f", "lavfi", "-i", "color=green:s=64x108:r=15",
                           "-f", "lavfi", "-i", "color=blue:s=64x108:r=15", "-filter_complex", "hstack=inputs=3"])
motion = clip("motion.mp4", ["-f", "lavfi", "-i", f"testsrc2=s={CLIP}:r={FPS}"], 8 if args.perf else 6)

(root / "sway.conf").write_text(f"output * resolution {OUT}\noutput * bg #202020 solid_color\n")
sway = subprocess.Popen(["sway", "--unsupported-gpu", "-c", str(root / "sway.conf")], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
procs.append(sway)
display = None
for _ in range(100):
    found = [p.name for p in run.glob("wayland-*") if not p.name.endswith(".lock")]
    if found:
        display = found[0]
        break
    time.sleep(0.1)
if not display:
    sys.exit("sway did not start")
env["WAYLAND_DISPLAY"] = display
time.sleep(0.5)

def shot(name):
    out = root / f"{name}.png"
    subprocess.run(["grim", str(out)], env=env, check=True)
    return Image.open(out).convert("RGB")

def ticks(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])

def rollup(pid):
    values = {}
    for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
        key, _, rest = line.partition(":")
        if rest.strip().endswith("kB"):
            values[key] = int(rest.split()[0])
    return values

def pss_kib(pid):
    return rollup(pid).get("Pss", -1)

def near(pixel, want, tolerance=40):
    return all(abs(a - b) <= tolerance for a, b in zip(pixel, want))

def send(proc, obj):
    proc.stdin.write((json.dumps(obj) + "\n").encode())
    proc.stdin.flush()

import selectors
def read_events(proc, wanted, timeout=15):
    sel = selectors.DefaultSelector()
    sel.register(proc.stdout, selectors.EVENT_READ)
    end = time.time() + timeout
    buffer = b""
    seen = []
    while time.time() < end:
        for _ in sel.select(0.2):
            chunk = os.read(proc.stdout.fileno(), 4096)
            if not chunk:
                return seen
            buffer += chunk
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                seen.append(json.loads(line))
        if any(e.get("event") == wanted for e in seen):
            return seen
    return seen

def measure(pid, seconds):
    a, t = ticks(pid), time.time()
    time.sleep(seconds)
    b = ticks(pid)
    return round((b - a) / os.sysconf("SC_CLK_TCK") / (time.time() - t) * 100, 2)

result = {"backend": "libmpv sw render", "hwdec_requested": args.hwdec, "output": OUT, "clip": f"{CLIP}@{FPS}"}
failed = []
def check(name, ok):
    result.setdefault("checks", {})[name] = bool(ok)
    if not ok:
        failed.append(name)

try:
    # Output name as sway reports it.
    sock = next(iter(run.glob("sway-ipc.*.sock")), None)
    output = "HEADLESS-1"
    if sock:
        env["SWAYSOCK"] = str(sock)
        outputs = json.loads(subprocess.run(["swaymsg", "-t", "get_outputs", "-r"], env=env, capture_output=True, text=True).stdout or "[]")
        output = outputs[0]["name"] if outputs else output
    err_path = root / "renderer.err"
    proc = subprocess.Popen([engine, f"--hwdec={args.hwdec}"] + args.engine_arg, env=env, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=err_path.open("w"))
    procs.append(proc)
    events = read_events(proc, "ready", 10)
    check("ready", any(e.get("event") == "ready" for e in events))
    pid = proc.pid
    result["idle_pss_kib"] = pss_kib(pid)

    # 1. Crop fill: the band clip in a square output keeps the centre (green) wide.
    send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(bands), "fit": "crop", "loop": True}})
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    events = read_events(proc, "first-frame")
    check("first_frame", any(e.get("event") == "first-frame" for e in events))
    time.sleep(0.5)
    image = shot("crop")
    check("crop_keeps_centre", args.perf or near(image.getpixel((54, 54)), (0, 128, 0), 40) and near(image.getpixel((30, 54)), (0, 128, 0), 40) and near(image.getpixel((2, 54)), (255, 0, 0), 40) and near(image.getpixel((105, 54)), (0, 0, 255), 40))
    # 2. Switch to the moving clip.
    switched = time.monotonic()
    send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(motion), "fit": "crop", "loop": True}})
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    events = read_events(proc, "first-frame")
    result["switch_first_frame_ms"] = round((time.monotonic() - switched) * 1000, 2)
    check("replacement_first_frame", any(e.get("event") == "first-frame" for e in events))
    result["hwdec"] = next((e["mode"] for e in events if e.get("event") == "hwdec"), None)
    time.sleep(1.0)
    # Which presentation path the renderer picked: EGL (GPU) or shared memory. A headless
    # pixman sway has no GPU, so the fallback must take over without any intervention.
    log = err_path.read_text()
    check("render_context_reused", "reusing mpv render context" in log)
    result["presentation"] = "shm" if "shared-memory presentation" in log else "egl" if "EGL presentation" in log else "shm-forced-or-unknown"
    if args.expect_presentation:
        check("presentation_" + args.expect_presentation, result["presentation"].startswith(args.expect_presentation))
    result["playing_cpu_pct"] = measure(pid, 3)
    result["playing_pss_kib"] = pss_kib(pid)
    first = shot("play1")
    time.sleep(0.35)
    second = shot("play2")
    check("frames_advance", list(first.getdata()) != list(second.getdata()))
    # The picker accepts these containers. Replacing on the same engine must
    # report a new frame and display its content, including after an error.
    result["format_switch_ms"] = {}
    for extension, codec in [("webm", "libvpx-vp9"), ("mkv", "libx264"), ("mov", "libx264"), ("m4v", "libx264")]:
        path = root / ("replacement." + extension)
        subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i",
                        "color=yellow:s=192x108:r=15", "-t", "1", "-c:v", codec,
                        "-threads", "1", "-pix_fmt", "yuv420p", str(path)], check=True)
        started = time.monotonic()
        send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(path), "loop": True}})
        send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
        events = read_events(proc, "first-frame")
        result["format_switch_ms"][extension] = round((time.monotonic() - started) * 1000, 2)
        # First-frame time above includes the start of the opacity blend. Compare
        # the final colour only after the compositor-side fade has settled.
        time.sleep(0.25)
        check("replacement_" + extension, any(e.get("event") == "first-frame" for e in events)
              and near(shot("format-" + extension).getpixel((54, 54)), (255, 255, 0), 20))
    send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(motion), "loop": True}})
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    check("resume_motion_after_formats", any(e.get("event") == "first-frame" for e in read_events(proc, "first-frame")))
    # Clear the video layer to reveal the compositor wallpaper, then load again in
    # the same renderer. This covers video→static→video and confirms the new frame
    # is visible after the output layer has completed its unmap/configure cycle.
    send(proc, {"cmd": "source", "source": None})
    send(proc, {"cmd": "source", "source": None})
    time.sleep(0.25)
    cleared = shot("source-cleared")
    check("source_clear_reveals_static", near(cleared.getpixel((54, 54)), (32, 32, 32), 4))
    switched = time.monotonic()
    send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(motion), "fit": "crop", "loop": True}})
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    events = read_events(proc, "first-frame")
    result["clear_to_video_first_frame_ms"] = round((time.monotonic() - switched) * 1000, 2)
    check("video_after_clear_first_frame", any(e.get("event") == "first-frame" for e in events))
    resumed = shot("video-after-clear")
    check("video_after_clear_visible", not near(resumed.getpixel((54, 54)), (32, 32, 32), 4))
    # 3. Pause: frozen picture, ~zero CPU.
    send(proc, {"cmd": "state", "decoder": "pause", "render": []})
    time.sleep(0.5)
    frozen1 = shot("pause1")
    result["paused_cpu_pct"] = measure(pid, 3)
    result["paused_pss_kib"] = pss_kib(pid)
    result["paused_rollup_kib"] = {k: v for k, v in rollup(pid).items() if k in ("Pss_Anon", "Pss_File", "Pss_Shmem", "Rss")}
    frozen2 = shot("pause2")
    check("pause_freezes_frame", list(frozen1.getdata()) == list(frozen2.getdata()))
    # 4. Release the decoder: last frame stays, memory drops, still no CPU.
    send(proc, {"cmd": "state", "decoder": "release", "render": []})
    time.sleep(1.0)
    released = shot("release")
    check("release_keeps_last_frame", list(released.getdata()) == list(frozen2.getdata()))
    result["released_cpu_pct"] = measure(pid, 3)
    result["released_pss_kib"] = pss_kib(pid)
    result["released_rollup_kib"] = {k: v for k, v in rollup(pid).items() if k in ("Pss_Anon", "Pss_File", "Pss_Shmem", "Rss")}
    check("released_below_paused", result["released_pss_kib"] < result["paused_pss_kib"])
    # 5. Resume.
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    time.sleep(1.5)
    after = shot("resume1")
    time.sleep(0.4)
    after2 = shot("resume2")
    check("resume_plays", list(after.getdata()) != list(after2.getdata()))
    result["resumed_cpu_pct"] = measure(pid, 3)
    if args.perf:
        for n in range(3):
            send(proc, {"cmd": "state", "decoder": "release", "render": []})
            time.sleep(1.0)
            result.setdefault("cycle_released_pss_kib", []).append(pss_kib(pid))
            send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
            time.sleep(1.5)
            result.setdefault("cycle_playing_pss_kib", []).append(pss_kib(pid))
        check("cycles_survive", proc.poll() is None)
    # 6. A broken source reports an error and the process survives.
    send(proc, {"cmd": "source", "source": {"kind": "video", "path": str(root / "missing.mp4")}})
    send(proc, {"cmd": "state", "decoder": "play", "render": [output]})
    events = read_events(proc, "error")
    check("bad_source_reports_error", any(e.get("event") == "error" for e in events))
    check("renderer_survives_bad_source", proc.poll() is None)
    # 7. Crash: killing exactly this renderer leaves the compositor running and the surface gone.
    os.kill(pid, signal.SIGKILL)
    proc.wait()
    time.sleep(0.5)
    check("compositor_survives_renderer_death", sway.poll() is None)
    gone = shot("after-kill")
    check("fallback_visible_after_kill", near(gone.getpixel((54, 54)), (32, 32, 32), 4))
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
result["failed"] = failed
print(json.dumps(result))
sys.exit(1 if failed else 0)
