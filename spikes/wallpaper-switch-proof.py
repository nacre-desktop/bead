#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0142: five complete switch cycles using the guarded QML/Bead picker driver.

Each cycle covers static→static, static→video, video→video and video→static.
Selection clocks, presentation markers, frame sequences and process memory stay
in the existing driver's evidence. All sources are generated private fixtures.
"""
import importlib.util
import os
import re
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("picker", Path(__file__).with_name("picker-fixes-proof.py"))
picker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(picker)
m = picker.m
m.__file__ = __file__
states = {}
for cycle in range(5):
    for index, original in enumerate(("wallpaper-travel", "wallpaper-format-mp4",
                                       "wallpaper-format-webm", "wallpaper-travel")):
        name = f"wallpaper-switch-{cycle + 1}-{index + 1}"
        opens, closes, _ = m.STATES[original]
        # Timing and memory cohorts avoid concurrent screencopy load; image
        # cohorts retain transition frames to check for blank backgrounds.
        seconds = 0 if os.environ.get("BEAD_SWITCH_TIMING_ONLY") == "1" else .5
        states[name] = (opens, closes, seconds)
        m.STATE_PREPARE[name] = list(m.STATE_PREPARE[original])
        m.STATE_SETTLE[name] = 0

input_mode = os.environ.get("BEAD_SWITCH_INPUT", "keyboard")
if input_mode not in ("keyboard", "mouse"):
    raise ValueError("BEAD_SWITCH_INPUT must be keyboard or mouse")
if input_mode == "mouse":
    for cycle in range(5):
        for index in range(4):
            name = f"wallpaper-switch-{cycle + 1}-{index + 1}"
            opens, closes, seconds = states[name]
            gesture = "arrow" if index in (0, 3) else "card"
            converted = []
            for step in opens:
                if step == ("selection-key", "Right"):
                    converted.append(("mouse-selection", gesture))
                elif gesture == "card" and step == ("key", "Return"):
                    continue
                else:
                    converted.append(step)
            states[name] = (converted, closes, seconds)
m.STATES = states

# Include the shell's native UI worker children; video remains a separate entry
# from the picker driver's existing sampler. These states open no Settings/chat app.
original_app_rss = m.Arm.app_rss

def app_rss(self):
    result = original_app_rss(self)
    if self.name != "bead":
        return result
    pending = [self.shell.pid]
    seen = set(pending)
    while pending:
        parent = pending.pop()
        for task in Path(f"/proc/{parent}/task").glob("*"):
            try:
                children = map(int, (task / "children").read_text().split())
                for pid in children:
                    if pid in seen:
                        continue
                    seen.add(pid)
                    pending.append(pid)
                    if Path(f"/proc/{pid}/exe").resolve().name == "nacre-shell":
                        result[f"ui-worker-{pid}"] = self.rss(pid)
            except OSError:
                pass
    return result

m.Arm.app_rss = app_rss

def mouse_selection(self, target):
    if not self.args.pointer_tool or not Path(self.args.pointer_tool).is_file():
        raise RuntimeError("mouse switch proof requires --pointer-tool")
    x, y = (1204, 1157) if target == "arrow" else (960, 1054)
    width, height = self.args.size
    scale = self.geometry["scale"]
    command = [self.args.pointer_tool, str(round(x * scale)), str(round(y * scale)),
               str(width), str(height), "click-clock"]
    result = self.run(command, timeout=10, check=False)
    if result.returncode:
        result = self.run(command, timeout=10, check=False,
                          env=dict(self.env, WAYLAND_DISPLAY=self.env["BEAD_PARENT"]))
    if result.returncode:
        raise RuntimeError("virtual-pointer click failed: " + result.stderr.strip())
    released = re.search(r"^POINTER_RELEASED monotonic_ns=(\d+)$", result.stdout, re.MULTILINE)
    if not released:
        raise RuntimeError("virtual-pointer click did not report POINTER_RELEASED")
    self.selection_keys.append({"input": "pointer-release", "send_ns": int(released.group(1)),
                                "state_start_ns": round(self.state_start * 1e9)})
    (self.out / "selection-clocks.json").write_text(json.dumps(self.selection_keys))

if input_mode == "mouse":
    original_steps = m.Arm.steps

    def steps(self, operations):
        for step in operations:
            if step[0] == "mouse-selection":
                mouse_selection(self, step[1])
            else:
                original_steps(self, [step])

    m.Arm.steps = steps

if __name__ == "__main__":
    sys.exit(m.main())
