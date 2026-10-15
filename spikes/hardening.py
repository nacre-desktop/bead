#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Architecture proofs inside measure.py's existing private compositor isolation."""
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('matrix', ROOT/'spikes/bar-matrix.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


def sample(env, raw, compositor, stop):
    if os.environ.get('BEAD_HARDENING') == 'panel-reload':
        spec=importlib.util.spec_from_file_location('panel_reload_proof',ROOT/'spikes/panel-reload-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'diagnostics':
        spec=importlib.util.spec_from_file_location('diagnostics_proof',ROOT/'spikes/diagnostics-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'panel-reference':
        spec=importlib.util.spec_from_file_location('panel_reference',ROOT/'spikes/panel-reference.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'panel':
        spec=importlib.util.spec_from_file_location('panel_proof',ROOT/'spikes/panel-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'commands':
        spec=importlib.util.spec_from_file_location('command_proof',ROOT/'spikes/command-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'restart':
        spec=importlib.util.spec_from_file_location('restart_proof',ROOT/'spikes/restart-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'kit':
        spec=importlib.util.spec_from_file_location('kit_proof',ROOT/'spikes/kit-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') == 'policy':
        spec=importlib.util.spec_from_file_location('policy_proof',ROOT/'spikes/policy-proof.py')
        proof=importlib.util.module_from_spec(spec);spec.loader.exec_module(proof)
        return proof.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_HARDENING') in ['layers', 'stall']:
        spec = importlib.util.spec_from_file_location('layer_proof', ROOT/'spikes/layer-proof.py')
        proof = importlib.util.module_from_spec(spec); spec.loader.exec_module(proof)
        return proof.sample(env, raw, compositor, stop)
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS': env['DBUS_SESSION_BUS_ADDRESS'],
           'PIPEWIRE_REMOTE': 'bead-private-unavailable'}
    binary = pathlib.Path(os.environ.get('BEAD_HARDENING_BINARY',
                          str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell')))
    if os.environ.get('BEAD_GL_DEBUG'):
        env['WAYLAND_DEBUG'] = '1'
    rows = []
    deadline = time.monotonic()+45 if os.environ.get('BEAD_PACING_ONLY') else None
    for renderer in os.environ.get('BEAD_PACING_CASES', 'cpu,femtovg-gl').split(','):
        label = compositor+'-'+renderer
        command = [str(binary), '--bar', '--bar-panel', '--renderer', renderer,
                   '--namespace', 'bead-hardening']
        if os.environ.get('BEAD_TEST_PASSIVE'):
            command += ['--test-passive']
        if os.environ.get('BEAD_PROOF_OUTPUT'):
            command += ['--output', os.environ['BEAD_PROOF_OUTPUT']]
        if os.environ.get('BEAD_PANEL_RENDERER'):
            command += ['--panel-renderer', os.environ['BEAD_PANEL_RENDERER']]
        if compositor == 'hyprland':
            command += ['--output', 'bead-bench']
        idle = None
        if not os.environ.get('BEAD_PACING_ONLY'):
            path = raw/(label+'-idle.log')
            with path.open('w') as log:
                client = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log,
                                          stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    matrix.wait_log(path, lambda t: 'UI_COMMIT' in t and 'READY output=' in t)
                    time.sleep(3)
                    idle = matrix.idle(client.pid, float(os.environ.get('BEAD_IDLE_SECONDS', '60')))
                    (raw/(label+'-idle.json')).write_text(json.dumps(idle, indent=2)+'\n')
                finally:
                    stop(client)
            assert 'STOP surfaces=0' in path.read_text(), path.read_text()
        path = raw/(label+'-pacing.log')
        seconds = 10
        with path.open('w') as log:
            client = subprocess.Popen(command+['--animate', '--seconds', str(seconds)],
                                      cwd=ROOT, env=env, stdout=log,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            if os.environ.get('BEAD_PACING_ONLY'):
                (raw/'active.pid').write_text(str(client.pid)+'\n')
                print('PASSIVE_PROOF_CLIENT pid='+str(client.pid), flush=True)
            try:
                client.wait(timeout=min(25, max(.1, deadline-time.monotonic())) if deadline else 25)
            finally:
                stop(client)
                if deadline:
                    (raw/'active.pid').unlink(missing_ok=True)
        assert client.returncode == 0, path.read_text()
        metrics = []
        for line in path.read_text().splitlines():
            if line.startswith('METRICS '):
                metrics.append(dict(re.findall(r'(\w+)=([^ ]+)', line)))
        assert len(metrics) == 2, metrics
        for metric in metrics:
            # Nested niri's presentation protocol reports refresh_ns=0; use
            # the output rate configured and checked by measure.py instead.
            hz = float(os.environ.get('BEAD_PROOF_HZ', '120' if compositor == 'hyprland' else '60'))
            assert int(metric['frames']) <= seconds*hz+3, metric
        row = {'label': label, 'idle': idle, 'pacing_seconds': seconds, 'surfaces': metrics}
        rows.append(row)
        print('HARDENING_PACING '+json.dumps(row), flush=True)
    (raw/'pacing.json').write_text(json.dumps({'binary': str(binary), 'rows': rows}, indent=2)+'\n')
