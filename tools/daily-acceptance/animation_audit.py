#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0168 animation audit: per-surface frame pacing, motion, CPU and idle memory (stable vs main vs QML).

Wraps spikes/animation-pacing.py (same private labwc > niri > shell topology, WAYLAND_DEBUG presentation
feedback) and adds, per surface open and close:
  * CPU time of the shell process (per thread name) during the window, and idle CPU / Pss_Anon;
  * a motion pass: back-to-back grim frames; progress of each frame toward the final image shows whether the
    surface slides (several intermediate frames) or just appears (none), and its visible duration.
Run on the desktop under the lock, same flags as animation-pacing.py. AUDIT_SURFACES=a,b picks surfaces,
AUDIT_REPEATS=N (default 3), AUDIT_MOTION=0 skips the motion pass.
"""
import importlib.util
import json
import os
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
spikes = HERE.parents[1] / 'spikes'
sys.path.insert(0, str(spikes))
os.environ.setdefault('PACING_REPEATS', os.environ.get('AUDIT_REPEATS', '3'))
spec = importlib.util.spec_from_file_location('apace', spikes / 'animation-pacing.py')
ap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ap)
m = ap.m
CLK = os.sysconf('SC_CLK_TCK')

ap.ANIMS.update({
    'popout-bluetooth': ([], [('ui', 'nacre', 'popout', 'bluetooth', '1600')], [('ui', 'nacre', 'close')], 1.2),
    'popout-battery': ([], [('ui', 'nacre', 'popout', 'battery', '1640')], [('ui', 'nacre', 'close')], 1.2),
    'popout-calendar': ([], [('ui', 'nacre', 'popout', 'calendar', '1700')], [('ui', 'nacre', 'close')], 1.2),
    'menu-power': ([], [('ui', 'nacre', 'menu', 'power', '')], [('ui', 'nacre', 'close')], 1.2),
    'popout-updates': ([], [('ui', 'nacre', 'controls', 'updates')], [('ui', 'nacre', 'close')], 1.2),
})
SURFACES = os.environ.get('AUDIT_SURFACES') or ('dashboard,control-centre,sidebar,launcher,popout-network,popout-audio,'
    'popout-bluetooth,popout-battery,popout-calendar,menu-power,notification,wallpaper-carousel,osd')
for _n in ap.ANIMS:
    m.STATES.setdefault('pace-' + _n, ([], [], 0))


def thread_ticks(pid):
    out = {}
    for stat in Path(f'/proc/{pid}/task').glob('*/stat'):
        try:
            line = stat.read_text()
            name = line[line.index('(') + 1:line.rindex(')')]
            f = line[line.rindex(')') + 2:].split()
            out[name] = out.get(name, 0) + int(f[11]) + int(f[12])
        except (OSError, ValueError, IndexError):
            pass
    return out


class Sampler(threading.Thread):
    def __init__(self, pid):
        super().__init__(daemon=True)
        self.pid, self.rows, self.stop_event = pid, [], threading.Event()

    def run(self):
        while not self.stop_event.wait(0.05):
            self.rows.append((time.monotonic(), thread_ticks(self.pid)))

    def between(self, a, b):
        lo = min(self.rows, key=lambda r: abs(r[0] - a))
        hi = min(self.rows, key=lambda r: abs(r[0] - b))
        delta = {k: (hi[1].get(k, 0) - lo[1].get(k, 0)) * 1000 / CLK for k in hi[1]}
        delta = {k: v for k, v in delta.items() if v > 0}
        return round(sum(delta.values())), dict(sorted(delta.items(), key=lambda kv: -kv[1])[:3])


def progress(seq_dir, stamps):
    """Per-frame progress toward the final frame (0 first image, 1 final) via PIL mean absolute difference."""
    from PIL import Image, ImageChops, ImageStat
    files = sorted(seq_dir.glob('*.jpg'))
    if len(files) < 3:
        return None
    images = [Image.open(f).convert('L') for f in files]
    final = images[-1]
    dist = [sum(ImageStat.Stat(ImageChops.difference(im, final)).mean) for im in images]
    d0 = max(dist[:3]) or 1e-9
    prog = [round(1 - d / d0, 3) for d in dist]
    mid = [i for i, p in enumerate(prog) if 0.04 < p < 0.96]
    moving = [i for i, p in enumerate(prog) if p > 0.04]
    first = moving[0] if moving else None
    done = next((i for i, p in enumerate(prog) if p >= 0.96 and first is not None and i >= first), None)
    return {'frames': len(files), 'mid_frames': len(mid), 'changed': d0 > 0.3,
            'visible_ms': (stamps[done] - stamps[first]) if first is not None and done is not None else None,
            'slides': len(mid) >= 2, 'progress': prog[:40]}


def drive(self, names):
    pid = self.shell.pid
    sampler = Sampler(pid)
    sampler.start()
    t0 = time.monotonic()
    time.sleep(float(os.environ.get('AUDIT_IDLE_S', '15')))
    idle_ms, idle_top = sampler.between(t0, time.monotonic())
    idle_mem = self.rss()
    os.environ['PACING_ANIMS'] = SURFACES
    ap.pace_drive(self, names)
    sampler.stop_event.set()
    path = self.out / 'pacing.json'
    data = json.loads(path.read_text())
    data['idle_cpu_pct'] = round(idle_ms / 10 / float(os.environ.get('AUDIT_IDLE_S', '15')), 2)
    data['idle_cpu_top_threads_ms'] = idle_top
    data['idle_mem_kib'] = idle_mem
    for w in data['windows']:
        # window times are monotonic ns; the sampler uses time.monotonic() seconds
        w['cpu_ms'], w['cpu_top'] = sampler.between(w['mono_start_ns'] / 1e9, w['mono_end_ns'] / 1e9)
    if os.environ.get('AUDIT_MOTION', '1') != '0':
        motion = {}
        for name in SURFACES.split(','):
            prepare, opens, closes, seconds = ap.ANIMS[name]
            for rep in range(int(os.environ.get('PACING_REPEATS', '3'))):
                try:
                    self.steps([('ui', 'nacre', 'close')])
                    time.sleep(0.8)
                    self.steps(prepare)
                    for phase, st in (('open', opens), ('close', closes)):
                        sname = f'audit-{name}-{phase}{rep}'
                        stamps = self.sequence(sname, lambda s=st: self.steps(s), seconds)
                        motion.setdefault(f'{name}/{phase}', []).append(progress(self.out / 'seq' / sname, stamps))
                        time.sleep(0.5)
                except Exception as error:  # keep the audit going; the error is part of the evidence
                    motion.setdefault(f'{name}/error', []).append(str(error)[:200])
        data['motion'] = motion
    path.write_text(json.dumps(data, indent=1))


m.__file__ = __file__  # guarded re-execs come back through this wrapper
m.Arm.drive = drive
if __name__ == '__main__':
    if '--states' not in sys.argv:
        sys.argv += ['--states', 'pace']
    if '--seq-interval' not in sys.argv:
        sys.argv += ['--seq-interval', '25', '--seq-scale', '0.25']
    sys.exit(m.main())
