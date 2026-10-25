#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0110 animation frame-pacing harness: QML shell vs Bead in private nested sessions.

Wraps parity-compare.py (same private headless labwc > niri > shell topology, fixtures, fake daemon)
and replaces its screenshot driver with a pacing driver. Every shell runs with WAYLAND_DEBUG=client,
so the client-side protocol trace gives, for QML and Bead alike, each wl_surface.commit time, each
frame callback and (Bead only) wp_presentation `presented` stamp. Bead additionally logs one
`UI_FRAME ... render_ms=` line per rendered frame (BEAD_FRAME_TRACE=1): its per-frame CPU/GL time.

Per animation window: commit-interval p50/p95/p99, dropped frames (interval > 1.5 vsync, counted as
missed vsyncs), longest gap, first-frame latency (trigger -> first commit; includes the IPC hop) and
Bead render_ms p50/p99. Gaps longer than 250 ms are idle, not drops.

  PACING_HZ=60|120   private parent output refresh (the nested niri paces its clients from it)
  PACING_ANIMS=a,b   animations (default: all); PACING_REPEATS=N (default 3)
  animation-pacing.py --nacre N --bead-bin B --bead-shell S --out O --cpu --pointer-tool P ...
(the flags are parity-compare.py's; --states is ignored). Never touches the live session.
"""
import importlib.util
import json
import math
import os
import re
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
os.environ['BEAD_FRAME_TRACE'] = '1'
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__  # guarded re-execs must come back through this wrapper

HZ = int(os.environ.get('PACING_HZ', '60'))
VSYNC_MS = 1000.0 / HZ
REPEATS = int(os.environ.get('PACING_REPEATS', '3'))
IDLE_GAP_MS = 250.0

# name: (prepare, open, close, window seconds). Steps are parity-compare steps plus ('wheel', ...).
ANIMS = {
    'launcher': ([], [('ui', 'nacre', 'launcher', '')], [('ui', 'nacre', 'close')], 1.4),
    'dashboard': ([], [('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '0')],
                  [('ui', 'nacre', 'close')], 1.6),
    'dashboard-tab': ([('ui', 'drawers', 'toggle', 'dashboard'), ('sleep', 3), ('click', 610, 78), ('sleep', 1)],
                      [('click', 842, 78)], [('click', 610, 78)], 1.2),
    'control-centre': ([], [('ui', 'nacre', 'controls', 'home')], [('ui', 'nacre', 'close')], 1.4),
    'popout-network': ([], [('ui', 'nacre', 'popout', 'network', '1560')], [('ui', 'nacre', 'close')], 1.2),
    'popout-audio': ([('ui', 'nacre', 'popout', 'network', '1560'), ('sleep', 0.4),
                      ('ui', 'nacre', 'close'), ('sleep', 0.5)],
                     [('ui', 'nacre', 'popout', 'audio', '1520')], [('ui', 'nacre', 'close')], 1.2),
    'hover-peek': ([], [('pointer-fast', 960, 25)], [('pointer', 960, 600), ('ui', 'nacre', 'close')], 1.2),
    'sidebar': ([], [('ui', 'nacre', 'left')],
                [('only', 'qml', ('ui', 'nacre', 'close')), ('only', 'bead', ('ui', 'nacre', 'left'))], 1.4),
    'wallpaper-carousel': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '),
                            ('sleep', 2)],
                           [('key', 'Right'), ('key', 'Right'), ('key', 'Right'), ('key', 'Right')],
                           [('ui', 'nacre', 'close')], 2.2),
    'settings-wheel': ([('ui', 'nacre', 'settings'), ('sleep', 2.5)],
                       [('wheel', 'wheel', 1, 16, 0.07)], [('wheel', 'wheel', -1, 16, 0.07)], 1.6),
    'settings-touchpad': ([('ui', 'nacre', 'settings'), ('sleep', 2.5)],
                          [('wheel', 'pixel', 14, 60, 0.016)], [('wheel', 'pixel', -14, 60, 0.016)], 1.6),
    'osd': ([], m.STATES['osd-volume'][0], m.STATES['osd-volume'][1], 1.6),
    'notification': ([], [('notify', 'Pacing check', 'A notification toast.')], [('ui', 'nacre', 'close')], 2.0),
}
CLOSE_APPS = {'settings-wheel': 'settings', 'settings-touchpad': 'settings'}
for _name in ANIMS:
    m.STATES.setdefault('pace-' + _name, ([], [], 0))
m.STATES['pace'] = ([], [], 0)

_orig_spawn = m.Arm.spawn
_orig_run = m.Arm.run
_orig_steps = m.Arm.steps


def spawn(self, command, name, env=None, cwd=None):
    if name == 'shell':  # only the shell: niri/labwc must not trace as clients
        env = dict(env or self.env, WAYLAND_DEBUG='client', BEAD_FRAME_TRACE='1', BEAD_VERBOSE_LOG='1')
        if os.environ.get('PACING_PANEL_RENDERER') and self.name == 'bead':
            command = [*command, '--panel-renderer', os.environ['PACING_PANEL_RENDERER']]
    return _orig_spawn(self, command, name, env, cwd)


def run(self, command, timeout=15, check=True, env=None):
    command = [part.replace('@60Hz', f'@{HZ}Hz') if isinstance(part, str) else part for part in command]
    return _orig_run(self, command, timeout, check, env)


def steps(self, operations):
    for step in operations:
        if step[0] == 'wheel':
            _, kind, delta, count, interval = step
            scale = self.geometry['scale']
            command = [self.args.pointer_tool, str(round(1100 * scale)), str(round(600 * scale)),
                       str(self.args.size[0]), str(self.args.size[1]), 'stream', kind, str(delta),
                       str(count), str(max(1, round(interval * 1000)))]
            self.release_pointer()
            limit = count * interval + 10
            for display in (self.env['WAYLAND_DISPLAY'], self.env['BEAD_PARENT']):
                try:  # niri may lack wlr-virtual-pointer: the private parent labwc then forwards it
                    if subprocess.run(command, env=dict(self.env, WAYLAND_DISPLAY=display), timeout=limit,
                                      check=False, stdout=subprocess.DEVNULL).returncode == 0:
                        break
                except subprocess.TimeoutExpired:
                    self.errors.append(f'wheel injection timed out on {display}')
        else:
            _orig_steps(self, [step])


m.Arm.spawn, m.Arm.run, m.Arm.steps = spawn, run, steps


def percentile(values, p):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, math.ceil((len(ordered) - 1) * p))]


STAMP = re.compile(r'^\[(\d\d):(\d\d):(\d\d)\.(\d{6})\]\s+(?:\{[^}]*\}\s+)?(->\s+)?(\S+?)#(\d+)\.(\w+)\((.*)\)')


def parse_trace(path, real_ref):
    """Rows (seconds since real_ref, is_request, interface, id, event, args) for commit/done/presented.

    libwayland stamps are CLOCK_REALTIME as UTC HH:MM:SS.micros, so they map onto time.time()."""
    ref_sod = real_ref % 86400
    rows = []
    with open(path, errors='replace') as handle:
        for line in handle:
            if not line.startswith('['):
                continue
            hit = STAMP.match(line)
            if not hit:
                continue
            hh, mm, ss, micro, arrow, interface, ident, event, args = hit.groups()
            if event not in ('commit', 'done', 'presented', 'attach'):
                continue
            sod = int(hh) * 3600 + int(mm) * 60 + int(ss) + int(micro) / 1e6
            rows.append(((sod - ref_sod) % 86400, bool(arrow), interface, int(ident), event, args))
    return rows


def window_metrics(rows, start, end, frames):
    commits, attached = {}, set()
    for stamp, request, interface, ident, event, args in rows:
        if not (request and interface == 'wl_surface'):
            continue
        if event == 'attach':  # a commit carries a visible frame only if a buffer was attached since the last one
            if not args.startswith('nil'):
                attached.add(ident)
        elif event == 'commit':
            if ident in attached and start <= stamp <= end:
                commits.setdefault(ident, []).append(stamp)
            attached.discard(ident)
    if not commits:
        return {'frames': 0}
    surface, stamps = max(commits.items(), key=lambda item: len(item[1]))
    intervals = [(b - a) * 1000 for a, b in zip(stamps, stamps[1:]) if (b - a) * 1000 < IDLE_GAP_MS]
    dropped = sum(max(0, round(i / VSYNC_MS) - 1) for i in intervals if i > 1.5 * VSYNC_MS)
    active = (stamps[-1] - stamps[0]) * 1000
    result = {
        'surface': surface, 'commits': len(stamps), 'active_ms': round(active, 1),
        'first_frame_ms': round((stamps[0] - start) * 1000, 1),
        'interval_ms': {k: round(percentile(intervals, p), 2) for k, p in (('p50', .5), ('p95', .95), ('p99', .99))} if intervals else None,
        'longest_gap_ms': round(max(intervals), 1) if intervals else None,
        'dropped': dropped, 'slow_intervals': sum(i > 1.5 * VSYNC_MS for i in intervals),
        'drop_rate': round(dropped / max(1, dropped + len(intervals)), 3),
    }
    presented = []
    for stamp, request, interface, ident, event, args in rows:
        if start <= stamp <= end and not request and event == 'presented':
            values = [part.strip() for part in args.split(',')]
            if len(values) >= 3:
                presented.append(((int(values[0], 0) << 32 | int(values[1], 0)) + int(values[2], 0) / 1e9))
    deltas = [(b - a) * 1000 for a, b in zip(presented, presented[1:]) if (b - a) * 1000 < IDLE_GAP_MS]
    if deltas:
        result['presented_interval_ms'] = {k: round(percentile(deltas, p), 2) for k, p in (('p50', .5), ('p95', .95), ('p99', .99))}
    cpu = [f['render_ms'] for f in frames if start <= f['t'] <= end]
    if cpu:
        result['render_ms'] = {'p50': round(percentile(cpu, .5), 3), 'p99': round(percentile(cpu, .99), 3), 'max': round(max(cpu), 3)}
        result['render_over_vsync'] = round(sum(c > VSYNC_MS for c in cpu) / len(cpu), 3)
    return result


def pace_drive(self, names):
    selected = os.environ.get('PACING_ANIMS')
    names = selected.split(',') if selected else list(ANIMS)
    real_ref = time.time()
    mono_ref = time.monotonic()
    windows = []
    memory = {}
    self.errors = []
    try:  # shell process components (Pss, Pss_Anon, Pss_Shmem, DRM, SwapPss ...) in KiB
        memory['idle'] = self.rss()
    except Exception as error:
        self.errors.append(f'rss: {error}')
    self.dispatch_trace = []
    self.state_start = time.monotonic()
    for name in names:
        prepare, opens, closes, seconds = ANIMS[name]
        for repeat in range(REPEATS):
            try:
                self.steps([('ui', 'nacre', 'close')])
                time.sleep(0.8)
                self.steps(prepare)
                for phase, phase_steps in (('open', opens), ('close', closes)):
                    start = time.time()
                    mono = time.monotonic()
                    self.steps(phase_steps)
                    time.sleep(max(0, seconds - (time.monotonic() - mono)))
                    windows.append({'animation': name, 'phase': phase, 'repeat': repeat,
                                    'start': start - real_ref, 'end': time.time() - real_ref,
                                    'mono_start': mono - mono_ref})
                    time.sleep(0.5)
                if name in CLOSE_APPS:
                    self.steps([('app_close', CLOSE_APPS[name])])
                time.sleep(self.args.closed_settle)
                try:
                    memory[name] = self.rss()
                except Exception as error:
                    self.errors.append(f'rss {name}: {error}')
            except Exception as error:
                self.errors.append(f'{name}: {str(error)[:200]}')
    time.sleep(0.5)
    log_path = self.logs / 'shell.log'
    rows = parse_trace(log_path, real_ref)
    frames = []
    # UI_FRAME carries monotonic_ns; map it onto the real-clock reference by the harness offset.
    offset = real_ref - mono_ref
    for line in log_path.read_text(errors='replace').splitlines():
        if line.startswith('UI_FRAME '):
            fields = dict(part.split('=', 1) for part in line.split()[1:])
            frames.append({'t': int(fields['monotonic_ns']) / 1e9 + offset - real_ref, 'render_ms': float(fields['render_ms'])})
    commits = []  # first content-bearing frame: the panel scene's UI_COMMIT (Bead only)
    for line in log_path.read_text(errors='replace').splitlines():
        if line.startswith('UI_COMMIT ') and 'companion=true' in line:
            commits.append(int(re.search(r'monotonic_ns=(\d+)', line).group(1)) / 1e9 - mono_ref)
    results = []
    for window in windows:
        metrics = window_metrics(rows, window['start'], window['end'], frames)
        later = [c for c in commits if c >= window['mono_start']]
        if later and later[0] <= window['mono_start'] + (window['end'] - window['start']):
            metrics['content_commit_ms'] = round((later[0] - window['mono_start']) * 1000, 1)
        results.append({**window, **metrics})
    (self.out / 'pacing.json').write_text(json.dumps({'arm': self.name, 'hz': HZ, 'real_ref': real_ref, 'vsync_ms': VSYNC_MS,
                                                      'backend': 'cpu' if self.args.cpu else 'default',
                                                      'errors': self.errors, 'memory_kib': memory, 'windows': results}, indent=1))
    m.log(f'{self.name}: pacing {len(results)} windows, {len(self.errors)} errors')


def summarize(args, names):
    """Aggregate pacing.json of each arm into pacing-summary.json and a plain table."""
    table = {}
    for arm in args.arms.split(','):
        path = Path(args.out) / arm / 'pacing.json'
        if not path.is_file():
            continue
        for window in json.loads(path.read_text())['windows']:
            key = (window['animation'], window['phase'])
            cell = table.setdefault(key, {}).setdefault(arm, [])
            cell.append(window)
    rows, lines = {}, ['animation phase arm n commits dropped/p99ms/gap first_ms render_p99']
    for (animation, phase), arms in sorted(table.items()):
        for arm, ws in arms.items():
            good = [w for w in ws if w.get('commits')]
            if not good:
                rows[f'{animation}/{phase}/{arm}'] = {'frames': 0}
                lines.append(f'{animation} {phase} {arm}: no frames')
                continue
            def med(key, sub=None):
                values = [(w[key][sub] if sub else w[key]) for w in good if w.get(key) is not None and (not sub or w[key])]
                return percentile(values, .5) if values else None
            row = {'n': len(good), 'commits': med('commits'), 'dropped': sum(w['dropped'] for w in good) / len(good),
                   'drop_rate': med('drop_rate'), 'p99': med('interval_ms', 'p99'), 'p50': med('interval_ms', 'p50'),
                   'gap': med('longest_gap_ms'), 'first': med('first_frame_ms'), 'render_p99': med('render_ms', 'p99')}
            rows[f'{animation}/{phase}/{arm}'] = row
            lines.append(f"{animation} {phase} {arm}: n={row['n']} commits={row['commits']} dropped={row['dropped']:.1f} "
                         f"p50={row['p50']} p99={row['p99']} gap={row['gap']} first={row['first']} render_p99={row['render_p99']}")
    (Path(args.out) / f'pacing-summary-{HZ}hz.json').write_text(json.dumps(rows, indent=1))
    print('\n'.join(lines))
    return rows


def report(specs):
    """animation-pacing.py report label=DIR[:ARM] ...: one row per animation/phase, one column group per label."""
    columns = {}
    for spec in specs:
        label, _, rest = spec.partition('=')
        directory, _, arm = rest.partition(':')
        data = json.loads((Path(directory) / (arm or 'bead') / 'pacing.json').read_text())
        for window in data['windows']:
            columns.setdefault((window['animation'], window['phase']), {}).setdefault(label, []).append(window)
    labels = [spec.partition('=')[0] for spec in specs]
    print('animation/phase'.ljust(26) + ''.join(label.ljust(40) for label in labels))
    print(''.ljust(26) + ''.join('drop% p99ms gap first content rndr99 slow%'.ljust(40) for _ in labels))
    for key, by_label in sorted(columns.items()):
        cells = []
        for label in labels:
            good = [w for w in by_label.get(label, []) if w.get('commits')]
            if not good:
                cells.append('-'.ljust(40))
                continue
            def med(field, sub=None):
                values = [w[field][sub] if sub else w[field] for w in good if w.get(field) and (sub is None or w[field])]
                return percentile(values, .5) if values else None
            def fmt(v):
                return '-' if v is None else f'{v:.1f}'
            rate = med('drop_rate')
            cells.append(f"{fmt(None if rate is None else rate * 100)} {fmt(med('interval_ms', 'p99'))} {fmt(med('longest_gap_ms'))} {fmt(med('first_frame_ms'))} {fmt(med('content_commit_ms'))} {fmt(med('render_ms', 'p99'))} {fmt(None if med('render_over_vsync') is None else med('render_over_vsync') * 100)}".ljust(40))
        print('/'.join(key).ljust(26) + ''.join(cells))


m.Arm.drive = pace_drive
m.build_gallery = lambda args, names: summarize(args, names) and {}
if __name__ == '__main__':
    if sys.argv[1:2] == ['report']:
        report(sys.argv[2:])
        sys.exit(0)
    if '--states' not in sys.argv:
        sys.argv += ['--states', 'pace']
    sys.exit(m.main())
