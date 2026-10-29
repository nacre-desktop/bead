#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0123: use the guarded private parity driver for cold, warm and expiry opens."""
import importlib.util
import sys
import os
import json
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("picker", Path(__file__).with_name("picker-fixes-proof.py"))
picker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(picker)
m = picker.m
m.__file__ = __file__

open_picker = ("ui", "nacre", "launcher", ">wallpaper ")
close_picker = ("ui", "nacre", "close")
pref = ("wallpaper-pref", "static", "spotlight")
for state, preparation in [
    ("spotlight-cold", []),
    ("spotlight-repeat", [open_picker, ("sleep", 4), close_picker, ("sleep", 1)]),
    ("spotlight-settled", [open_picker, ("sleep", 4), close_picker, ("sleep", 3), ("sample", "settled-closed")]),
    ("spotlight-expired", [open_picker, ("sleep", 4), close_picker, ("sleep", 125), ("sample", "expired-closed")]),
]:
    m.STATE_PREPARE[state] = [pref, ("bar-window",), *preparation]
    m.STATES[state] = ([open_picker], [close_picker, ("bar-window-close",)], 2)
    m.STATE_SETTLE[state] = 3
m.STATE_SETTLE["spotlight-desktop"] = 0
m.STATE_PREPARE["spotlight-desktop"] = [("bar-window",)]
m.STATES["spotlight-desktop"] = ([], [("bar-window-close",)], 0)
for state, kind in [("spotlight-hover", "pointer"), ("spotlight-pressed", "press")]:
    m.STATE_PREPARE[state] = [pref, ("bar-window",), open_picker, ("sleep", 4)]
    m.STATES[state] = ([(kind, 960, 600)], [("release",), close_picker, ("bar-window-close",)], 0)
    m.STATE_SETTLE[state] = .1
palette_layout = "spotlight" if os.environ.get("PICKER_PALETTE_LAYOUT") == "spotlight" else "carousel"
m.STATE_PREPARE["palette-sync"] = [("wallpaper-pref", "static", palette_layout), open_picker, ("sleep", 8)]
selection = ("selection-click", 1225, 1060) if os.environ.get("PICKER_SELECTION_CLICK") == "1" else ("selection-key", "Right")
m.STATES["palette-sync"] = ([selection], [close_picker], 3)
m.STATE_SETTLE["palette-sync"] = 3
m.STATE_CLEANUP["palette-sync"] = [("active-assert", "generated-2.jpg")]
original_spawn = m.Arm.spawn

def spawn(self, command, name, env=None, cwd=None):
    if name == "shell":
        self.env["BEAD_FRAME_TRACE"] = "1"
        self.env["WAYLAND_DEBUG"] = "client"
        if self.name == "bead" and os.environ.get("PICKER_BAD_EGL") == "1":
            self.env["__EGL_VENDOR_LIBRARY_FILENAMES"] = "/nonexistent/b0123-egl-vendor.json"
        if self.name == "bead" and os.environ.get("PICKER_GL") in ("1", "panel"):
            renderer = "auto" if os.environ["PICKER_GL"] == "panel" else "femtovg-gl"
            command = [*command, "--renderer", renderer, "--panel-renderer", "femtovg-gl"]
    return original_spawn(self, command, name, env, cwd)

m.Arm.spawn = spawn
original_run = m.Arm.run

def run(self, command, timeout=15, check=True, env=None):
    hz = os.environ.get("PACING_HZ", "60")
    command = [part.replace("@60Hz", "@" + hz + "Hz") if isinstance(part, str) else part for part in command]
    return original_run(self, command, timeout, check, env)

m.Arm.run = run
original_sequence = m.Arm.sequence

def sequence(self, name, trigger, seconds):
    if name == "palette-sync-open":
        self.grab(self.out / "shots" / "palette-sync-before.png")
    panel_before = self.socket_call(self.runtime / "bead-command.sock", "bead.panel", "state", []).get("result") if self.name == "bead" else None
    real, mono = time.time(), time.monotonic()
    if os.environ.get("PICKER_PACING_ONLY") == "1":
        trigger()
        time.sleep(max(0, seconds-(time.monotonic()-mono)))
        result = []
    else:
        result = original_sequence(self, name, trigger, seconds)
    path = self.out / "sequence-clocks.json"
    clocks = json.loads(path.read_text()) if path.exists() else {}
    clocks.setdefault(name, {})
    clocks[name].update(real_start=real, mono_start=mono, seconds=seconds, panel_before=panel_before)
    path.write_text(json.dumps(clocks))
    return result

m.Arm.sequence = sequence
original_steps = m.Arm.steps

def steps(self, operations):
    for step in operations:
        if step[0] == "sample":
            path = self.out / "memory-phases.json"
            phases = json.loads(path.read_text()) if path.exists() else {}
            phases[step[1]] = self.rss()
            path.write_text(json.dumps(phases))
        elif step[0] == "selection-click":
            original_steps(self, [("pointer", step[1], step[2])])
            press_send_ns = time.monotonic_ns()
            original_steps(self, [("press", step[1], step[2])])
            self.selection_keys.append({"key": "Pointer", "press_send_ns": press_send_ns, "send_ns": time.monotonic_ns(), "state_start_ns": round(self.state_start*1e9)})
            original_steps(self, [("release",)])
            (self.out / "selection-clocks.json").write_text(json.dumps(self.selection_keys))
        else:
            original_steps(self, [step])

m.Arm.steps = steps
original_drive = m.Arm.drive

def drive(self, names):
    (self.out / "idle-memory.json").write_text(json.dumps(self.rss()))
    return original_drive(self, names)

m.Arm.drive = drive
if __name__ == "__main__":
    sys.exit(m.main())
