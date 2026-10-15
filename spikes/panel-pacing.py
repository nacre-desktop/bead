#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""IPC-only panel pacing; reuses measure.py containment, no input injection."""
import importlib.util
import json
import os
import pathlib
import re
import signal
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('matrix', ROOT/'spikes/bar-matrix.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


def sample(env, raw, compositor, stop):
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS': env['DBUS_SESSION_BUS_ADDRESS'],
           'PIPEWIRE_REMOTE': 'bead-private-pw', 'BEAD_PRIVATE_SERVICES_TEST': '1'}
    binary = os.environ.get('BEAD_NATIVE_BINARY', str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    sock = pathlib.Path(env['XDG_RUNTIME_DIR'])/'panel.sock'
    log = raw/'panel.log'
    output = os.environ.get('BEAD_PROOF_OUTPUT') or ('bead-bench' if compositor == 'hyprland' else None)
    command = [binary, '--bar', '--audio-panel', '--command-socket', str(sock),
               '--namespace', 'bead-panel-pacing', '--trace-state']
    if output: command += ['--output', output]
    if os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') == '1' or os.environ.get('BEAD_PACING_PASSIVE') == '1': command += ['--test-passive']
    client = pw = None
    result = {'compositor': compositor, 'cycles': []}

    def call(method):
        args = ['sound'] if method == 'open' else []
        p = subprocess.run([binary, 'ipc', '--socket', str(sock), '--json',
                            'bead.panel', method, *args], env=env, cwd=ROOT,
                           capture_output=True, text=True, check=True, timeout=5)
        return json.loads(p.stdout)['result']

    def segment():
        return matrix.log_text(log)[offset:]

    def stamps():
        return [int(n) for n in re.findall(r'PANEL_PRESENTED monotonic_ns=(\d+)', segment())]

    def intervals(values):
        return [(b-a)/1e6 for a,b in zip(values, values[1:])]

    def interrupted(signum, frame):
        raise KeyboardInterrupt()

    previous = signal.signal(signal.SIGTERM, interrupted)
    pidfile = pathlib.Path(os.environ['BEAD_TEST_PID_FILE']) if os.environ.get('BEAD_TEST_PID_FILE') else None
    if pidfile:
        with pidfile.open('x') as f: f.write(str(os.getpid()))
    try:
        pw = matrix.services.pipewire(env, raw)
        with log.open('w') as f:
            client = subprocess.Popen(command, env=env, cwd=ROOT, stdout=f,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            (raw/'owned-pids.json').write_text(json.dumps({'client':client.pid, 'pipewire':pw.pid}))
            print(f'PANEL_TEST_OWNED client={client.pid} pipewire={pw.pid}', flush=True)
            matrix.wait_log(log, lambda t: 'UI_PRESENTED generation=1' in t)
            assert 'PRESENTATION clock_id=1' in matrix.log_text(log), 'CLOCK_MONOTONIC required'
            time.sleep(2)
            result['bar'] = matrix.idle(client.pid, float(os.environ.get('BEAD_PACING_IDLE_SECONDS', '2')))
            for i in range(int(os.environ.get('BEAD_PACING_CYCLES', '20'))):
                offset = len(matrix.log_text(log)); started = time.monotonic_ns()
                call('open')
                matrix.wait_log(log, lambda _: re.search(r'UI_PRESENTED generation=1 monotonic_ns=(\d+) refresh_ns=(\d+) companion=true',segment()))
                ready = re.search(r'UI_PRESENTED generation=1 monotonic_ns=(\d+) refresh_ns=(\d+) companion=true',segment())
                time.sleep(.4)
                opening = stamps()
                assert len(opening) >= 3, 'missing opening presentation feedback'
                warm = matrix.idle(client.pid, .01)
                offset = len(matrix.log_text(log))
                call('close'); time.sleep(.4)
                closing = stamps()
                assert len(closing) >= 3, 'missing closing presentation feedback'
                matrix.wait_log(log, lambda _: not call('state')['resident'], timeout=8)
                row = {'cycle':i, 'ready_ms':(int(ready[1])-started)/1e6,
                       'refresh_ns':int(ready[2]), 'opening_ms':intervals(opening),
                       'closing_ms':intervals(closing), 'warm':warm}
                result['cycles'].append(row)
                (raw/'panel-pacing.json').write_text(json.dumps(result,indent=2)+'\n')
                print('PANEL_TEST_CYCLE '+json.dumps({'cycle':i,'ready_ms':row['ready_ms'],
                       'open':matrix.percentiles(row['opening_ms']), 'close':matrix.percentiles(row['closing_ms'])}),flush=True)
            result['closed'] = matrix.idle(client.pid, float(os.environ.get('BEAD_PACING_IDLE_SECONDS', '2')))
            for phase in ['opening','closing']:
                values = [v for row in result['cycles'] for v in row[phase+'_ms']]
                result[phase] = {**matrix.percentiles(values), 'longest':max(values), 'n':len(values)}
            result['ready'] = matrix.percentiles([r['ready_ms'] for r in result['cycles']])
            result['gate_c_physical_pacing'] = all(result[p]['p99'] <= 16.7 for p in ['opening','closing'])
            (raw/'panel-pacing.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PANEL_TEST_RESULT '+json.dumps({k:result[k] for k in ['opening','closing','ready','gate_c_physical_pacing']}),flush=True)
    finally:
        if client: stop(client)
        if pw: stop(pw)
        if pidfile: pidfile.unlink(missing_ok=True)
        signal.signal(signal.SIGTERM, previous)
        print('PANEL_TEST_CLEANED',flush=True)
