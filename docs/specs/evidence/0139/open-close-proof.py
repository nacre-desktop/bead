#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Guarded private Niri proof for repeated control/dashboard open and close."""
import importlib.util
import json
import re
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    'release_proof', HERE / 'spikes/pearl-release-proof.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
m = p.m
m.__file__ = __file__  # capture-safety re-execs must re-enter this wrapper

TRIALS = 20
OPEN_SECONDS = 1.0
CLOSE_SECONDS = 0.3
POLL_SECONDS = 0.1


def window_summary(self):
    windows = json.loads(self.run(['niri', 'msg', '-j', 'windows']).stdout)
    if not isinstance(windows, list):
        raise RuntimeError('niri windows query did not return a list')
    pearl_count = sum(1 for window in windows if window.get('app_id') == 'bead.pearl')
    if pearl_count:
        raise RuntimeError('nested Niri reported a bead.pearl toplevel')
    return {
        'window_count': len(windows),
        'windows_with_id_count': sum('id' in window for window in windows),
        'windows_with_title_count': sum(bool(window.get('title')) for window in windows),
        'pearl_app_id_count': pearl_count,
    }


def panel_state(self):
    state = p.panel_call(self, 'state')
    if not isinstance(state, dict):
        raise RuntimeError('bead.panel state query did not return an object')
    return state


def panel_summary(state):
    size = state.get('size')
    if not (isinstance(size, list) and len(size) == 2
            and all(isinstance(value, (int, float)) for value in size)):
        size = None
    return {'target_open': state.get('target_open'), 'view': state.get('view'), 'size': size}


def valid_open_state(state, expected_view):
    size = state.get('size')
    return (state.get('target_open') is True and state.get('view') == expected_view
            and isinstance(size, list) and len(size) == 2
            and all(isinstance(value, (int, float)) for value in size)
            and min(size) > 100)


def wait_open_state(self, expected_view):
    return m.wait_until(
        lambda: (state if valid_open_state(state := panel_state(self), expected_view) else None),
        12, f'bead.panel open state for {expected_view}')


def wait_closed_state(self):
    return m.wait_until(
        lambda: (state if (state := panel_state(self)).get('target_open') is False else None),
        12, 'bead.panel target_open false')


def observe(self, phase, seconds, expected_view=None):
    deadline = time.monotonic() + seconds
    samples = []
    while True:
        state = panel_state(self)
        if expected_view is not None and not valid_open_state(state, expected_view):
            raise RuntimeError(f'bead.panel left expected open view {expected_view}')
        if expected_view is None and state.get('target_open') is not False:
            raise RuntimeError('bead.panel reopened during close observation')
        panel = panel_summary(state)
        windows = window_summary(self)
        samples.append({'panel': panel, 'windows': windows})
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(POLL_SECONDS, remaining))
    return {'phase': phase, 'duration_seconds': seconds, 'samples': samples}


def drive(self, _names):
    if self.name != 'bead':
        raise RuntimeError('B0139 open/close proof requires --arms bead')
    if tuple(self.args.size) != (2880, 1800) or self.geometry['scale'] != 1.5:
        raise RuntimeError('B0139 open/close proof requires 2880x1800 at scale 1.5')
    if self.args.parent_hz != 120:
        raise RuntimeError('B0139 open/close proof requires the existing 120 Hz labwc parent')
    if not self.args.pointer_tool or not self.args.parent_output_tool:
        raise RuntimeError('provide the existing pointer tool and wlr-randr so the private parent mode is set')

    report = {
        'method': 'private nested Niri; 20 control open/close cycles and 20 dashboard open/close cycles; '
                  'panel state and sanitized Niri window counts sampled during each open and close; '
                  'no pointer input is sent',
        'geometry': self.geometry,
        'parent_hz': self.args.parent_hz,
        'cycles_per_route': TRIALS,
        'timing_seconds': {'open_observation': OPEN_SECONDS, 'close_observation': CLOSE_SECONDS,
                           'poll_interval': POLL_SECONDS},
        'routes': {},
    }
    try:
        for name, expected_view, open_route in (
                ('control', 'control', lambda: self.dispatch('nacre', 'controls', 'home')),
                ('dashboard', 'dashboard-today', lambda: (
                    self.dispatch('drawers', 'toggle', 'dashboard'),
                    self.dispatch('nacre', 'tab', '0')))):
            rows = []
            for cycle in range(1, TRIALS + 1):
                self.dispatch('nacre', 'close')
                baseline_panel = wait_closed_state(self)
                baseline_windows = window_summary(self)
                closed_baseline = {'panel': panel_summary(baseline_panel),
                                   'windows': baseline_windows}
                if baseline_windows['pearl_app_id_count'] != 0:
                    raise RuntimeError(f'{name} cycle {cycle}: pearl present before open')

                open_route()
                opened_state = wait_open_state(self, expected_view)
                opened = observe(self, 'open', OPEN_SECONDS, expected_view)
                if not opened['samples'] or not any(
                        sample['panel']['target_open'] is True for sample in opened['samples']):
                    raise RuntimeError(f'{name} cycle {cycle}: open observation had no target_open sample')
                self.dispatch('nacre', 'close')
                closed_state = wait_closed_state(self)
                closed = observe(self, 'close', CLOSE_SECONDS)
                rows.append({'cycle': cycle, 'closed_baseline': closed_baseline,
                             'opened_state': panel_summary(opened_state),
                             'closed_state': panel_summary(closed_state),
                             'open_observation': opened, 'close_observation': closed})
            report['routes'][name] = rows

        shell_log = (self.logs / 'shell.log').read_text(errors='replace')
        toplevel_requests = len(re.findall(r'\bget_toplevel\s*\(', shell_log))
        report['wayland_protocol'] = {'get_toplevel_request_count': toplevel_requests}
        if toplevel_requests:
            raise RuntimeError(f'WAYLAND_DEBUG shell.log contains {toplevel_requests} get_toplevel requests')
    finally:
        try:
            self.dispatch('nacre', 'close')
        except Exception:
            pass
        (self.out / 'open-close.json').write_text(json.dumps(report, indent=2) + '\n')


m.Arm.spawn = p.spawn
m.Arm.drive = drive
m.build_gallery = lambda *args: {}
sys.exit(m.main())
