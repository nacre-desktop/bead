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

  animation-pacing.py report label=DIR[:ARM] ...   comparison table (medians over repeats, zero-drop windows included)
  animation-pacing.py rescore DIR ARM               recompute an earlier run's commit metrics from its shell.log
  A commit whose only damage is one pixel, after 100 ms of quiet, is housekeeping and not an animation frame (B0117).
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
import shutil
import subprocess
import sys
import threading
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
    'settings-reversal-wheel': ([('ui', 'nacre', 'settings'), ('sleep', 2.5)],
                                [('wheel', 'wheel-reverse', 1, 12, 0.12)], [], 1.6),
    'settings-reversal-touchpad': ([('ui', 'nacre', 'settings'), ('sleep', 2.5)],
                                   [('wheel', 'pixel-reverse', 7, 160, 0.008)], [], 1.6),
    'osd': ([], m.STATES['osd-volume'][0], m.STATES['osd-volume'][1], 1.6),
    'notification': ([], [('notify', 'Pacing check', 'A notification toast.')], [('ui', 'nacre', 'close')], 2.0),
}
CLOSE_APPS = {'settings-wheel': 'settings', 'settings-touchpad': 'settings'}
# B0135 scroll matrix: these scenes already have private QML/Bead fixtures in
# parity-compare. Coordinates are logical pixels in its 1920x1200 scene.
SCROLL_SCENES = {
    'settings-content': (m.STATES['settings'][0], 500, 600, 80, 1.6),
    'settings-nav': (m.STATES['settings'][0], 150, 600, 80, 1.6),
    'launcher': (m.STATES['launcher'][0], 960, 600, 80, 1.6),
    # The private popup begins around logical y=628; y=900 lands in its list.
    'keybindings': (m.STATES['keybindings'][0], 960, 900, 80, 1.6),
    'colors-app': (m.STATES['colors-app'][0], 500, 650, 80, 1.6),
    'colors-rail': (m.STATES['colors-app'][0], 150, 650, 80, 1.6),
    'welcome-hero': (m.STATES['welcome-hero'][0], 500, 650, 80, 1.6),
    'welcome-shortcuts': (m.STATES['welcome-shortcuts'][0], 500, 650, 80, 1.6),
    'welcome-apps': (m.STATES['welcome-apps'][0], 500, 650, 80, 1.6),
    'welcome-ready': (m.STATES['welcome-ready'][0], 500, 650, 80, 1.6),
    'welcome-help': (m.STATES['welcome-help'][0], 500, 650, 80, 1.6),
    'ai-chat': ([(step) for _ in range(12) for step in [('chat-seed',), ('sleep', 0.2)]]
                + m.STATES['sidebar-conversation'][0], 240, 650, 80, 1.6),
    # The picker is bottom-docked; scroll the hexagon grid, not the carousel.
    'wallpaper-picker': (m.STATES['wallpaper-hexagons'][0] + [('sleep', 1.0), ('click', 1070, 900)],
                         960, 1000, 80, 1.6),
    # Widgets has a Bead-only route in this harness; the natural registry drives the gallery.
    'widgets-gallery': ([('shell', 'nacre', 'menu', 'widgets')], 960, 600, 80, 1.6),
    'updates-popout': ([('ui', 'nacre', 'controls', 'updates')], 1600, 600, 80, 1.6),
    'clipboard': ([('only', 'qml', ('ui', 'drawers', 'toggle', 'dashboard')),
                   ('ui', 'nacre', 'tab', '6')], 500, 600, 80, 1.6),
    # Populate the private daemon's notification list before opening its panel.
    'notifications-list': ([('notify', f'Scroll fixture {i:02d}', f'Private synthetic notification row {i:02d}.')
                            for i in range(20)] + [('sleep', 2.0), ('close-fixture-notices',),
                                                   ('ui', 'nacre', 'controls', 'notifications')],
                           500, 300, 80, 1.6),
    'notifications-popup': ([('notify', f'Scroll fixture {i:02d}', f'Private synthetic notification row {i:02d}.')
                             for i in range(20)] + [('sleep', 2.0)], 1715, 650, 80, 1.6),
}
for _scene, (_prepare, _x, _y, _count, _seconds) in SCROLL_SCENES.items():
    ANIMS['scroll-' + _scene + '-touchpad'] = (
        list(_prepare), [('wheel', 'pixel-reverse', 7, _count * 2, 0.008, _x, _y)], [], _seconds)
    ANIMS['scroll-' + _scene + '-wheel'] = (
        list(_prepare), [('wheel', 'wheel-reverse', 1, 12, 0.12, _x, _y)], [], 1.6)
SCROLL_CLEANUPS = {
    **{f'scroll-{name}-{device}': [('app_close', 'settings')]
       for name in ('settings-content', 'settings-nav') for device in ('touchpad', 'wheel')},
    **{f'scroll-{name}-{device}': [('app_close', 'colors' if name in ('colors-app', 'colors-rail') else 'welcome')]
       for name in ('colors-app', 'colors-rail', 'welcome-hero', 'welcome-shortcuts', 'welcome-apps',
                    'welcome-ready', 'welcome-help')
       for device in ('touchpad', 'wheel')},
    **{f'scroll-ai-chat-{device}': [('ui', 'nacre', 'left')]
       for device in ('touchpad', 'wheel')},
    **{f'scroll-widgets-gallery-{device}': [('app_close', 'widgets')]
       for device in ('touchpad', 'wheel')},
}
_orig_start_daemon = m.Arm.start_daemon


def start_daemon(self):
    selected = os.environ.get('PACING_ANIMS')
    names = selected.split(',') if selected else list(ANIMS)
    if any(name.startswith('scroll-ai-chat-') for name in names):
        self.install_chat_fixture()
    if selected and any(name.startswith('scroll-clipboard-') for name in names):
        self.clipboard_fixture_dir = seed_clipboard_fixture(self)
        bin_dir = self.root / 'cliphist-bin'
        bin_dir.mkdir(exist_ok=True)
        cliphist = bin_dir / 'cliphist'
        cliphist.write_text(
            '#!/bin/sh\n'
            'test "$1" = list || exit 2\n'
            'i=120\n'
            'while test "$i" -gt 0; do printf "%s\\tSynthetic clipboard row %03d\\n" "$i" "$i"; i=$((i - 1)); done\n'
        )
        cliphist.chmod(0o700)
        self.env['PATH'] = str(bin_dir) + os.pathsep + self.env['PATH']
    _orig_start_daemon(self)


m.Arm.start_daemon = start_daemon

_orig_start_shell = m.Arm.start_shell


def start_shell(self):
    _orig_start_shell(self)
    if self.name == 'bead':
        m.wait_until(lambda: 'PRESENTATION clock_id=1' in (self.logs / 'shell.log').read_text(errors='replace'),
                     10, 'CLOCK_MONOTONIC presentation feedback clock')


m.Arm.start_shell = start_shell

_orig_notify = m.Arm.notify

def notify(self, summary, body):
    if not getattr(self, 'scroll_notice_fixture', False):
        return _orig_notify(self, summary, body)
    reply = self.run([
        'gdbus', 'call', '--session', '--dest', 'org.freedesktop.Notifications',
        '--object-path', '/org/freedesktop/Notifications',
        '--method', 'org.freedesktop.Notifications.Notify', 'parity', '0', '',
        summary, body, '[]', '{}', '60000'
    ], check=False)
    match = re.search(r'uint32\s+(\d+)', reply.stdout or '')
    if reply.returncode == 0 and match:
        self.fixture_notice_ids = getattr(self, 'fixture_notice_ids', [])
        self.fixture_notice_ids.append(int(match.group(1)))
        self.fixture_active_ids = getattr(self, 'fixture_active_ids', [])
        self.fixture_active_ids.append(int(match.group(1)))
    else:
        self.errors.append(f'notification fixture failed: {reply.stderr.strip()[:160]}')


def close_fixture_notifications(self, notice_ids):
    closed = []
    for notice_id in notice_ids:
        reply = self.run([
            'gdbus', 'call', '--session', '--dest', 'org.freedesktop.Notifications',
            '--object-path', '/org/freedesktop/Notifications',
            '--method', 'org.freedesktop.Notifications.CloseNotification', str(notice_id)
        ], check=False)
        if reply.returncode == 0:
            closed.append(notice_id)
        else:
            self.errors.append(f'notification fixture cleanup failed for id {notice_id}')
    return closed


def close_active_fixture_notifications(arm):
    active = list(getattr(arm, 'fixture_active_ids', []))
    closed = close_fixture_notifications(arm, active)
    arm.fixture_closed_ids = getattr(arm, 'fixture_closed_ids', [])
    arm.fixture_closed_ids.extend(closed)
    arm.fixture_active_ids = [notice_id for notice_id in active if notice_id not in closed]
    return closed


m.Arm.notify = notify


def seed_clipboard_fixture(arm):
    """Seed equivalent private text rows for QML cliphist and Bead's fixture topic."""
    fixture_root = arm.root / 'clipboard-fixtures' / 'dashboard'
    extras = fixture_root.with_name('dashboard-extras')
    fixture_root.mkdir(parents=True, exist_ok=True)
    extras.mkdir(parents=True, exist_ok=True)
    now_ms = int(time.time() * 1000)
    ids = [f'c{index:03d}' for index in range(120, 0, -1)]
    items = {
        item_id: {'kind': 'text', 'preview': f'Synthetic clipboard row {int(item_id[1:]):03d}',
                  'created_ms': now_ms - offset}
        for offset, item_id in enumerate(ids)
    }
    (extras / 'clipboard.json').write_text(json.dumps({
        'health': {'available': True, 'stale': False},
        'entries': {'items': items, 'order': ids},
    }, indent=2) + '\n')
    return fixture_root


def append_scroll_shortcut_fixture(path):
    """Add many harmless rows inside the existing private Niri binds block."""
    modifiers = (
        'Super+Ctrl', 'Super+Shift', 'Super+Alt', 'Ctrl+Shift', 'Ctrl+Alt', 'Shift+Alt',
        'Super+Ctrl+Shift', 'Super+Ctrl+Alt', 'Super+Shift+Alt', 'Ctrl+Shift+Alt',
    )
    rows = [f'    {modifier}+F{key} repeat=false hotkey-overlay-title="Nacre:scroll fixture {index:03d}" {{ spawn-sh "true"; }}'
            for index, (modifier, key) in enumerate(
                ((modifier, key) for modifier in modifiers for key in range(1, 13)), start=1)]
    source = path.read_text()
    prefix, separator, suffix = source.rpartition('\n}')
    if not separator:
        raise ValueError(f'expected closing brace in private shortcut fixture {path.name}')
    path.write_text(prefix + '\n' + '\n'.join(rows) + separator + suffix)


_orig_app_rss = m.Arm.app_rss


def app_rss(self):
    result = _orig_app_rss(self)
    if self.name != 'bead':
        return result
    import socket
    import struct
    for name in ('colors', 'welcome'):
        try:
            with socket.socket(socket.AF_UNIX) as client:
                client.connect(str(self.runtime / f'app-{name}.sock'))
                pid, _, _ = struct.unpack('3i', client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            result[name] = self.rss(pid)
        except OSError:
            pass
    return result


m.Arm.app_rss = app_rss
for _scene in SCROLL_SCENES:
    for _device in ('touchpad', 'wheel'):
        SCROLL_CLEANUPS.setdefault(f'scroll-{_scene}-{_device}', [('ui', 'nacre', 'close')])
for _name in ANIMS:
    m.STATES.setdefault('pace-' + _name, ([], [], 0))
m.STATES['pace'] = ([], [], 0)

_orig_spawn = m.Arm.spawn
_orig_run = m.Arm.run
_orig_steps = m.Arm.steps


def spawn(self, command, name, env=None, cwd=None):
    selected = os.environ.get('PACING_ANIMS', '').split(',')
    if name == 'niri' and any(item.startswith(('scroll-keybindings-', 'scroll-welcome-shortcuts-')) for item in selected):
        bindings = self.root / 'fixture-bindings.kdl'
        if bindings.is_file():
            append_scroll_shortcut_fixture(bindings)
    if name == 'shell':  # only the shell: niri/labwc must not trace as clients
        env = shell_trace_env(self.name, self.logs, env or self.env)
        if self.name == 'bead' and any(item.startswith('scroll-clipboard-') for item in selected):
            command = [*command, '--dashboard-fixtures', str(self.clipboard_fixture_dir)]
        if os.environ.get('PACING_PANEL_RENDERER') and self.name == 'bead':
            command = [*command, '--panel-renderer', os.environ['PACING_PANEL_RENDERER']]
        if os.environ.get('PACING_TRACE_STATE') == '1' and self.name == 'bead':
            env['BEAD_TRACE_STATE'] = '1'
            command = [*command, '--trace-state']
    return _orig_spawn(self, command, name, env, cwd)


def run(self, command, timeout=15, check=True, env=None):
    command = [part.replace('@60Hz', f'@{HZ}Hz') if isinstance(part, str) else part for part in command]
    return _orig_run(self, command, timeout, check, env)


def steps(self, operations):
    for step in operations:
        if step[0] == 'close-fixture-notices':
            close_active_fixture_notifications(self)
        elif step[0] == 'wheel':
            _, kind, delta, count, interval, *point = step
            reverse = kind.endswith('-reverse')
            axis_kind = kind.removesuffix('-reverse')
            oscillate = reverse and axis_kind == 'pixel' and os.environ.get('PACING_OSCILLATE') == '1'
            scale = self.geometry['scale']
            x, y = point if point else (1100, 600)
            command = [self.args.pointer_tool, str(round(x * scale)), str(round(y * scale)),
                       str(self.args.size[0]), str(self.args.size[1]),
                       scroll_stream_mode(axis_kind, reverse, oscillate),
                       axis_kind, str(delta), str(count), str(max(1, round(interval * 1000)))]
            if oscillate:
                command.append(os.environ.get('PACING_OSCILLATE_PERIOD', '3'))
            self.release_pointer()
            limit = count * interval + 10
            for display in (self.env['WAYLAND_DISPLAY'], self.env['BEAD_PARENT']):
                try:  # niri may lack wlr-virtual-pointer: the private parent labwc then forwards it
                    capture_mid = reverse and getattr(self, 'scroll_mid_capture', None)
                    if capture_mid:
                        completed = []
                        def inject():
                            try:
                                completed.append(subprocess.run(
                                    command, env=dict(self.env, WAYLAND_DISPLAY=display), timeout=limit,
                                    check=False, capture_output=True, text=True))
                            except Exception as error:
                                completed.append(error)
                        worker = threading.Thread(target=inject)
                        worker.start()
                        time.sleep(0.2 + count * interval / 2)
                        if worker.is_alive():
                            try:
                                self.scroll_mid_capture()
                            except Exception as error:
                                self.errors.append(f'mid-scroll screenshot: {error}')
                            finally:
                                self.scroll_mid_capture = None
                        worker.join(timeout=limit + 1)
                        if worker.is_alive():
                            raise subprocess.TimeoutExpired(command, limit)
                        result = completed[0]
                        if isinstance(result, Exception):
                            raise result
                    else:
                        result = subprocess.run(command, env=dict(self.env, WAYLAND_DISPLAY=display), timeout=limit,
                                                check=False, capture_output=True, text=True)
                    if result.returncode == 0:
                        if result.stdout:
                            with (self.out / 'axis-input.log').open('a') as trace:
                                trace.write(result.stdout)
                        break
                except subprocess.TimeoutExpired:
                    self.errors.append(f'wheel injection timed out on {display}')
        else:
            _orig_steps(self, [step])


m.Arm.spawn, m.Arm.run, m.Arm.steps = spawn, run, steps

SCROLL_STORE_KEYS = {
    'launcher': 'launcher.scroll.y', 'clipboard': 'dashboard.clipboard.scroll',
    'keybindings': 'extras.scroll',
    'settings-content': 'settings.scroll', 'settings-nav': 'settings.nav.scroll',
    'colors-app': 'colors.scroll', 'colors-rail': 'colors.rail.scroll', 'welcome-hero': 'welcome.scroll',
    'welcome-shortcuts': 'welcome.scroll', 'welcome-apps': 'welcome.scroll',
    'welcome-ready': 'welcome.scroll', 'welcome-help': 'welcome.scroll',
    'ai-chat': 'chat.scroll', 'wallpaper-picker': 'extras.scroll',
    'widgets-gallery': 'widgets.scroll',
    'notifications-list': 'dashboard.notifications.scroll', 'notifications-popup': 'notifications.scroll.y',
    'updates-popout': 'status.updates_scroll',
}


def archive_app_logs(log_dir, scene, repeat):
    """Preserve app subprocess logs before the next spawn truncates app-NAME.log."""
    archived = []
    for source in sorted(Path(log_dir).glob('app-*.log')):
        if '--' in source.stem or not source.is_file():
            continue
        target = source.with_name(f'{source.stem}--{scene}--repeat-{repeat}{source.suffix}')
        if target.exists():
            raise FileExistsError(f'refusing to overwrite archived app trace {target.name}')
        source.rename(target)
        archived.append(target)
    return archived


def shell_trace_env(arm_name, log_dir, env=None):
    result = dict(env or {})
    result.update(WAYLAND_DEBUG='client', BEAD_FRAME_TRACE='1', BEAD_VERBOSE_LOG='1')
    if arm_name == 'bead':
        result['BEAD_APP_LOG_DIR'] = str(log_dir)
        if os.environ.get('PACING_PROFILE_INPUT') == '1':
            result['BEAD_PROFILE_INPUT'] = '1'
    return result


def scroll_stream_mode(axis_kind, reverse, oscillate):
    if reverse and axis_kind == 'pixel' and oscillate:
        return 'stream-oscillate'
    return 'stream-reverse' if reverse else 'stream'


def percentile(values, p):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, math.ceil((len(ordered) - 1) * p))]


def reversal_proof(log_paths, input_path, animation, start_ns, end_ns):
    """Join delivered axis direction to a changed scroll store that was presented."""
    scene = animation.removeprefix('scroll-').removesuffix('-touchpad').removesuffix('-wheel')
    store_key = SCROLL_STORE_KEYS.get(scene)
    if not store_key:
        return {'status': 'missing', 'reason': 'no traced scroll store is mapped for this surface'}
    input_lines = input_path.read_text(errors='replace').splitlines() if input_path.is_file() else []
    inputs = []
    for line in input_lines:
        if not line.startswith('AXIS_SENT '):
            continue
        fields = dict(part.split('=', 1) for part in line.split()[1:])
        stamp = int(fields['monotonic_ns'])
        if start_ns <= stamp <= end_ns:
            inputs.append((stamp, float(fields['delta'])))
    reversal_input = next(((stamp, delta) for (_, old), (stamp, delta) in zip(inputs, inputs[1:])
                           if old * delta < 0), None)
    if not reversal_input:
        return {'status': 'missing', 'reason': 'no contiguous input direction reversal in the window'}
    input_ns, input_delta = reversal_input
    candidates = []
    for path in log_paths:
        pending_state = None
        by_ui_update = {}
        window_states = {}
        window_by_id = {}
        delivered = []
        samples = []
        for line in path.read_text(errors='replace').splitlines():
            if line.startswith('UI_STATE '):
                try:
                    pending_state = json.loads(line.partition('UI_STATE ')[2])
                except json.JSONDecodeError:
                    pending_state = None
            elif line.startswith('AXIS_DELIVER '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if 'monotonic_ns' in fields and 'delta' in fields:
                    delivered.append((int(fields['monotonic_ns']), float(fields['delta'])))
            elif line.startswith('WINDOW_STATE '):
                prefix, _, state_text = line.partition(' state=')
                fields = dict(part.split('=', 1) for part in prefix.split()[1:] if '=' in part)
                try:
                    state = json.loads(state_text)
                    if store_key in state:
                        key = (fields['epoch'], fields['update'], fields['generation'])
                        window_states[key] = (int(fields['monotonic_ns']), float(state[store_key]))
                except (json.JSONDecodeError, KeyError, ValueError, TypeError):
                    pass
            elif line.startswith('WINDOW_COMMIT '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                key = (fields.get('epoch'), fields.get('update'), fields.get('generation'))
                if key in window_states:
                    window_by_id[key] = window_states[key][1]
            elif line.startswith('WINDOW_PRESENTED '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                key = (fields.get('epoch'), fields.get('update'), fields.get('generation'))
                if key in window_by_id and 'monotonic_ns' in fields:
                    stamp = int(fields['monotonic_ns'])
                    if stamp <= end_ns:
                        samples.append((stamp, window_by_id[key], ('window', fields.get('epoch'))))
            elif line.startswith('UI_COMMIT '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if pending_state is not None and 'update' in fields and store_key in pending_state:
                    key = (fields.get('companion'), fields.get('generation'), fields['update'])
                    by_ui_update[key] = float(pending_state[store_key])
                pending_state = None
            elif line.startswith('UI_PRESENTED '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                key = (fields.get('companion'), fields.get('generation'), fields.get('update'))
                if key in by_ui_update and 'monotonic_ns' in fields:
                    stamp = int(fields['monotonic_ns'])
                    if stamp <= end_ns:
                        samples.append((stamp, by_ui_update[key], ('ui', fields.get('companion'))))
        delivered.sort()
        samples.sort()
        # Keep two last actually-presented pre-window offsets only for a native
        # window epoch; UI_* has no source epoch, so it cannot safely seed a baseline.
        active = [sample for sample in samples if start_ns <= sample[0] <= end_ns]
        active_series = {sample[2] for sample in active}
        prior = {}
        for sample in samples:
            if (sample[0] < start_ns and sample[2] in active_series
                    and sample[2][0] == 'window'):
                prior.setdefault(sample[2], []).append(sample)
        preceding = [sample for values in prior.values() for sample in values[-2:]]
        samples = sorted([*preceding, *active])
        forward_input = inputs[0][1] if inputs else 0
        prior_delivery = next(((stamp, delta) for stamp, delta in reversed(delivered)
                               if inputs and inputs[0][0] <= stamp < input_ns), None)
        if not prior_delivery or not forward_input:
            continue
        direction_sign = 1 if prior_delivery[1] * forward_input > 0 else -1
        flips = [(stamp, delta) for (_, old), (stamp, delta) in zip(delivered, delivered[1:])
                 if stamp >= input_ns and old * delta < 0 and delta * input_delta * direction_sign > 0]
        if not flips or len(samples) < 2:
            continue
        flip_ns, flip_delta = flips[0]
        prior_moves = [b[1] - a[1] for a, b in zip(samples, samples[1:])
                       if b[0] < flip_ns and b[1] != a[1]]
        if not prior_moves:
            continue
        expected = -1 if prior_moves[-1] > 0 else 1
        for previous, current in zip(samples, samples[1:]):
            if current[0] >= flip_ns and (current[1] - previous[1]) * expected > 0:
                candidates.append({'status': 'proven', 'store': store_key,
                                   'input_reversal_ns': input_ns, 'delivered_reversal_ns': flip_ns,
                                   'presented_direction_change_ns': current[0],
                                   'input_to_visible_ms': round((current[0] - input_ns) / 1e6, 2),
                                   'delivery_to_visible_ms': round((current[0] - flip_ns) / 1e6, 2),
                                   'input_delta': input_delta, 'delivered_delta': flip_delta,
                                   'presented_samples': len(samples)})
                break
    return candidates[0] if candidates else {
        'status': 'missing', 'reason': 'no presented scroll-store direction change joined to delivered opposite axis input',
        'store': store_key, 'input_reversal_ns': input_ns}


def window_commit_metrics(log_paths, start_ns, end_ns):
    """Cadence from the dedicated window renderer's frame/commit trace."""
    by_surface = {}
    for path in log_paths:
        for line in path.read_text(errors='replace').splitlines():
            if not line.startswith('WINDOW_COMMIT '):
                continue
            fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
            if not {'monotonic_ns', 'epoch'} <= fields.keys():
                continue
            stamp = int(fields['monotonic_ns'])
            if start_ns <= stamp <= end_ns:
                by_surface.setdefault((path.name, fields['epoch']), []).append(stamp / 1e9)
    if not by_surface:
        return None
    _, stamps = max(by_surface.items(), key=lambda item: len(item[1]))
    stamps.sort()
    intervals = [(b - a) * 1000 for a, b in zip(stamps, stamps[1:]) if (b - a) * 1000 < IDLE_GAP_MS]
    if not intervals:
        return {'commits': len(stamps), 'cadence_source': 'WINDOW_COMMIT', 'frames': len(stamps)}
    dropped = sum(max(0, round(interval / VSYNC_MS) - 1)
                  for interval in intervals if interval > 1.5 * VSYNC_MS)
    return {'commits': len(stamps), 'cadence_source': 'WINDOW_COMMIT',
            'interval_ms': {k: round(percentile(intervals, p), 2)
                            for k, p in (('p50', .5), ('p95', .95), ('p99', .99))},
            'longest_gap_ms': round(max(intervals), 1), 'dropped': dropped,
            'slow_intervals': sum(interval > 1.5 * VSYNC_MS for interval in intervals),
            'drop_rate': round(dropped / max(1, dropped + len(intervals)), 3)}


def presented_cadence(log_paths, rows, start_ns, end_ns, start_rel, end_rel):
    """Actual wp_presentation feedback cadence, grouped by presented surface."""
    groups = {}
    for path in log_paths:
        for line in path.read_text(errors='replace').splitlines():
            if line.startswith(('WINDOW_PRESENTED ', 'UI_PRESENTED ')):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if 'monotonic_ns' not in fields:
                    continue
                stamp = int(fields['monotonic_ns'])
                if not start_ns <= stamp <= end_ns:
                    continue
                if line.startswith('WINDOW_PRESENTED '):
                    key = ('window', path.name, fields.get('epoch'))
                else:
                    key = ('ui', path.name, fields.get('companion'))
                group = groups.setdefault(key, {'stamps': [], 'refresh_ns': [], 'discarded': 0})
                group['stamps'].append(stamp)
                if fields.get('refresh_ns', '').isdigit():
                    group['refresh_ns'].append(int(fields['refresh_ns']))
            elif line.startswith(('WINDOW_DISCARDED ', 'UI_DISCARDED ')):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                stamp = int(fields['monotonic_ns']) if fields.get('monotonic_ns', '').isdigit() else None
                if stamp is not None and start_ns <= stamp <= end_ns:
                    key = (('window', path.name, fields.get('epoch')) if line.startswith('WINDOW_DISCARDED ')
                           else ('ui', path.name, fields.get('companion')))
                    groups.setdefault(key, {'stamps': [], 'refresh_ns': [], 'discarded': 0})['discarded'] += 1
    if groups:
        windows = {key: value for key, value in groups.items() if key[0] == 'window' and value['stamps']}
        ui = {key: value for key, value in groups.items() if key[0] == 'ui' and value['stamps']}
        pool = windows or {key: value for key, value in ui.items() if key[2] == 'true'} or ui
        if pool:
            key, chosen = max(pool.items(), key=lambda item: len(item[1]['stamps']))
            source = f'{key[0]}:{key[1]}:{key[2]}'
        else:
            chosen = None
            source = None
    else:
        chosen = None
        source = None
    # QML has no Bead-specific trace. Its client-side presentation event still gives
    # real compositor feedback timestamps, although multiple QML surfaces share the log.
    if chosen is None:
        by_client = {}
        for stamp, request, interface, ident, event, args in rows:
            if not request and interface == 'wp_presentation_feedback' and event in ('presented', 'discarded'):
                parts = [part.strip() for part in args.split(',')]
                if event == 'presented' and len(parts) >= 4:
                    ns = ((int(parts[0], 0) << 32) | int(parts[1], 0)) * 1_000_000_000 + int(parts[2], 0)
                    if start_ns <= ns <= end_ns:
                        key = ident.split(':', 1)[0]
                        group = by_client.setdefault(key, {'stamps': [], 'refresh_ns': [], 'discarded': 0})
                        group['stamps'].append(ns)
                        group['refresh_ns'].append(int(parts[3], 0))
                elif event == 'discarded' and start_rel <= stamp <= end_rel:
                    key = ident.split(':', 1)[0]
                    by_client.setdefault(key, {'stamps': [], 'refresh_ns': [], 'discarded': 0})['discarded'] += 1
        if by_client:
            key, chosen = max(by_client.items(), key=lambda item: len(item[1]['stamps']))
            source = f'wayland-client:{key}'
    if not chosen or len(chosen['stamps']) < 2:
        return {'presentation_source': source, 'presented_frames': len(chosen['stamps']) if chosen else None,
                'presentation_interval_ms': None, 'presented_dropped': None,
                'presented_drop_rate': None, 'presented_discarded': chosen['discarded'] if chosen else None,
                'presented_refresh_ns': percentile(chosen['refresh_ns'], .5) if chosen and chosen['refresh_ns'] else None}
    stamps = sorted(chosen['stamps'])
    intervals = [(b - a) / 1e6 for a, b in zip(stamps, stamps[1:])]
    if not intervals:
        return {'presentation_source': source, 'presented_frames': len(stamps),
                'presentation_interval_ms': None, 'presented_dropped': None,
                'presented_drop_rate': None, 'presented_discarded': chosen['discarded'],
                'presented_refresh_ns': percentile(chosen['refresh_ns'], .5) if chosen['refresh_ns'] else None}
    dropped = sum(max(0, round(interval / VSYNC_MS) - 1)
                  for interval in intervals if interval > 1.5 * VSYNC_MS)
    long_gaps = [interval for interval in intervals if interval >= IDLE_GAP_MS]
    return {'presentation_source': source, 'presented_frames': len(stamps),
            'presentation_interval_ms': {key: round(percentile(intervals, p), 2)
                                         for key, p in (('p50', .5), ('p95', .95), ('p99', .99))},
            'presented_dropped': dropped,
            'presented_drop_rate': round(dropped / max(1, dropped + len(intervals)), 3),
            'presented_long_gaps': len(long_gaps),
            'presented_max_gap_ms': round(max(intervals), 2),
            'presented_discarded': chosen['discarded'],
            'presented_refresh_ns': percentile(chosen['refresh_ns'], .5) if chosen['refresh_ns'] else None}


def input_cadence(path, start_ns, end_ns):
    stamps = []
    if path.is_file():
        for line in path.read_text(errors='replace').splitlines():
            if line.startswith('AXIS_SENT '):
                fields = dict(part.split('=', 1) for part in line.split()[1:])
                stamp = int(fields['monotonic_ns'])
                if start_ns <= stamp <= end_ns:
                    stamps.append(stamp)
    intervals = [(b - a) / 1e6 for a, b in zip(stamps, stamps[1:])]
    return {'events': len(stamps), 'interval_ms': {
        'p50': round(percentile(intervals, .5), 2), 'p95': round(percentile(intervals, .95), 2),
        'max': round(max(intervals), 2)} if intervals else None}


def trace_profile_metrics(log_paths):
    worker_ms = []
    worker_builds = []
    patch_misses = {}
    profile_wall_ms = {}
    profile_cpu_ms = {}
    timeline_marks = {}
    recovery_generations = []
    for path in log_paths:
        for line in path.read_text(errors='replace').splitlines():
            fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
            if line.startswith('WORKER_PREPARE '):
                if fields.get('total_ms', '').replace('.', '', 1).isdigit():
                    worker_ms.append(float(fields['total_ms']))
                if fields.get('builds', '').isdigit():
                    worker_builds.append(int(fields['builds']))
            elif line.startswith('INPUT_PATCH_MISS '):
                kind = fields.get('kind', 'unknown')
                patch_misses[kind] = patch_misses.get(kind, 0) + 1
            elif line.startswith('INPUT_PROFILE '):
                stage = fields.get('stage', 'unknown')
                value = fields.get('wall_ms', '')
                if value.replace('.', '', 1).isdigit():
                    profile_wall_ms.setdefault(stage, []).append(float(value))
                value = fields.get('cpu_ms', '')
                if value.replace('.', '', 1).isdigit():
                    profile_cpu_ms.setdefault(stage, []).append(float(value))
            elif line.startswith(('UI_TIMELINE ', 'UI_POINTER ')):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                stage = fields.get('stage', 'unknown')
                timeline_marks[stage] = timeline_marks.get(stage, 0) + 1
            elif line.startswith('RECOVERY_BUNDLE_RECORDED '):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if fields.get('generation', '').isdigit():
                    recovery_generations.append(int(fields['generation']))
    return {
        'worker_prepare_count': len(worker_ms),
        'worker_prepare_ms': ({key: round(percentile(worker_ms, p), 3)
                               for key, p in (('p50', .5), ('p95', .95), ('p99', .99))}
                              | {'max': round(max(worker_ms), 3)} if worker_ms else None),
        'worker_builds_max': max(worker_builds) if worker_builds else None,
        'input_patch_misses': patch_misses,
        'input_profile_wall_ms_by_stage': {stage: {'count': len(values), 'p95': round(percentile(values, .95), 3),
                                                   'max': round(max(values), 3)}
                                           for stage, values in profile_wall_ms.items()},
        'input_profile_cpu_ms_by_stage': {stage: {'count': len(values), 'p95': round(percentile(values, .95), 3),
                                                  'max': round(max(values), 3)}
                                          for stage, values in profile_cpu_ms.items()},
        'timeline_marks_by_stage': timeline_marks,
        'recovery_bundle_generations': recovery_generations,
    }


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
            if event not in ('commit', 'done', 'presented', 'discarded', 'attach', 'damage', 'damage_buffer'):
                continue
            sod = int(hh) * 3600 + int(mm) * 60 + int(ss) + int(micro) / 1e6
            # Surface ids are process-local, so retain the log's process name in the key.
            rows.append(((sod - ref_sod) % 86400, bool(arrow), interface,
                         f'{Path(path).name}:{int(ident)}', event, args))
    return rows


def window_metrics(rows, start, end, frames):
    commits, attached, visible, seen, housekeeping, last = {}, set(), set(), set(), 0, {}
    for stamp, request, interface, ident, event, args in rows:
        if not (request and interface == 'wl_surface'):
            continue
        if event == 'attach':  # a commit carries a visible frame only if a buffer was attached since the last one
            if not args.startswith('nil'):
                attached.add(ident)
        elif event in ('damage', 'damage_buffer'):
            seen.add(ident)
            values = [part.strip() for part in args.split(',')]
            if len(values) == 4 and int(values[2]) * int(values[3]) > 1:
                visible.add(ident)  # more than a 1x1 touch of the buffer
            elif len(values) != 4:
                visible.add(ident)
        elif event == 'commit':
            # A commit whose only damage is a single pixel changes nothing on screen (Bead sends one to get
            # presentation feedback for a scene that painted identically). Inside a running animation it is
            # still its frame; after 100 ms of quiet it is housekeeping and not part of the animation.
            carries = ident in attached and (ident in visible or ident not in seen)
            if ident in attached and not carries and stamp - last.get(ident, -1e9) > 0.1:
                if start <= stamp <= end:
                    housekeeping += 1
            elif ident in attached and start <= stamp <= end:
                commits.setdefault(ident, []).append(stamp)
            if ident in attached:
                last[ident] = stamp
            attached.discard(ident)
            visible.discard(ident)
            seen.discard(ident)
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
        'housekeeping_commits': housekeeping,
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
        result['render_ms'] = {'p50': round(percentile(cpu, .5), 3), 'p95': round(percentile(cpu, .95), 3),
                               'p99': round(percentile(cpu, .99), 3), 'max': round(max(cpu), 3)}
        result['render_over_vsync'] = round(sum(c > VSYNC_MS for c in cpu) / len(cpu), 3)
    return result


def pace_drive(self, names):
    selected = os.environ.get('PACING_ANIMS')
    names = selected.split(',') if selected else list(ANIMS)
    real_ref = time.time()
    mono_ref = time.monotonic()
    windows = []
    memory = {}
    memory_phase = {}
    self.errors = []
    notification_fixture_runs = []
    self.fixture_notice_ids = []
    self.fixture_active_ids = []
    self.fixture_closed_ids = []
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
                self.scroll_notice_fixture = name.startswith('scroll-notifications')
                if self.scroll_notice_fixture:
                    self.fixture_notice_ids = []
                    self.fixture_active_ids = []
                    self.fixture_closed_ids = []
                self.steps(prepare)
                if name.startswith('scroll-'):
                    time.sleep(2.0)  # Wait for the opened view/panel to paint before visual or input sampling.
                screenshot_base = None
                screenshot_mid_enabled = os.environ.get('PACING_CAPTURE_MID') == '1'
                if name.startswith('scroll-') and repeat == 0:
                    screenshot_base = self.out / 'scroll-evidence' / name
                    screenshot_base.parent.mkdir(parents=True, exist_ok=True)
                    self.grab(screenshot_base.with_name(name + '-before.png'), scale=0.5)
                phases = [('open', opens)] if name.startswith(('scroll-', 'settings-reversal-')) else [('open', opens), ('close', closes)]
                for phase, phase_steps in phases:
                    start = time.time()
                    mono = time.monotonic()
                    try:
                        if screenshot_base is not None and screenshot_mid_enabled and phase == 'open':
                            self.scroll_mid_capture = lambda: self.grab(
                                screenshot_base.with_name(name + '-mid.png'), scale=0.5)
                        self.steps(phase_steps)
                    finally:
                        if screenshot_base is not None:
                            self.scroll_mid_capture = None
                    time.sleep(max(0, seconds - (time.monotonic() - mono)))
                    windows.append({'animation': name, 'phase': phase, 'repeat': repeat,
                                    'start': start - real_ref, 'end': time.time() - real_ref,
                                    'mono_start': mono - mono_ref, 'mono_start_ns': int(mono * 1e9),
                                    'mono_end_ns': int(time.monotonic() * 1e9),
                                    'screenshot_sampled': bool(screenshot_base is not None and screenshot_mid_enabled)})
                    time.sleep(0.5)
                    if screenshot_base is not None and phase == 'open':
                        self.grab(screenshot_base.with_name(name + '-after.png'), scale=0.5)
                if name.startswith('scroll-'):
                    try:
                        memory_phase.setdefault(name, []).append({
                            'repeat': repeat, 'shell': self.rss(), 'apps': self.app_rss()})
                    except Exception as error:
                        self.errors.append(f'memory {name}: {error}')
                if name in CLOSE_APPS:
                    self.steps([('app_close', CLOSE_APPS[name])])
                if name in SCROLL_CLEANUPS:
                    self.steps(SCROLL_CLEANUPS[name])
                if name.startswith('scroll-notifications'):
                    close_active_fixture_notifications(self)
                time.sleep(self.args.closed_settle)
                try:
                    memory[name] = self.rss()
                except Exception as error:
                    self.errors.append(f'rss {name}: {error}')
            except Exception as error:
                self.errors.append(f'{name}: {str(error)[:200]}')
            finally:
                try:
                    archive_app_logs(self.logs, name, repeat)
                except Exception as error:
                    self.errors.append(f'archiving app logs {name}: {error}')
                if name.startswith('scroll-notifications'):
                    notice_ids = list(self.fixture_notice_ids)
                    close_active_fixture_notifications(self)
                    closed_ids = list(self.fixture_closed_ids)
                    notification_fixture_runs.append({
                        'scene': name, 'repeat': repeat, 'posted_ids': notice_ids,
                        'closed_ids': closed_ids, 'ttl_ms': 60000
                    })
                    self.fixture_notice_ids = []
                    (self.out / 'notification-fixtures.json').write_text(
                        json.dumps(notification_fixture_runs, indent=1) + '\n')
                self.scroll_notice_fixture = False
    time.sleep(0.5)
    log_path = self.logs / 'shell.log'
    trace_logs = sorted(self.logs.glob('*.log'))
    rows = [row for source in trace_logs for row in parse_trace(source, real_ref)]
    frames = []
    trace_paths = []
    trace_dir = self.out / 'scroll-traces'
    if os.environ.get('PACING_KEEP_RAW_APP_LOGS') == '1':
        raw_app_dir = self.out / 'raw-app-logs'
        for source in trace_logs:
            if source.name.startswith('app-'):
                raw_app_dir.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, raw_app_dir / source.name)
    for source in trace_logs:
        lines = source.read_text(errors='replace').splitlines()
        selected = [line for line in lines if line.startswith((
            'UI_STATE ', 'UI_COMMIT ', 'UI_PRESENTED ', 'AXIS_DELIVER ', 'UI_FRAME ',
            'WINDOW_STATE ', 'WINDOW_COMMIT ', 'WINDOW_FRAME ', 'WINDOW_PRESENTED ',
            'WINDOW_DISCARDED ', 'UI_DISCARDED ', 'PRESENTATION clock_id=',
            'QML_PRESENTATION clock_id=', 'WORKER_PREPARE ', 'INPUT_PATCH_MISS ',
            'INPUT_PROFILE ', 'UI_TIMELINE ', 'UI_POINTER ', 'RECOVERY_BUNDLE_RECORDED '))]
        if selected:
            trace_dir.mkdir(parents=True, exist_ok=True)
            target = trace_dir / source.name
            target.write_text('\n'.join(selected) + '\n')
            trace_paths.append(target)
        # Both frame record types use CLOCK_MONOTONIC_NS and report renderer CPU cost.
        for line in selected:
            if line.startswith(('UI_FRAME ', 'WINDOW_FRAME ')):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if 'monotonic_ns' in fields and 'render_ms' in fields:
                    frames.append({'t': int(fields['monotonic_ns']) / 1e9 - mono_ref,
                                   'render_ms': float(fields['render_ms'])})
    commits = []  # first content-bearing frame: the panel scene's UI_COMMIT (Bead only)
    for line in log_path.read_text(errors='replace').splitlines():
        if line.startswith('UI_COMMIT ') and 'companion=true' in line:
            commits.append(int(re.search(r'monotonic_ns=(\d+)', line).group(1)) / 1e9 - mono_ref)
    results = []
    for window in windows:
        metrics = window_metrics(rows, window['start'], window['end'], frames)
        committed = window_commit_metrics(trace_paths, window['mono_start_ns'], window['mono_end_ns'])
        if committed:
            metrics.update(committed)
        metrics.update(presented_cadence(trace_paths, rows, window['mono_start_ns'], window['mono_end_ns'],
                                          window['start'], window['end']))
        if window['animation'].startswith('scroll-'):
            metrics['input_cadence'] = input_cadence(self.out / 'axis-input.log',
                                                      window['mono_start_ns'], window['mono_end_ns'])
            metrics['reversal'] = reversal_proof(trace_paths, self.out / 'axis-input.log',
                                                  window['animation'], window['mono_start_ns'], window['mono_end_ns'])
        later = [c for c in commits if c >= window['mono_start']]
        if later and later[0] <= window['mono_start'] + (window['end'] - window['start']):
            metrics['content_commit_ms'] = round((later[0] - window['mono_start']) * 1000, 1)
        results.append({**window, **metrics})
    presentation_clock_ids = sorted({
        line.split('clock_id=', 1)[1].split()[0]
        for path in trace_paths for line in path.read_text(errors='replace').splitlines()
        if line.startswith(('PRESENTATION clock_id=', 'QML_PRESENTATION clock_id='))
    })
    refresh_values = []
    presentation_discarded = 0
    discarded_trace_collected = False
    for path in trace_paths:
        for line in path.read_text(errors='replace').splitlines():
            if line.startswith(('WINDOW_DISCARDED ', 'UI_DISCARDED ')):
                presentation_discarded += 1
            if line.startswith(('WINDOW_FRAME ', 'WINDOW_PRESENTED ', 'UI_FRAME ', 'UI_PRESENTED ')):
                discarded_trace_collected = True
            if line.startswith(('UI_PRESENTED ', 'WINDOW_PRESENTED ')):
                fields = dict(part.split('=', 1) for part in line.split()[1:] if '=' in part)
                if fields.get('refresh_ns', '').isdigit():
                    refresh_values.append(int(fields['refresh_ns']))
    presented_refresh_ns = percentile(refresh_values, .5) if refresh_values else None
    trace_profile = trace_profile_metrics(trace_paths)
    (self.out / 'pacing.json').write_text(json.dumps({'arm': self.name, 'hz': HZ, 'real_ref': real_ref, 'vsync_ms': VSYNC_MS,
                                                      'backend': 'cpu' if self.args.cpu else 'default',
                                                      'presentation_clock_ids': presentation_clock_ids,
                                                      'presented_refresh_ns_median': presented_refresh_ns,
                                                      'presented_refresh_hz': round(1e9 / presented_refresh_ns, 3) if presented_refresh_ns else None,
                                                      'presented_refresh_matches_requested_hz': bool(presented_refresh_ns and
                                                                                                     abs(presented_refresh_ns - round(1e9 / HZ)) <= round(1e9 / HZ * .05)),
                                                      'input_pattern': ('finger-oscillating/wheel-single-reversal'
                                                                        if os.environ.get('PACING_OSCILLATE') == '1'
                                                                        else 'single-midpoint-reversal'),
                                                      'input_oscillation_period': int(os.environ.get('PACING_OSCILLATE_PERIOD', '3')) if os.environ.get('PACING_OSCILLATE') == '1' else None,
                                                      'trace_profile': trace_profile,
                                                      'presentation_discarded': presentation_discarded if discarded_trace_collected else None,
                                                      'presentation_discarded_trace_collected': discarded_trace_collected,
                                                      'errors': self.errors, 'memory_kib': memory,
                                                      'memory_phase_kib': memory_phase, 'windows': results}, indent=1))
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
                values = []
                for window in good:
                    value = window.get(key)
                    for field in sub.split('.') if sub else ():
                        value = value.get(field) if isinstance(value, dict) else None
                    if value is not None:
                        values.append(value)
                return percentile(values, .5) if values else None
            clean = [window for window in good if not window.get('screenshot_sampled')]
            dropped = [window.get('dropped') for window in good]
            row = {'n': len(good), 'commits': med('commits'),
                   'commit_dropped': sum(dropped) / len(good) if all(value is not None for value in dropped) else None,
                   'commit_drop_rate': med('drop_rate'), 'commit_p99': med('interval_ms', 'p99'),
                   'commit_p95': med('interval_ms', 'p95'), 'commit_p50': med('interval_ms', 'p50'),
                   'presented_dropped': med('presented_dropped'),
                   'presented_long_gaps': med('presented_long_gaps'),
                   'presented_max_gap_ms': med('presented_max_gap_ms'),
                   'presented_discarded': med('presented_discarded'),
                   'presented_drop_rate': med('presented_drop_rate'),
                   'presented_p95': med('presentation_interval_ms', 'p95'),
                   'presented_p99': med('presentation_interval_ms', 'p99'),
                   'gap': med('longest_gap_ms'), 'first': med('first_frame_ms'),
                   'render_p95': med('render_ms', 'p95'), 'render_p99': med('render_ms', 'p99'),
                   'input_p95': med('input_cadence', 'interval_ms.p95'),
                   'reversal_ms': med('reversal', 'delivery_to_visible_ms'),
                   'screenshot_sampled': sum(bool(window.get('screenshot_sampled')) for window in good),
                   'unperturbed_n': len(clean),
                   'unperturbed_render_p95': percentile([w['render_ms']['p95'] for w in clean
                                                         if w.get('render_ms')], .5),
                   'unperturbed_commit_p95': percentile([w['interval_ms']['p95'] for w in clean
                                                         if w.get('interval_ms')], .5),
                   'unperturbed_presented_p95': percentile([w['presentation_interval_ms']['p95'] for w in clean
                                                             if w.get('presentation_interval_ms')], .5)}
            rows[f'{animation}/{phase}/{arm}'] = row
            commit_drop_text = '-' if row['commit_dropped'] is None else f"{row['commit_dropped']:.1f}"
            lines.append(f"{animation} {phase} {arm}: n={row['n']} commits={row['commits']} commit_dropped={commit_drop_text} "
                         f"presented_dropped={row['presented_dropped']} present_p95={row['presented_p95']} "
                         f"presented_discarded={row['presented_discarded']} "
                         f"present_p99={row['presented_p99']} commit_p50={row['commit_p50']} "
                         f"commit_p95={row['commit_p95']} commit_p99={row['commit_p99']} "
                         f"gap={row['gap']} first={row['first']} "
                         f"render_p95={row['render_p95']} render_p99={row['render_p99']} "
                         f"input_p95={row['input_p95']} reversal_ms={row['reversal_ms']} "
                         f"sampled={row['screenshot_sampled']} clean_render95={row['unperturbed_render_p95']} "
                         f"clean_present95={row['unperturbed_presented_p95']}")
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
    print(''.ljust(26) + ''.join('present_drop% pdrop# present95 present99 commit95 commit99 gap first content render95 all/clean reversal'.ljust(100) for _ in labels))
    for key, by_label in sorted(columns.items()):
        cells = []
        for label in labels:
            good = [w for w in by_label.get(label, []) if w.get('commits')]
            if not good:
                cells.append('-'.ljust(40))
                continue
            clean = [window for window in good if not window.get('screenshot_sampled')]
            def med(field, sub=None, items=None):
                # A window with no dropped frame is a zero, not a missing value: it belongs in the median.
                values = []
                for window in good if items is None else items:
                    value = window.get(field)
                    for key in sub.split('.') if sub else ():
                        value = value.get(key) if isinstance(value, dict) else None
                    if value is not None:
                        values.append(value)
                return percentile(values, .5) if values else None
            def fmt(v):
                return '-' if v is None else f'{v:.1f}'
            rate = med('presented_drop_rate')
            clean_render = med('render_ms', 'p95', clean) if clean else None
            cells.append(f"{fmt(None if rate is None else rate * 100)} {fmt(med('presented_dropped'))} {fmt(med('presented_long_gaps'))}/{fmt(med('presented_max_gap_ms'))} {fmt(med('presentation_interval_ms', 'p95'))} {fmt(med('presentation_interval_ms', 'p99'))} {fmt(med('interval_ms', 'p95'))} {fmt(med('interval_ms', 'p99'))} {fmt(med('longest_gap_ms'))} {fmt(med('first_frame_ms'))} {fmt(med('content_commit_ms'))} {fmt(med('render_ms', 'p95'))}/{fmt(clean_render)} {fmt(med('reversal', 'delivery_to_visible_ms'))}".ljust(100))
        print('/'.join(key).ljust(26) + ''.join(cells))


def rescore(directory, arm):
    """animation-pacing.py rescore DIR ARM: recompute the commit metrics of every window of an earlier run from its saved
    shell.log (after a change of the metric). Windows, render times, memory and errors are kept."""
    base = Path(directory) / arm
    data = json.loads((base / 'pacing.json').read_text())
    rows = parse_trace(base / 'shell.log', data['real_ref'])
    for window in data['windows']:
        fresh = window_metrics(rows, window['start'], window['end'], [])
        for stale in ('surface', 'commits', 'active_ms', 'first_frame_ms', 'interval_ms', 'longest_gap_ms', 'housekeeping_commits',
                      'dropped', 'slow_intervals', 'drop_rate', 'presented_interval_ms'):
            window.pop(stale, None)
        window.update(fresh)
    (base / 'pacing.json').write_text(json.dumps(data, indent=1))


m.Arm.drive = pace_drive
m.build_gallery = lambda args, names: summarize(args, names) and {}
if __name__ == '__main__':
    if sys.argv[1:2] == ['report']:
        report(sys.argv[2:])
        sys.exit(0)
    if sys.argv[1:2] == ['rescore']:
        rescore(sys.argv[2], sys.argv[3])
        sys.exit(0)
    selected = os.environ.get('PACING_ANIMS', '').split(',')
    if any(item.startswith(('scroll-keybindings-', 'scroll-welcome-shortcuts-')) for item in selected):
        if '--shortcut-fixture' not in sys.argv:
            sys.argv.append('--shortcut-fixture')
    if '--states' not in sys.argv:
        sys.argv += ['--states', 'pace']
    sys.exit(m.main())
