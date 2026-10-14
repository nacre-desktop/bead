#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Layer and stall workloads; all containment comes from measure.py."""
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('matrix', ROOT/'spikes/bar-matrix.py')
matrix = importlib.util.module_from_spec(spec); spec.loader.exec_module(matrix)


def sample(env, raw, compositor, stop):
    env = {**env, 'BEAD_PRIVATE_INPUT_PROOF': '1', 'BEAD_MATCHED_ICONS': '1',
           'BEAD_ICON_PATH': str(ROOT/'shell/sample.svg')}
    if os.environ.get('BEAD_POINTER_TRACE'): env['WAYLAND_DEBUG'] = '1'
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release'
    binary = pathlib.Path(os.environ.get('BEAD_HARDENING_BINARY', str(target/'nacre-shell')))
    rows = []
    cases = ['stall'] if os.environ.get('BEAD_HARDENING') == 'stall' else os.environ.get('BEAD_LAYER_CASES', 'typing,scroll,progress').split(',')
    with tempfile.TemporaryDirectory(prefix='bead-layers-') as directory:
        bundle = pathlib.Path(directory); shutil.copytree(ROOT/'shell', bundle/'shell')
        grid = bundle/'shell/ui/components/app-grid.luau'
        grid.write_text(grid.read_text().replace('ui.Grid{min_cell_width=110,', 'ui.Grid{shrink=0,min_cell_width=110,'))
        catalog = bundle/'catalog.json'
        catalog.write_text(json.dumps([{'id': f'proof-{i:04}', 'name': f'Application {i:04}', 'icon': 'fixture'} for i in range(48 if cases == ['hover'] else 240)]))
        for case in cases:
            path = raw/(case+'.log'); pointer = None; client = None
            command = [str(binary), '--renderer', 'femtovg-gl', '--namespace', 'bead-layer-proof']
            if compositor == 'hyprland': command += ['--output', 'bead-bench']
            if case == 'progress':
                command += ['--debug-progress']
            else:
                command += ['--ui', 'ui/main.luau', '--catalog', str(catalog), '--backend', 'none', '--trace-state', '--test-keyboard', '--debug-continuous']
            if case == 'stall': command += ['--animate', '--debug-luau-stall-ms', '600', '--seconds', '9']
            try:
                with path.open('w') as log:
                    client = subprocess.Popen(command, cwd=bundle, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                    matrix.wait_log(path, lambda t: 'READY output=' in t and (case == 'progress' or 'UI_PRESENTED generation=1' in t), timeout=30)
                    if case == 'stall':
                        client.wait(timeout=20)
                        text = path.read_text()
                        begin = int(re.search(r'DEBUG_SCRIPT_STALL begin_ns=(\d+)', text)[1])
                        end = int(re.search(r'DEBUG_SCRIPT_STALL end_ns=(\d+)', text)[1])
                        committed = [int(n) for n in re.findall(r'DEBUG_FRAME monotonic_ns=(\d+)', text)]
                        presented = [int(n) for n in re.findall(r'DEBUG_PRESENTED monotonic_ns=(\d+)', text)]
                        nearby = [n for n in presented if begin-100_000_000 <= n <= end+100_000_000]
                        row = {'case': case, 'workload_log_offset': offset, 'stall_ms': (end-begin)/1e6,
                               'committed_during_stall': sum(begin <= n <= end for n in committed),
                               'presented_during_stall': sum(begin <= n <= end for n in presented),
                               'longest_period_ms': max((b-a)/1e6 for a,b in zip(nearby, nearby[1:]))}
                    else:
                        time.sleep(2)
                        if compositor == 'hyprland':
                            subprocess.run(['hyprctl', 'eval', 'hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'], env=env, check=True, capture_output=True, timeout=5)
                        if case in ['typing', 'scroll', 'hover']:
                            pointer = matrix.services.Probe([str(target/'examples/private-pointer')], env, raw/(case+'-pointer.log'))
                            pointer.wait('POINTER_READY')
                            time.sleep(.3)
                            pointer.command('move 20 20', 'POINTER_DONE')
                            time.sleep(.1)
                            pointer.command('move 1000 300', 'POINTER_DONE')
                            time.sleep(.3)
                            if case == 'typing':
                                subprocess.run(['wtype', '-s', '300', '-M', 'shift', '-k', 'Tab', '-m', 'shift', '-s', '300'], env=env, check=True, timeout=5)
                        if case == 'typing':
                            matrix.wait_log(path, lambda t: any(json.loads(l[9:]).get('bead.focused', '').endswith('.search') for l in t.splitlines() if l.startswith('UI_STATE ')))
                        if os.environ.get('BEAD_LAYER_CAPTURE'):
                            subprocess.run(['grim', str(raw/(case+'.png'))], env=env, capture_output=True, check=True, timeout=8)
                        offset = len(matrix.log_text(path))
                        started = time.monotonic(); before = matrix.measure.ticks(client.pid)
                        operations = 0
                        while time.monotonic()-started < 8:
                            if case == 'typing':
                                subprocess.run(['wtype', '-M', 'ctrl', '-k', 'a', '-m', 'ctrl', '-k', 'BackSpace', 'App'], env=env, check=True, timeout=5)
                            elif case == 'hover':
                                pointer.command('move '+('20 20' if operations%2 else '1000 300'), 'POINTER_DONE')
                            elif case == 'scroll':
                                pointer.command('scroll '+('40' if operations % 80 < 40 else '-40'), 'POINTER_DONE')
                            time.sleep(.15 if case == 'scroll' else .03 if case != 'progress' else .1)
                            operations += 1
                        elapsed = time.monotonic()-started
                        row = {'case': case, 'operations': operations, 'seconds': elapsed,
                               'cpu_one_core_percent': 100*(matrix.measure.ticks(client.pid)-before)/os.sysconf('SC_CLK_TCK')/elapsed,
                               'memory': matrix.measure.memory(client.pid), 'drm_mib': matrix.measure.drm_memory(client.pid)}
            finally:
                if pointer: pointer.close()
                if client: stop(client)
            text = path.read_text()
            assert 'STOP surfaces=0' in text and 'UI_ERROR' not in text, text[-1200:]
            row['metrics'] = [dict(re.findall(r'(\w+)=([^ ]+)', l)) for l in text.splitlines() if l.startswith('METRICS ')]
            row['prepare_ms'] = matrix.percentiles([float(n) for n in re.findall(r'UI_COMMIT .*prepare_ms=([\d.]+)', text)])
            damage = [float(v) for v in re.findall(r'DAMAGE .*share=([\d.]+)', text[row.get('workload_log_offset', 0):])]
            row['damage_nonzero_share'] = matrix.percentiles([v for v in damage if v>0])
            row['upload_pixels'] = sum(int(v) for v in re.findall(r'stage=layer_upload .*pixels=(\d+)', text))
            row['bakes'] = len(re.findall(r'stage=layer_bake', text))
            row['uploads'] = len(re.findall(r'stage=layer_upload', text))
            states = [json.loads(l[9:]) for l in text.splitlines() if l.startswith('UI_STATE ')]
            if case == 'typing': assert any(s.get('launcher.search.text') == 'App' for s in states)
            if case == 'scroll': assert max(s.get('launcher.scroll.y', 0) for s in states) > 0
            rows.append(row)
            (raw/'layer-proof.json').write_text(json.dumps({'compositor': compositor, 'rows': rows}, indent=2)+'\n')
            print('LAYER_PROOF '+json.dumps(row), flush=True)
