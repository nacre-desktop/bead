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
    client_env = env.copy()
    if os.environ.get('BEAD_ALLOCATOR_PRELOAD'): client_env['LD_PRELOAD'] = os.environ['BEAD_ALLOCATOR_PRELOAD']
    if os.environ.get('BEAD_STACK_BYTES'): client_env['RUST_MIN_STACK'] = os.environ['BEAD_STACK_BYTES']
    if os.environ.get('BEAD_ALLOCATOR_ARENAS'): client_env['MALLOC_ARENA_MAX'] = os.environ['BEAD_ALLOCATOR_ARENAS']
    binary = os.environ.get('BEAD_NATIVE_BINARY', str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    sock = pathlib.Path(env['XDG_RUNTIME_DIR'])/'panel.sock'
    log = raw/'panel.log'
    output = os.environ.get('BEAD_PROOF_OUTPUT') or ('bead-bench' if compositor == 'hyprland' else None)
    command = [binary, '--bar', '--audio-panel', '--command-socket', str(sock),
               '--namespace', 'bead-panel-pacing', '--trace-state']
    if os.environ.get('BEAD_PANEL_RENDERER'): command += ['--panel-renderer',os.environ['BEAD_PANEL_RENDERER']]
    if os.environ.get('BEAD_PACING_EXPERIMENT') == '1': env['BEAD_PRIVATE_INPUT_PROOF']='1'; client_env['BEAD_PRIVATE_INPUT_PROOF']='1'
    if output: command += ['--output', output]
    if os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') == '1' or os.environ.get('BEAD_PACING_PASSIVE') == '1': command += ['--test-passive']
    aging = os.environ.get('BEAD_PACING_AGING_ONLY') == '1'
    if aging: assert os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') != '1', 'aging is nested only'
    prefetch = os.environ.get('BEAD_PACING_PREPARE') == '1'
    capture = os.environ.get('BEAD_PACING_CAPTURE') == '1'
    if capture: assert os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') != '1', 'capture is nested only'
    if prefetch:
        assert os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') != '1', 'prefetch proof is nested only'
        env['BEAD_PRIVATE_INPUT_PROOF'] = '1'
    client = pw = pointer = None
    result = {'compositor': compositor, 'memory_aging_only':aging, 'cycles': []}

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
            client = subprocess.Popen(command, env=client_env, cwd=ROOT, stdout=f,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            (raw/'owned-pids.json').write_text(json.dumps({'client':client.pid, 'pipewire':pw.pid}))
            print(f'PANEL_TEST_OWNED client={client.pid} pipewire={pw.pid}', flush=True)
            matrix.wait_log(log, lambda t: 'UI_PRESENTED generation=1' in t)
            assert 'PRESENTATION clock_id=1' in matrix.log_text(log), 'CLOCK_MONOTONIC required'
            time.sleep(2)
            result['bar'] = matrix.idle(client.pid, float(os.environ.get('BEAD_PACING_IDLE_SECONDS', '2')))
            if prefetch:
                pointer = matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')], env, raw/'pointer.log')
                pointer.wait('POINTER_READY')
                region = call('state')['prepare_regions'][0]
                x,y,w,h = region
                def hover():
                    pointer.command('move 20 70', 'POINTER_DONE')
                    time.sleep(.05)
                    pointer.command(f'move {round(x+w/2)} {round(y+h/2)}', 'POINTER_DONE')
                hover()
                matrix.wait_log(log, lambda _: call('state')['prepared'])
                state = call('state')
                assert not state['target_open'] and state['surfaces'] == 0, 'preparation mapped a panel'
                statefile = pathlib.Path(env['XDG_STATE_HOME'])/'bead/panels.json'
                assert not json.loads(statefile.read_text())['values']['panel.open'], 'unused hover persisted open state'
                matrix.wait_log(log, lambda _: not call('state')['resident'],timeout=5)
                time.sleep(.4)
                result['unused_hover'] = matrix.idle(client.pid, float(os.environ.get('BEAD_PACING_IDLE_SECONDS','2')))
                assert result['unused_hover']['pss_plus_drm_mib'] - result['bar']['pss_plus_drm_mib'] <= 5
            soak = float(os.environ.get('BEAD_PACING_SOAK_SECONDS','0'))
            soak_started = time.monotonic()
            for i in range(1000000 if soak else int(os.environ.get('BEAD_PACING_CYCLES', '20'))):
                if prefetch:
                    hover()
                    time.sleep(.15)
                    assert call('state')['prepared'], '150 ms preparation lead did not finish'
                    assert call('state')['surfaces'] == 0

                offset = len(matrix.log_text(log)); started = time.monotonic_ns()
                call('open')
                matrix.wait_log(log, lambda _: re.search(r'UI_PRESENTED generation=1 monotonic_ns=(\d+) refresh_ns=(\d+) companion=true',segment()))
                ready = re.search(r'UI_PRESENTED generation=1 monotonic_ns=(\d+) refresh_ns=(\d+) companion=true',segment())
                time.sleep(.4)
                opening = stamps()
                if not aging: assert len(opening) >= 3, 'missing opening presentation feedback'
                warm = matrix.idle(client.pid, .01)
                bright = None
                if prefetch or capture:
                    from PIL import Image
                    capture = raw/f'prepared-{i}.png'
                    subprocess.run(['grim', str(capture)],env=env,check=True,capture_output=True,timeout=8)
                    picture=Image.open(capture).convert('RGB');scale=picture.width/1920
                    area=picture.crop((round((1920-464+32+20)*scale),round((70+32+160)*scale),round((1920-464+32+372)*scale),round((70+32+190)*scale)))
                    bright=sum(r>140 and g>140 and b>90 for r,g,b in area.get_flattened_data())
                    assert bright>1000*scale*scale, 'prepared panel has no visible volume track'

                if i == 0 and os.environ.get('BEAD_PACING_WARM_SECONDS'):
                    result['warm_idle'] = matrix.idle(client.pid, float(os.environ['BEAD_PACING_WARM_SECONDS']))
                offset = len(matrix.log_text(log))
                call('close'); time.sleep(.4)
                closing = stamps()
                if not aging: assert len(closing) >= 3, 'missing closing presentation feedback'
                matrix.wait_log(log, lambda _: not call('state')['resident'], timeout=8)
                row = {'opening_feedback_count':len(opening),'closing_feedback_count':len(closing),'slider_bright_pixels':bright, 'elapsed_seconds':time.monotonic()-soak_started, 'closed_memory':matrix.measure.memory(client.pid), 'cycle':i, 'ready_ms':(int(ready[1])-started)/1e6,
                       'refresh_ns':int(ready[2]), 'opening_ms':intervals(opening),
                       'closing_ms':intervals(closing), 'warm':warm}
                result['cycles'].append(row)
                (raw/'panel-pacing.json').write_text(json.dumps(result,indent=2)+'\n')
                print('PANEL_TEST_CYCLE '+json.dumps({'cycle':i,'ready_ms':row['ready_ms'],
                       'open':matrix.percentiles(row['opening_ms']), 'close':matrix.percentiles(row['closing_ms'])}),flush=True)
                if soak:
                    if time.monotonic()-soak_started >= soak: break
                    time.sleep(7)
            result['elapsed_use_seconds'] = time.monotonic()-soak_started
            result['closed'] = matrix.idle(client.pid, float(os.environ.get('BEAD_PACING_IDLE_SECONDS', '2')))
            for phase in ['opening','closing']:
                values = [v for row in result['cycles'] for v in row[phase+'_ms']]
                result[phase] = {**matrix.percentiles(values), 'longest':max(values), 'n':len(values)}
            result['ready'] = matrix.percentiles([r['ready_ms'] for r in result['cycles']])
            result['pacing_within_16_7_ms'] = None if aging else all(result[p]['p99'] <= 16.7 for p in ['opening','closing'])
            (raw/'panel-pacing.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PANEL_TEST_RESULT '+json.dumps({k:result[k] for k in ['opening','closing','ready','pacing_within_16_7_ms']}),flush=True)
    finally:
        if pointer: pointer.close()
        if client: stop(client)
        if pw: stop(pw)
        if pidfile: pidfile.unlink(missing_ok=True)
        signal.signal(signal.SIGTERM, previous)
        print('PANEL_TEST_CLEANED',flush=True)
