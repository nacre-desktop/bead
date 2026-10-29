#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private B0119 click-to-focus and title-churn repro, built on parity-compare.py.

Use the parity-compare.py arguments (notably --nacre, --bead-bin, --bead-shell,
--out, --arms, --pointer-tool and --parent-output-tool). Set CLICK_TRIALS to
change the default 100 clicks. Each arm writes click-focus.json and paired
screenshots into its own output directory. The inherited capture guard, private
HOME, nested labwc/niri setup, swap checks and process cleanup remain active.
"""
import importlib.util
import hashlib
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__  # guarded re-execs must return through this wrapper
m.NIRI_CONFIG = m.NIRI_CONFIG.replace('input {', 'input {\n    focus-follows-mouse', 1)

TRIALS = int(os.environ.get('CLICK_TRIALS', '100'))
TITLE_SCRIPT = r'''import sys,time
import tty
tty.setraw(sys.stdin.fileno())
log=open(sys.argv[1],"ab",buffering=0)
i=0
while True:
 sys.stdout.write("\033]0;"+("◐ typing" if i%2==0 else "◑ typing")+"\007"); sys.stdout.flush()
 i+=1; time.sleep(.25)
 try:
  import select,os
  if select.select([sys.stdin],[],[],0)[0]:
   data=os.read(sys.stdin.fileno(),4096); log.write(data)
 except Exception: pass
'''


def _cpu_ticks(pid):
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1].split()
        return int(fields[11]) + int(fields[12])
    except (OSError, IndexError, ValueError):
        return None


def _focused_id(arm):
    rows = json.loads(arm.run(['niri', 'msg', '-j', 'windows']).stdout)
    if isinstance(rows, dict):
        rows = rows.get('windows', [])
    return next((str(row.get('id')) for row in rows if row.get('is_focused')), None)


def _windows(arm):
    rows = json.loads(arm.run(['niri', 'msg', '-j', 'windows']).stdout)
    return rows.get('windows', []) if isinstance(rows, dict) else rows


def _spawn(self, command, name, env=None, cwd=None):
    tracing = os.environ.get('CLICK_TRACE', '1') != '0'
    if name in ('shell', 'focus-target'):
        env = dict(env or self.env)
        if tracing:
            env['WAYLAND_DEBUG'] = 'client'
        else:
            env.pop('WAYLAND_DEBUG', None)
            env['BEAD_VERBOSE_LOG'] = '0'
    if name == 'niri':
        env = dict(env or self.env, RUST_LOG='niri=debug' if tracing else 'niri=warn')
    return _original_spawn(self, command, name, env, cwd)


_original_spawn = m.Arm.spawn
m.Arm.spawn = _spawn


def drive(self, _names):
    """One focus experiment per parity arm; processes are all Arm.spawn-owned."""
    a = self.args
    if not a.pointer_tool:
        raise RuntimeError('click-focus repro requires --pointer-tool')
    script = self.root / 'title-spinner.py'
    sink = self.out / 'stdin.raw'
    script.write_text(TITLE_SCRIPT)
    self.spawn(['alacritty', '-o', 'colors.primary.background="#234567"', '--class', 'nacre-ai-task',
                         '-e', sys.executable, str(script), str(sink)], 'focus-target')
    self.spawn(['alacritty', '--class', 'focus-other', '-e', 'sh', '-c',
                        'exec sleep 3600'], 'focus-other')
    self.errors = []
    result = {'arm': self.name, 'trials_requested': TRIALS,
              'backend': 'qml-default' if self.name == 'qml' else 'cpu' if a.cpu else 'default',
              'wayland_trace': os.environ.get('CLICK_TRACE', '1') != '0',
              'trials': [], 'cpu': {}, 'errors': self.errors}
    try:
        presentation = self.root / 'state/nacre/presentation.json'
        if presentation.is_file():
            record = json.loads(presentation.read_text())
            result['palette'] = {'mode': record['mode'], 'colours': record['colours']}
            poster = record.get('poster')
            if poster and Path(poster).is_file():
                result['wallpaper_sha256'] = hashlib.sha256(Path(poster).read_bytes()).hexdigest()
        wait = m.wait_until
        wait(lambda: any(w.get('app_id') == 'nacre-ai-task' for w in _windows(self)), 20,
             'nacre-ai-task window')
        wait(lambda: any(w.get('app_id') == 'focus-other' for w in _windows(self)), 20,
             'other terminal window')
        windows = _windows(self)
        target_window = next(w for w in windows if w.get('app_id') == 'nacre-ai-task')
        target_id = str(target_window['id'])
        other_window = next(w for w in windows if w.get('app_id') == 'focus-other')
        width, height = self.geometry['logical']
        w, h = round(width * .44), height - 150
        x, y = width - w - 24, 90
        # Stable private geometry: niri 26.04 reports null workspace-view
        # positions over IPC. Place two non-overlapping floating clients through
        # official actions rather than interpreting scrolling indices as pixels.
        for window, left in ((target_window, x), (other_window, 24)):
            ident = str(window['id'])
            for action, args in (
                ('move-window-to-floating', []),
                ('set-window-width', [str(w)]),
                ('set-window-height', [str(h)]),
                ('move-floating-window', ['--x', str(left), '--y', str(y)]),
            ):
                self.run(['niri', 'msg', 'action', action, '--id', ident, *args])
        time.sleep(.3)
        windows = _windows(self)
        result['requested_client_geometry'] = [x, y, w, h]
        # Build and close warm panels so retained surfaces remain mapped during
        # the spinner, matching the incident's B0110 keep-alive condition.
        self.steps([('ui', 'nacre', 'launcher', ''), ('sleep', 1.0), ('ui', 'nacre', 'close'), ('sleep', 1.0)])
        self.run(['grim', str(self.out / 'before.png')])
        (self.out / 'windows.json').write_text(json.dumps(windows, indent=2) + '\n')
        target_window = next(window for window in windows if str(window['id']) == target_id)
        actual_size = target_window.get('layout', {}).get('window_size')
        if actual_size != [w, h]:
            raise RuntimeError(f'client size {actual_size} differs from requested {[w, h]}')
        from PIL import Image, ImageChops
        image = Image.open(self.out / 'before.png').convert('RGB')
        red, green, blue = image.split()
        mask = ImageChops.multiply(red.point(lambda v: 255 if v == 0x23 else 0),
                                  green.point(lambda v: 255 if v == 0x45 else 0))
        mask = ImageChops.multiply(mask, blue.point(lambda v: 255 if v == 0x67 else 0))
        bounds = mask.getbbox()
        if not bounds:
            raise RuntimeError('target background not found in private screenshot')
        scale = self.geometry['scale']
        left, top, right, bottom = [value / scale for value in bounds]
        if abs(right - left - w) > 2 or abs(bottom - top - h) > 2:
            raise RuntimeError(f'target screenshot bounds {bounds} differ from client size {[w, h]}')
        result['verified_client_bounds'] = [left, top, right, bottom]
        points = [(round((left + right) / 2), round((top + bottom) / 2)),
                  (round(left + 12), round(top + 12)), (round(right - 12), round(bottom - 12))]
        for i in range(TRIALS):
            point = points[i % len(points)]
            away_id = str(other_window['id'])
            self.run(['niri', 'msg', 'action', 'focus-window', '--id', away_id])
            away_focused = _focused_id(self)
            # Arm.pointer retries through private parent labwc when niri lacks
            # wlr-virtual-pointer, matching parity-compare's established path.
            self.pointer(*point, mode='click')
            time.sleep(.03)
            focused = _focused_id(self)
            token = f'B{i:03d}Z'
            before = sink.stat().st_size if sink.exists() else 0
            self.run(['wtype', token], check=False)
            deadline = time.monotonic() + 2
            received = b''
            while time.monotonic() < deadline:
                try:
                    received = sink.read_bytes()[before:]
                except OSError:
                    pass
                if token.encode() in received:
                    break
                time.sleep(.02)
            row = {'trial': i + 1, 'point': point, 'focus_away_window_id': away_focused,
                   'focus_away_succeeded': away_focused == away_id,
                   'focused_window_id': focused,
                   'target_window_id': target_id, 'first_click_focused': focused == target_id,
                   'typed_received': token.encode() in received}
            result['trials'].append(row)
            if i % 20 == 19 and i + 1 < TRIALS:
                self.steps([('ui', 'nacre', 'launcher', ''), ('sleep', 0.4),
                            ('ui', 'nacre', 'close'), ('sleep', 0.4)])
            if i == 0:
                self.run(['grim', str(self.out / 'after-click.png')])
        # Title spinner remains active during this 30-second CPU interval.
        tracked = [('niri', p) for p in self.procs if p.args and p.args[0] == 'niri']
        tracked.append(('shell', self.shell))
        before = {name: _cpu_ticks(p.pid) for name, p in tracked}
        cpu_log_start = (self.logs / 'shell.log').stat().st_size
        began = time.monotonic()
        time.sleep(30)
        elapsed = time.monotonic() - began
        hz = os.sysconf('SC_CLK_TCK')
        result['cpu'] = {name: {'seconds': round(((after or 0) - (before.get(name) or 0)) / hz, 3),
                                'core_percent': round(100 * (((after or 0) - (before.get(name) or 0)) / hz) / elapsed, 3)}
                         for name, p in tracked if (after := _cpu_ticks(p.pid)) is not None and before.get(name) is not None}
        with (self.logs / 'shell.log').open('rb') as log:
            log.seek(cpu_log_start)
            result['backend_update_logs_during_cpu'] = log.read().count(b'BACKEND_UPDATE ')
        self.run(['wtype', 'QUIETCHECK'], check=True)
        time.sleep(.3)
        result['keyboard_after_quiet'] = _focused_id(self) == target_id and b'QUIETCHECK' in sink.read_bytes()
        # Retain complete seat transitions and focus-policy requests, with their
        # Wayland object IDs; the ordinary shell log is truncated by run_arm.
        for name in ('shell', 'focus-target', 'niri'):
            path = self.logs / f'{name}.log'
            lines = path.read_text(errors='replace').splitlines()
            if name != 'niri':
                lines = [line for line in lines if 'wl_keyboard' in line or 'keyboard_interactivity' in line
                         or 'get_layer_surface' in line or 'PANEL_' in line]
            (self.out / f'{name}-focus.log').write_text('\n'.join(lines) + '\n')
    except Exception as error:
        failure = f'{type(error).__name__}: {error}'
        self.errors.append(failure)
        result['failure'] = failure
        try:
            result['windows_on_failure'] = _windows(self)
        except Exception as snapshot_error:
            result['windows_snapshot_error'] = f'{type(snapshot_error).__name__}: {snapshot_error}'
        try:
            result['terminal_log_on_failure'] = sink.read_bytes().decode('utf-8', errors='replace')[-8000:]
        except OSError:
            result['terminal_log_on_failure'] = '<unavailable>'
        try:
            result['terminal_process_log_on_failure'] = (self.logs / 'focus-target.log').read_text(errors='replace')[-8000:]
        except OSError:
            result['terminal_process_log_on_failure'] = '<unavailable>'
    finally:
        # Arm.shutdown() owns all registered PIDs and stops them after the driver returns.
        pass
    (self.out / 'click-focus.json').write_text(json.dumps(result, indent=2) + '\n')
    passed = sum(t['first_click_focused'] and t['typed_received'] for t in result['trials'])
    m.log(f'{self.name}: {"FAILED " + result["failure"] + "; " if result.get("failure") else ""}'
          f'click focus and typing {passed}/{len(result["trials"])}')
    if result.get('failure'):
        raise RuntimeError(result['failure'])


m.Arm.drive = drive
m.build_gallery = lambda _args, _names: {}


if __name__ == '__main__':
    sys.exit(m.main())
