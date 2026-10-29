#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0137: real edge dwell/lip/body/exit trials in parity-compare's guarded private sessions."""
import importlib.util
import json
import os
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__


_spawn = m.Arm.spawn

def spawn(self, command, *args, **kwargs):
    if os.environ.get('BEAD_HOVER_TRACE') and '--bar' in command and Path(command[0]).name == 'nacre-shell':
        command = [*command, '--trace-state']
        if os.environ.get('BEAD_HOVER_WAYLAND_TRACE'):
            kwargs['env'] = dict(kwargs.get('env') or self.env, WAYLAND_DEBUG='client')
    return _spawn(self, command, *args, **kwargs)

m.Arm.spawn = spawn


def drive(self, _names):
    self.errors = []
    width, height = self.geometry['logical']
    shots = self.out / 'shots'
    shots.mkdir(exist_ok=True)
    rows = []
    observed = {}
    trials = int(os.environ.get("BEAD_HOVER_TRIALS", "20"))
    assert 1 <= trials <= 20

    def state(panel, require_frame=False):
        if self.name == 'bead':
            path = self.runtime / ('app-chat.sock' if panel == 'sidebar' else 'bead-command.sock')
            if not path.exists():
                return False
            value = self.socket_call(path, 'bead.panel', 'state', [])['result']
            observed[panel] = {key: value.get('stores', {}).get(key) for key in
                               ['panel.open', 'panel.hover_opened', 'panel.anchor_hovered', 'bead.focused']}
            return value['target_open'] and (not require_frame or
                (value['prepared'] and value['surfaces'] > 0 and value['size'] is not None))
        value = self.dispatch('nacre', 'state')
        if isinstance(value, str):
            value = json.loads(value)
        return value['left' if panel == 'sidebar' else 'osd']

    def shot(name):
        self.grab(shots / (name + '.png'))

    self.pointer(width // 2, height - 100)
    shot('idle')
    for panel, edge, body in [('sidebar', (3, height // 2), (200, height // 2)),
                              ('control', (width - 3, height // 2), (width - 200, height // 2))]:
        if panel == 'control':
            body = m.PTR['control-tile'][self.name]
        if panel not in os.environ.get('BEAD_HOVER_PANELS', 'sidebar,control').split(','):
            continue
        for trial in range(trials):
            if trial == 0:
                self.sequence(panel + '-open', lambda: self.pointer(*edge, settle=.01), 1.0)
            else:
                self.pointer(*edge, settle=.8)
            try:
                m.wait_until(lambda: state(panel, True), 10, panel + ' hover-open')
            except Exception:
                shot('failure')
                (self.out / 'failure-layers.json').write_text(self.run(['niri', 'msg', '-j', 'layers']).stdout)
                raise
            start = time.monotonic()
            # Hold the lip, then cross into the body for at least five seconds.
            while time.monotonic() - start < 1.0:
                assert state(panel, True), f'{panel} trial {trial}: closed on lip {observed.get(panel)}'
                time.sleep(.1)
            if trial == 0:
                shot(panel + '-lip')
            self.pointer(*body, settle=.05)
            body_start = time.monotonic()
            while time.monotonic() - body_start < 5.1:
                assert state(panel, True), f'{panel} trial {trial}: closed in body {observed.get(panel)}'
                time.sleep(.1)
            if trial == 0:
                shot(panel + '-body')
            held = time.monotonic() - start
            body_held = time.monotonic() - body_start
            self.pointer(width // 2, height - 100, settle=.01)
            left = time.monotonic()
            m.wait_until(lambda: not state(panel), 3, panel + ' close after exit')
            rows.append(dict(panel=panel, trial=trial + 1, held_seconds=held, body_seconds=body_held,
                             close_seconds=time.monotonic() - left))
            time.sleep(.25)
        shot(panel + '-closed')
        if panel == 'sidebar' and self.name == 'bead':
            self.pointer(*edge, settle=.8)
            m.wait_until(lambda: state(panel, True), 10, 'focus proof open')
            self.pointer(*body, mode='click')
            for _ in range(32):
                focused = self.socket_call(self.runtime / 'app-chat.sock', 'bead.panel', 'state', [])['result']
                if focused['stores'].get('bead.focused') == 'chat.composer':
                    break
                self.run(['wtype', '-k', 'Tab'])
                time.sleep(.1)
            else:
                raise AssertionError('keyboard focus missed composer: ' + str(focused['stores'].get('bead.focused')))
            self.pointer(width // 2, height - 100)
            focus_start = time.monotonic()
            while time.monotonic() - focus_start < 5.1:
                assert state(panel), 'focused composer closed outside'
                time.sleep(.1)
            shot('sidebar-focused-outside')
            self.socket_call(self.runtime / 'app-chat.sock', 'bead.panel', 'close', [])
            time.sleep(.6)
        self.pointer(*edge, mode='press', settle=.05)
        shot(panel + '-pressed')
        self.release_pointer()
        self.pointer(width // 2, height - 100)
        if self.name == 'bead' and panel == 'sidebar':
            self.socket_call(self.runtime / 'app-chat.sock', 'bead.panel', 'close', [])
        else:
            self.dispatch('nacre', 'close')
        time.sleep(.6)
    (self.out / 'hover-trials.json').write_text(json.dumps(rows, indent=2) + '\n')


m.Arm.drive = drive
if __name__ == '__main__':
    raise SystemExit(m.main())
