#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Idle minute publication/presentation and process-stop recovery in guarded private sessions.

Use parity-compare.py's arguments, under the desktop capture lock. CLOCK_PROOF_MINUTES
defaults to six; CLOCK_PROOF_RESUME=1 also stops only this proof's shell across a minute.
"""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import time
from PIL import Image

spec = importlib.util.spec_from_file_location('parity', Path(__file__).with_name('parity-compare.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__
original_spawn = m.Arm.spawn


def publication(text):
    ticks = re.findall(r'CLOCK_TICK value=(\S+)', text)
    primary = re.search(r'get_layer_surface\([^\n]*wl_surface[@#](\d+),[^\n]*"bead-dev"', text)
    commits = len(re.findall(r'wl_surface[@#]' + primary[1] + r'\.commit\(', text)) if primary else 0
    return ticks, commits


def capture(self, path):
    self.grab(path)
    # Fixed proof geometry: the clock and calendar, excluding the power button.
    # Keep the full-window PNG; hash only this crop to isolate minute repaint.
    with Image.open(path) as image:
        crop = image.crop((image.width - 210, 0, image.width - 60, round(50 * self.args.scale)))
        return hashlib.sha256(crop.tobytes()).hexdigest()


def spawn(self, command, *args, **kwargs):
    if '--bar' in command and Path(command[0]).name == 'nacre-shell':
        command = [*command, '--trace-state']
        kwargs['env'] = dict(kwargs.get('env') or self.env, WAYLAND_DEBUG='client')
    return original_spawn(self, command, *args, **kwargs)


def drive(self, _names):
    self.errors = []
    shots = self.out / 'shots'
    shots.mkdir(exist_ok=True)
    rows = []
    start = time.monotonic()
    previous_ticks = None
    previous_commits = None
    previous_pixels = None
    for minute in range(int(os.environ.get('CLOCK_PROOF_MINUTES', '6')) + 1):
        time.sleep(max(0, start + minute * 60 - time.monotonic()))
        pixels = capture(self, shots / f'idle-{minute}.png')
        text = (self.logs / 'shell.log').read_text(errors='replace')
        ticks, commits = publication(text)
        rows.append(dict(elapsed=time.monotonic() - start, wall=time.strftime('%H:%M'),
                         ticks=ticks, bar_commits=commits, clock_pixels_sha256=pixels))
        (self.out / 'clock.json').write_text(json.dumps(rows, indent=2) + '\n')
        print('CLOCK_PROOF ' + json.dumps(rows[-1]), flush=True)
        assert self.shell.poll() is None, 'shell exited'
        if self.name == 'bead':
            assert ticks[-1] == rows[-1]['wall'], 'published clock is stale'
        if previous_ticks is not None:
            assert len(ticks) > previous_ticks, 'service failed to publish a minute'
            assert commits > previous_commits, 'bar failed to commit the changed minute'
            assert pixels != previous_pixels, 'clock pixels did not change'
        previous_ticks = len(ticks)
        previous_commits = commits
        previous_pixels = pixels
    if os.environ.get('CLOCK_PROOF_PANELS') == '1':
        def panel_ready():
            state = self.socket_call(self.runtime / 'bead-command.sock', 'bead.panel', 'state', [])['result']
            return state['target_open'] and state['prepared'] and state['surfaces'] > 0

        self.dispatch('nacre', 'popout', 'calendar', '1560')
        m.wait_until(panel_ready, 15, 'calendar first frame')
        time.sleep(1)
        self.grab(shots / 'calendar-open.png')
        self.dispatch('nacre', 'close')
        m.wait_until(lambda: 'PANEL_GPU_SUSPENDED' in (self.logs / 'shell.log').read_text(errors='replace'),
                     30, 'closed panel GPU suspend')
        # Leave the bar untouched across another boundary after panel teardown.
        time.sleep(62)
        pixels = capture(self, shots / 'gpu-suspended.png')
        text = (self.logs / 'shell.log').read_text(errors='replace')
        ticks, commits = publication(text)
        assert len(ticks) > previous_ticks, 'clock stopped after closing the panel'
        assert commits > previous_commits, 'bar stopped committing after panel GPU suspend'
        assert pixels != previous_pixels, 'clock pixels stopped after panel GPU suspend'
        rows.append(dict(gpu_suspended=True, wall=time.strftime('%H:%M'), ticks=ticks,
                         bar_commits=commits, clock_pixels_sha256=pixels))
        self.dispatch('nacre', 'popout', 'calendar', '1560')
        m.wait_until(panel_ready, 15, 'calendar first frame after GPU suspend')
        time.sleep(1)
        self.grab(shots / 'calendar-reopened.png')
        self.dispatch('nacre', 'close')
    if os.environ.get('CLOCK_PROOF_RESUME') == '1':
        self.shell.send_signal(signal.SIGSTOP)
        try:
            time.sleep(65)
        finally:
            self.shell.send_signal(signal.SIGCONT)
        self.run(['gdbus', 'emit', '--session', '--object-path', '/org/freedesktop/login1',
                  '--signal', 'org.freedesktop.login1.Manager.PrepareForSleep', 'false'])
        time.sleep(2)
        pixels = capture(self, shots / 'resumed.png')
        text = (self.logs / 'shell.log').read_text(errors='replace')
        ticks, commits = publication(text)
        rows.append(dict(resume=True, wall=time.strftime('%H:%M'),
                         ticks=ticks, bar_commits=commits, clock_pixels_sha256=pixels))
        assert rows[-1]['ticks'][-1] == rows[-1]['wall'], 'clock did not recover after process stop/resume'
        assert commits > rows[-2]['bar_commits'], 'bar did not commit after process stop/resume'
        assert pixels != rows[-2]['clock_pixels_sha256'], 'clock pixels did not recover after process stop/resume'
        (self.out / 'clock.json').write_text(json.dumps(rows, indent=2) + '\n')


m.Arm.spawn = spawn
m.Arm.drive = drive
if __name__ == '__main__':
    raise SystemExit(m.main())
