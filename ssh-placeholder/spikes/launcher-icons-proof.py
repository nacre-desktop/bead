#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0129: first-frame icons, real private menu engine, and panel-expiry proof.

Uses parity-compare.py's guarded private session and its CLI. Run one arm per
desktop-heavy.lock acquisition; alternate control/candidate at least five times.
The frame count uses committed model + baked image readiness, not screenshot
polling. ICON_PARITY=1 uses the original screenshot driver for paired visual
evidence (--states idle,launcher,launcher-hover,launcher-pressed --arms qml,bead).
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import time

os.environ.update(BEAD_PROFILE_INPUT='clocks', BEAD_VERBOSE_LOG='1', BEAD_FRAME_TRACE='1')
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__
original_daemon = m.Arm.start_daemon
original_spawn = m.Arm.spawn


def spawn(self, command, name, env=None, cwd=None):
    if name == 'shell' and self.name == 'bead':
        command = [*command, '--trace-state']
    return original_spawn(self, command, name, env=env, cwd=cwd)


def daemon(self):
    # Identical private applications for QML/control/candidate; no owner catalog.
    from PIL import Image
    data = self.root / 'data'
    apps = data / 'applications'
    icons = data / 'icons'
    apps.mkdir(parents=True, exist_ok=True)
    icons.mkdir(parents=True, exist_ok=True)
    for index in range(64):
        icon = icons / f'fixture-{index}.{"svg" if index % 2 else "png"}'
        color = (40 + index * 3, 190 - index, 80 + index * 2)
        if index % 2:
            icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect x="2" y="2" width="44" height="44" rx="9" fill="rgb{color}"/></svg>')
        else:
            Image.new('RGBA', (128, 128), (*color, 255)).save(icon)
        (apps / f'b0129-{index:03}.desktop').write_text(
            f'[Desktop Entry]\nType=Application\nName=AAA Icon {index:03}\nExec=true\nIcon={icon}\nCategories=Utility;\n')
    config = self.root / 'config/nacre'
    config.mkdir(parents=True, exist_ok=True)
    (config / 'launcher.json').write_text(json.dumps({
        'version': 1, 'favorites': [f'b0129-{i:03}' for i in range(12)],
        'recent': [f'b0129-{i:03}' for i in range(20, 28)], 'hidden': [], 'usage': {}}))
    return original_daemon(self)


def fields(line):
    return dict(re.findall(r'(\w+)=([^\s]+)', line))


def score(text, trigger_ns):
    state = {}
    baked = {}
    ready = False
    frames = []
    presentations = []
    panel_stamps = []
    presented_updates = {}
    ready_updates = {}
    commits = []
    for line in text.splitlines():
        if line.startswith('UI_STATE '):
            state = json.loads(line.removeprefix('UI_STATE '))
        elif line.startswith('UI_IMAGE_BYTES '):
            baked = fields(line)
        elif line.startswith('UI_COMMIT ') and 'companion=true' in line:
            f = fields(line)
            # A successful bake is synchronous; a populated model with no missing
            # image rasters is ready before this scene can reach the presenter.
            ready = (state.get('launcher.result_count', 0) > 0
                     and not state.get('launcher.pending', True)
                     and int(baked.get('images', 0)) > 0
                     and int(baked.get('missing', 0)) == 0)
            if 'update' in f:
                ready_updates[f['update']] = ready
            if int(f['monotonic_ns']) >= trigger_ns:
                commits.append({'ms': (int(f['monotonic_ns']) - trigger_ns) / 1e6,
                                'ready': ready, 'rows': state.get('launcher.result_count'),
                                'images': baked.get('images'), 'missing': baked.get('missing')})
        elif line.startswith('UI_FRAME ') and 'companion=true' in line:
            f = fields(line)
            stamp = int(f['monotonic_ns'])
            if stamp >= trigger_ns:
                frames.append({'ms': (stamp - trigger_ns) / 1e6, 'ready': ready})
        elif line.startswith('UI_PRESENTED ') and 'companion=true' in line:
            f = fields(line)
            stamp = int(f['monotonic_ns'])
            presented_updates[stamp] = f.get('update')
            if stamp >= trigger_ns:
                presentations.append({'ms': (stamp - trigger_ns) / 1e6,
                                      'ready': ready_updates.get(f.get('update'), False)})
        elif line.startswith('PANEL_PRESENTED '):
            panel_stamps.append(int(fields(line)['monotonic_ns']))
    if panel_stamps:
        # UI_PRESENTED logs only a change of scene/update. PANEL_PRESENTED logs
        # every animation presentation, before UI_PRESENTED on a changed update.
        presentations = []
        update = None
        for stamp in panel_stamps:
            update = presented_updates.get(stamp, update)
            if stamp >= trigger_ns:
                presentations.append({'ms': (stamp - trigger_ns) / 1e6,
                                      'ready': ready_updates.get(update, False)})
    return {'commits': commits, 'frames': len(frames),
            'missing_icon_frames': sum(not frame['ready'] for frame in frames),
            'presented_frames': len(presentations),
            'missing_icon_presentations': sum(not frame['ready'] for frame in presentations),
            'first_presented_ms': presentations[0]['ms'] if presentations else None,
            'first_all_icons_presented_ms': next((frame['ms'] for frame in presentations if frame['ready']), None),
            'first_frame_ms': frames[0]['ms'] if frames else None,
            'first_all_icons_ms': next((frame['ms'] for frame in frames if frame['ready']), None)}


def drive(self, names):
    log = self.logs / 'shell.log'
    result = {'backend': 'cpu' if self.args.cpu else 'default', 'geometry': self.geometry,
              'memory_kib': {'idle': self.rss()}, 'phases': {}}
    self.errors = []
    self.dispatch_trace = []
    self.state_start = time.monotonic()
    if self.name == 'qml' or os.environ.get('ICON_PARITY') == '1':
        # Use the established parity driver, with the same generated apps/icons.
        return original_drive(self, names)
    for phase in ('cold', 'warm', 'expired'):
        if phase == 'expired':
            # The production close timer is 120 s; allow its animation and timer
            # delivery to finish. This is intentionally not a shortened test TTL.
            time.sleep(123)
            result['memory_kib']['expired_closed'] = self.rss()
        warm_events = [line for line in log.read_text(errors='replace').splitlines()
                       if line.startswith('ICON_WARM ')]
        trigger = time.monotonic_ns()
        self.steps([('ui', 'nacre', 'launcher', '')])
        time.sleep(1.1)
        text = log.read_text(errors='replace')
        result['phases'][phase] = score(text, trigger)
        result['phases'][phase]['trigger_ns'] = trigger
        result['phases'][phase]['sample_end_ns'] = time.monotonic_ns()
        result['phases'][phase]['prewarm_before_open'] = fields(warm_events[-1]) if warm_events else None
        result['memory_kib'][phase + '_open'] = self.rss()
        if phase == 'cold':
            key_offset = len(log.read_text(errors='replace'))
            self.run(['wtype', 'a'])
            time.sleep(.5)
            key_text = log.read_text(errors='replace')[key_offset:]
            received = re.search(r'INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)', key_text)
            updates = re.findall(r'UI_COMMIT .*update=(\d+).*source_error=false', key_text)
            presented = re.findall(r'UI_PRESENTED generation=\d+ monotonic_ns=(\d+) refresh_ns=\d+ companion=\w+ update=(\d+) cpu_ns=(\d+)', key_text)
            match = next((row for row in presented if received and row[1] in updates and int(row[0]) >= int(received[1])), None)
            result['first_key_cpu_ms'] = (int(match[2]) - int(received[2])) / 1e6 if match else None
            self.run(['wtype', '-k', 'BackSpace'])
            time.sleep(.3)
        self.steps([('ui', 'nacre', 'close')])
        time.sleep(.8)
        result['memory_kib'][phase + '_closed'] = self.rss()
    (self.out / 'icon-proof.json').write_text(json.dumps(result, indent=2))
    (self.out / 'shell-full.log').write_bytes(log.read_bytes())
    # Keep numeric traces and the native log; no false PASS on an empty model.
    if any(not row['presented_frames'] or row['first_all_icons_presented_ms'] is None for row in result['phases'].values()):
        raise RuntimeError('missing icon-ready frame; inspect icon-proof.json and shell.log')


original_drive = m.Arm.drive
m.Arm.start_daemon = daemon
m.Arm.spawn = spawn
m.Arm.drive = drive
if os.environ.get('ICON_PARITY') == '1':
    # The Recent column shifts quick controls; target the same fixture app tile.
    for state, action in [('launcher-hover', 'pointer'), ('launcher-pressed', 'press')]:
        cleanup = [('release',)] if action == 'press' else []
        m.STATES[state] = ([('ui', 'nacre', 'launcher', ''), ('sleep', 1.4),
                            (action, 655, 714)], [*cleanup, ('ui', 'nacre', 'close')], 0)
    m.STATES['launcher-search'] = ([('ui', 'nacre', 'launcher', ''),
                                    ('sleep', 1.4), ('type', 'a')],
                                   [('ui', 'nacre', 'close')], 0)
    m.STATE_SETTLE.update({'launcher-hover': .18, 'launcher-pressed': .05,
                          'launcher-search': 1.0})
else:
    m.build_gallery = lambda args, names: {}
if __name__ == '__main__':
    raise SystemExit(m.main())
