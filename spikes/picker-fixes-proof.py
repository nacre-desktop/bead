#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0109 input and timing cases on the existing guarded parity driver (desktop only)."""
import importlib.util
import json
import subprocess
import sys
import threading
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("parity", Path(__file__).with_name("parity-compare.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__

# Prepare outside the recorded interval; exercise the actual controls, not a
# preference write masquerading as a toolbar click.
for name, x, key, value in [
    ("spotlight", 988, "layout", "spotlight"),
    ("hexagons", 1068, "layout", "hexagons"),
    ("dynamic", 815, "kind", "dynamic"),
]:
    state = "wallpaper-button-" + name
    m.STATE_PREPARE[state] = [
        ("wallpaper-pref", "static", "carousel"),
        ("ui", "nacre", "launcher", ">wallpaper "), ("sleep", 3),
    ]
    m.STATES[state] = ([("press", x, 900), ("release",), ("sleep", .7),
                        ("preference-assert", key, value)], [("ui", "nacre", "close")], 1.5)

m.STATE_PREPARE["wallpaper-travel"] = [
    ("wallpaper-pref", "static", "carousel"),
    ("ui", "nacre", "launcher", ">wallpaper "), ("sleep", 3),
]
m.STATES["wallpaper-travel"] = ([("selection-key", "Right")], [("ui", "nacre", "close")], 2)
m.STATE_PREPARE["wallpaper-travel-burst"] = list(m.STATE_PREPARE["wallpaper-travel"])
m.STATES["wallpaper-travel-burst"] = (
    [("selection-key", "Right"), ("sleep", .06), ("selection-key", "Right"), ("sleep", .06),
     ("selection-key", "Right"), ("sleep", 4), ("active-assert", "generated-4.png")],
    [("ui", "nacre", "close")], 2)
m.STATES["current-scene"] = (
    [("only", "qml", ("qml_settings", "wallpaper")),
     ("only", "bead", ("shell", "nacre", "settings", "wallpaper")), ("sleep", 3)],
    [("app_close", "settings")], 0)

# Use the public window and compositor state for this page; a normal main shell
# does not enable the pearl-drag input probe or its inherited private window RPC.
_scene_open = [*m.STATES["current-scene"][0], ("scene-window-assert",)]
m.STATES["current-scene"] = (_scene_open, [("app_close", "settings")], 0)
m.STATE_SETTLE["current-scene"] = 0
for name, mode in [("current-scene-hover", "pointer"), ("current-scene-pressed", "press")]:
    m.STATES[name] = ([*_scene_open, (mode, 1420, 852)], [("release",), ("ui", "nacre", "close"), ("app_close", "settings")], 0)
    m.STATE_SETTLE[name] = 0
m.STATES["current-scene-picker"] = ([*_scene_open, ("press", 1420, 852), ("release",),
                                   ("sleep", 1), ("picker-open-assert",)],
                                  [("ui", "nacre", "close"), ("app_close", "settings")], 0)
m.STATE_SETTLE["current-scene-picker"] = 0

for index, extension in [(1, "mp4"), (2, "webm"), (3, "mkv"), (4, "mov"), (5, "m4v")]:
    state = "wallpaper-format-" + extension
    m.STATE_PREPARE[state] = [
        ("wallpaper-pref", "dynamic", "carousel"),
        ("ui", "nacre", "launcher", ">wallpaper "), ("sleep", 3),
        ("press", 960, 944), ("release",), ("type", "motion-" + str(index)),
    ]
    m.STATES[state] = ([("selection-key", "Right"), ("key", "Return"), ("sleep", 4),
                       ("video-assert", f"motion-{index}.{extension}")], [("ui", "nacre", "close")], 1.5 if extension in ("mp4", "webm") else 0)

# Open/closed memory phases reuse the same guarded driver, waiting seven seconds
# after closing before the driver's additional closed-settle interval.
for state, kind, layout, operations in [
    ("memory-carousel", "static", "carousel", [("selection-key", "Right"), ("sleep", .4), ("press", 960, 944), ("release",),
                                                   ("type", "generated-2"), ("selection-key", "Right"), ("sleep", 4)]),
    ("memory-spotlight", "static", "spotlight", [("sleep", 3)]),
    ("memory-hexagons", "static", "hexagons", [("sleep", 3)]),
    ("memory-dynamic", "dynamic", "carousel", [("press", 960, 944), ("release",), ("type", "motion-2"),
                                                ("selection-key", "Right"), ("sleep", 4)]),
]:
    if state == "memory-carousel": operations.append(("active-assert", "generated-2.jpg"))
    if state == "memory-dynamic": operations.append(("active-assert", "motion-2.webm"))
    m.STATE_PREPARE[state] = [("wallpaper-pref", kind, layout)]
    m.STATES[state] = ([("ui", "nacre", "launcher", ">wallpaper "), ("sleep", 2), *operations],
                      [("ui", "nacre", "close"), ("sleep", 7), ("closed-assert",)], 0)
    m.STATE_SETTLE[state] = 0

m.STATES["memory-scene"] = (_scene_open, [("app_close", "settings"), ("sleep", 7), ("closed-assert",)], 0)
m.STATE_SETTLE["memory-scene"] = 0

original_sequence = m.Arm.sequence

def sequence(self, name, trigger, seconds):
    errors = []
    def checked_trigger():
        try:
            trigger()
        except Exception as error:
            errors.append(error)
    clocks_file = self.out / "sequence-clocks.json"
    clocks = json.loads(clocks_file.read_text()) if clocks_file.exists() else {}
    clocks[name] = {"before_sequence_ns": time.monotonic_ns()}
    clocks_file.write_text(json.dumps(clocks))
    result = original_sequence(self, name, checked_trigger, seconds)
    if errors:
        self.grab(self.out / "shots" / (name + "-failed.png"))
        raise errors[0]
    return result

m.Arm.sequence = sequence
original_spawn = m.Arm.spawn

def spawn(self, command, name, env=None, cwd=None):
    if name == "shell":
        self.env.pop("BEAD_PRIVATE_INPUT_PROOF", None)
        self.env["BEAD_PROFILE_INPUT"] = "1"
    return original_spawn(self, command, name, env, cwd)

m.Arm.spawn = spawn
original_start = m.Arm.start_shell

def start(self):
    # Generated fixtures only: preserve the initial png and media count while
    # exercising the picker with common image and video containers/resolutions.
    from PIL import Image
    folder = Path(self.env["HOME"]) / "Pictures/Wallpapers"
    for index, extension, size in [(2, "jpg", (3840, 2160)), (3, "webp", (1280, 720))]:
        source = folder / f"generated-{index}.png"
        if source.exists():
            with Image.open(source) as image:
                image.resize(size).save(source.with_suffix("." + extension))
            source.unlink()
    for index, extension, codec, size in [(2, "webm", "libvpx-vp9", "1280:720"),
                                          (3, "mkv", "libx264", "1920:1080"),
                                          (4, "mov", "libx264", "640:360"),
                                          (5, "m4v", "libx264", "854:480")]:
        source = folder / "Dynamic" / f"motion-{index}.mp4"
        if source.exists():
            colour = {2: "ff00ff", 3: "00ffff", 4: "ffff00", 5: "0000ff"}[index]
            filters = "scale=" + size + ",drawbox=x=iw/10:y=ih/5:w=iw/10:h=ih/10:color=0x" + colour + ":t=fill"
            self.run(["ffmpeg", "-v", "error", "-i", str(source), "-vf", filters,
                      "-c:v", codec, "-threads", "1", "-y", str(source.with_suffix("." + extension))], timeout=60)
            source.unlink()
    original_start(self)
    self.selection_keys = []
    # Receive clocks include pipe/read scheduling. They are independent of UI
    # profile marks and screencopy spawn clocks; none are physical scanout.
    self.wallpaper_watch = subprocess.Popen(
        [self.nacrectl, "watch", "wallpaper.active"], env=self.env,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True)
    self.procs.append(self.wallpaper_watch)
    def read():
        with (self.out / "wallpaper-observed.jsonl").open("w") as out:
            for line in self.wallpaper_watch.stdout:
                try:
                    value = json.loads(line)
                except ValueError:
                    continue
                out.write(json.dumps({"receive_ns": time.monotonic_ns(), "message": value}) + "\n")
                out.flush()
    self.wallpaper_reader = threading.Thread(target=read)
    self.wallpaper_reader.start()

m.Arm.start_shell = start
original_steps = m.Arm.steps

def steps(self, operations):
    for step in operations:
        if step[0] == "preference-assert":
            data = self.call("settings.get", {"store": "wallpaper-picker"})["data"]
            if data.get(step[1]) != step[2]:
                raise RuntimeError("toolbar did not persist " + step[1] + "=" + step[2])
        elif step[0] == "active-assert":
            entries = self.call("wallpaper.catalogue", {"ids": []}, timeout=30)["entries"]
            active = [Path(entry["id"]).name for entry in entries if entry["active"]]
            if active != [step[1]]:
                raise RuntimeError("latest selection did not activate " + step[1])
        elif step[0] == "scene-window-assert":
            windows = json.loads(self.run(["niri", "msg", "--json", "windows"]).stdout)
            if not any("Settings" in window.get("title", "") and window.get("is_focused") for window in windows):
                raise RuntimeError("Current scene is not a focused real Settings window")
        elif step[0] == "picker-open-assert":
            state = self.dispatch("nacre", "state")
            state = json.loads(state) if isinstance(state, str) else state
            if not state.get("launcher") or state.get("launcherMode") != "wallpaper":
                raise RuntimeError("Current scene button did not open the wallpaper picker")
        elif step[0] == "closed-assert":
            state = self.dispatch("nacre", "state")
            state = json.loads(state) if isinstance(state, str) else state
            if state.get("launcher"):
                raise RuntimeError("picker did not close before memory sampling")
        elif step[0] == "video-assert":
            from PIL import Image, ImageChops
            entries = self.call("wallpaper.catalogue", {"ids": []}, timeout=30)["entries"]
            active = [Path(entry["id"]).name for entry in entries if entry["active"]]
            if active != [step[1]]:
                raise RuntimeError("video selection did not activate " + step[1])
            if self.name == "bead":
                log = (self.logs / "shell.log").read_text(errors="replace")
                if "WALLPAPER_VIDEO_ERROR" in log or "EGL presentation" not in log:
                    raise RuntimeError("video renderer failed or did not use GPU presentation")
            before = self.out / "shots" / (step[1] + "-motion-a.png")
            after = self.out / "shots" / (step[1] + "-motion-b.png")
            self.grab(before)
            time.sleep(.35)
            self.grab(after)
            with Image.open(before) as a, Image.open(after) as b:
                # Each replacement has a unique solid marker, proving the new
                # source is shown rather than merely an old clip still moving.
                index = int(step[1].split("-", 1)[1].split(".", 1)[0])
                markers = {2: ((255, 0, 255), (1280, 720)), 3: ((0, 255, 255), (1920, 1080)),
                           4: ((255, 255, 0), (640, 360)), 5: ((0, 0, 255), (854, 480))}
                if index in markers:
                    expected, (sw, sh) = markers[index]
                    width, height = a.size
                    scale = max(width/sw, height/sh)
                    x = sw*.1*scale - (sw*scale-width)/2
                    y = sh*.2*scale - (sh*scale-height)/2
                    region = tuple(round(v) for v in (x+sw*.03*scale, y+sh*.03*scale,
                                                       x+sw*.07*scale, y+sh*.07*scale))
                    actual = a.convert("RGB").crop(region).resize((1, 1)).getpixel((0, 0))
                    # Display colour conversion can add a light wash. Distinct
                    # source markers differ by 255 in at least one channel.
                    if max(abs(c-e) for c, e in zip(actual, expected)) > 64:
                        raise RuntimeError("renderer did not show the selected source marker: " + str(actual))
                # Centre of the background, clear of the bottom picker and bar.
                region = (100, 200, 1500, 900)
                if not ImageChops.difference(a.convert("RGB").crop(region), b.convert("RGB").crop(region)).getbbox():
                    raise RuntimeError("selected wallpaper frames did not advance")
        elif step[0] == "selection-key":
            self.selection_keys.append({"key": step[1], "send_ns": time.monotonic_ns(), "state_start_ns": round(self.state_start*1e9)})
            self.run(["wtype", "-k", step[1]])
            (self.out / "selection-clocks.json").write_text(json.dumps(self.selection_keys))
        else:
            original_steps(self, [step])

m.Arm.steps = steps
original_shutdown = m.Arm.shutdown

def shutdown(self):
    original_shutdown(self)
    if hasattr(self, "wallpaper_reader"):
        self.wallpaper_reader.join(timeout=5)
        if self.wallpaper_reader.is_alive():
            raise RuntimeError("wallpaper watcher did not retire")

m.Arm.shutdown = shutdown
if __name__ == "__main__":
    sys.exit(m.main())
