#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0189: idle cost of desktop-layer widgets in a private nested session.

Wraps parity-compare.py (Bead arm only) with the fixed layout
spikes/desktop-widgets-layout.json (digital clock, analog clock, battery ring).
State `widgets-idle` samples the shell process for WIDGET_IDLE_SECONDS (default 60):
CPU per mille of one core, context switches per second (wakeup proxy), canvas
redraws and damaged pixels from the CANVAS_DAMAGE trace, and the surface size.
Pass --cpu --size 2880x1800 --scale 1.5 for the laptop mode."""
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import time

QUIET = os.environ.get('WIDGET_IDLE_QUIET') == '1'  # no trace output: CPU only
if not QUIET:
    os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
else:
    os.environ.pop('BEAD_VERBOSE_LOG', None)
SPIKES = Path(__file__).resolve().parent
sys.path.insert(0, str(SPIKES))
spec = importlib.util.spec_from_file_location('parity', SPIKES / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__
if QUIET:
    os.environ.pop('BEAD_VERBOSE_LOG', None)

SECONDS = float(os.environ.get('WIDGET_IDLE_SECONDS', '60'))
# B0193: widgets-covered measures the same idle while a fullscreen window covers the output,
# then uncovers it (the screenshot shows the resumed widgets).
STATES = {'widgets-idle': ([('idle-measure', SECONDS)], [], 0), 'widgets-shot': ([('sleep', 3.0)], [], 0),
          'widgets-covered': ([('cover', 'on'), ('sleep', 3.0), ('idle-measure', SECONDS), ('cover', 'off'), ('sleep', 3.0)], [], 0)}
m.STATES.update(STATES)
m.STATE_SETTLE.update({name: .5 for name in STATES})


def _ticks(pid):
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return int(fields[11]) + int(fields[12])


def _switches(pid):
    total = 0
    for task in Path(f'/proc/{pid}/task').iterdir():
        for line in (task / 'status').read_text().splitlines():
            if 'ctxt_switches' in line:
                total += int(line.split()[1])
    return total


def idle_steps(self, steps, _steps=m.Arm.steps):
    for step in steps:
        if step[0] == 'cover':
            if step[1] == 'on':
                self.cover = self.spawn(['alacritty', '--class', 'widget-cover', '-e', 'sleep', '900'], 'cover')
                time.sleep(3.0)
                self.run(['niri', 'msg', 'action', 'fullscreen-window'], check=False)
            else:
                self.run(['niri', 'msg', 'action', 'fullscreen-window'], check=False)
                time.sleep(1.0)
                self.cover.terminate()
            continue
        if step[0] != 'idle-measure':
            _steps(self, [step])
            continue
        pid = self.shell_pid
        log = self.logs / 'shell.log'
        start_size = log.stat().st_size if log.exists() else 0
        t0, c0, s0 = time.monotonic(), _ticks(pid), _switches(pid)
        time.sleep(step[1])
        t1, c1, s1 = time.monotonic(), _ticks(pid), _switches(pid)
        elapsed = t1 - t0
        with log.open('rb') as handle:
            handle.seek(start_size)
            text = handle.read().decode('utf-8', 'replace')
        damage = [tuple(map(int, row)) for row in re.findall(r'CANVAS_DAMAGE rects=(\d+) pixels=(\d+) surface=(\d+)', text)]
        result = {
            'seconds': round(elapsed, 2),
            'cpu_permille_of_core': round((c1 - c0) / os.sysconf('SC_CLK_TCK') / elapsed * 1000, 2),
            'context_switches_per_s': round((s1 - s0) / elapsed, 2),
            'quiet': QUIET,
            'canvas_redraws': len(damage),
            'canvas_redraws_per_s': round(len(damage) / elapsed, 3),
            'damage_pixels_per_redraw': round(sum(row[1] for row in damage) / len(damage), 1) if damage else 0,
            'surface_pixels': damage[-1][2] if damage else None,
        }
        (self.out / 'idle.json').write_text(json.dumps(result, indent=1))
        print('IDLE_RESULT', json.dumps(result), flush=True)


_spawn = m.Arm.spawn


def spawn(self, command, name, env=None, cwd=None):
    if name == 'shell' and not QUIET:
        command = [*command, '--trace-state']
    process = _spawn(self, command, name, env, cwd)
    if name == 'shell':
        self.shell_pid = process.pid
    return process


_start_daemon = m.Arm.start_daemon


def start_daemon(self):
    layout = self.root / 'state' / 'nacre' / 'widget-capture-layout.json'
    layout.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    self.env['BEAD_WIDGET_LAYOUT'] = str(layout)
    document = json.loads((SPIKES / 'desktop-widgets-layout.json').read_text())
    for item in document['hosts']['canvas']['instances']:
        if item['iid'] == 'desk-analog':
            item['style'] = os.environ.get('WIDGET_ANALOG_STYLE', 'analog')
    if os.environ.get('WIDGET_NO_CANVAS') == '1':  # the shell's own floor, no desktop widgets
        document['hosts']['canvas']['instances'] = []
    layout.write_text(json.dumps(document))
    layout.chmod(0o600)
    return _start_daemon(self)


def gallery(args, names):
    return {}


# A stub swaybg (bin dir) keeps the poster step quiet; WIDGET_BG paints the nested
# compositor background instead (bright values test text contrast on light wallpapers).
# WIDGET_WALLPAPER=bright|dark: the fixture wallpaper (parity-compare's generated-1.png) is a flat
# light or dark image, so the real wallpaper pipeline (daemon, decode, luminance) is exercised.
FLAT = {'bright': (232, 238, 242), 'dark': (16, 20, 24)}
if os.environ.get('WIDGET_WALLPAPER') in FLAT:
    from PIL import Image
    _save = Image.Image.save

    def flat_save(self, fp, *args, **kwargs):
        if str(fp).endswith('generated-1.png'):
            self = Image.new('RGB', self.size, FLAT[os.environ['WIDGET_WALLPAPER']])
        return _save(self, fp, *args, **kwargs)
    Image.Image.save = flat_save
m.NIRI_CONFIG += 'layout { background-color "%s"; }\n' % os.environ.get('WIDGET_BG', '#101418')
m.Arm.steps = idle_steps
m.Arm.spawn = spawn
m.Arm.start_daemon = start_daemon
m.build_gallery = gallery

if __name__ == '__main__':
    argv = sys.argv[1:]
    if '--arms' not in argv:
        argv += ['--arms', 'bead']
    if '--states' not in argv:
        argv += ['--states', 'widgets-idle,widgets-shot']
    sys.argv = [sys.argv[0], *argv]
    sys.exit(m.main())
