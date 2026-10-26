#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0142: five complete switch cycles using the guarded QML/Bead picker driver.

Each cycle covers static→static, static→video, video→video and video→static.
Selection clocks, presentation markers, frame sequences and process memory stay
in the existing driver's evidence. All sources are generated private fixtures.
"""
import importlib.util
import os
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

if __name__ == "__main__":
    sys.exit(m.main())
