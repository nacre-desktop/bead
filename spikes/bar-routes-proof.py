# SPDX-License-Identifier: GPL-3.0-only
"""Private nested proof (Hyprland, two outputs) of B0022 daemon bar routes.

Runs the bar against spikes/fake-bar-daemon.py and checks: a workspace click on
each output reaches the daemon with that output's name, the bar survives a daemon
restart without replaying the click made while it was down, and a daemon that
advertises no methods disables the controls. Called through bar-ui.py with
BEAD_BAR_ROUTES_PROOF=1. Memory is not measured here (see per-output-proof.py).
"""
import importlib.util
import json
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import pathlib
import socket as unix_socket
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location('measure', ROOT/'spikes/measure.py')
measure = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(measure)
_spec = importlib.util.spec_from_file_location('services_lifecycle', ROOT/'spikes/services-lifecycle.py')
lifecycle = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lifecycle)


def sample(env, raw, compositor, stop):
    if compositor != 'hyprland':
        raise RuntimeError('bar routes proof needs isolated Hyprland (two private outputs)')
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS': env['DBUS_SESSION_BUS_ADDRESS'],
           'PIPEWIRE_REMOTE': 'bead-private-unavailable', 'BEAD_PRIVATE_SERVICES_TEST': '1',
           'BEAD_PRIVATE_INPUT_PROOF': '1'}
    runtime = pathlib.Path(env['XDG_RUNTIME_DIR'])
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
    binary = pathlib.Path(os.environ.get('BEAD_NATIVE_BINARY', str(target/'release/nacre-shell')))
    command_socket, daemon_socket = runtime/'bar-routes.sock', runtime/'fake-daemon.sock'
    calls_log, client_log = raw/'daemon-calls.jsonl', raw/'bar-routes-client.log'
    calls_log.write_text('')
    client = daemon = pointer = None
    result = {'compositor': compositor, 'binary': str(binary)}

    def hypr(*args):
        return subprocess.run(args, env=env, capture_output=True, text=True, check=True, timeout=8)

    def monitors():
        return json.loads(hypr('hyprctl', '-j', 'monitors').stdout)

    def wait_for(predicate, label, timeout=15):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            value = predicate()
            if value:
                return value
            if client is not None and client.poll() is not None:
                raise RuntimeError(f'bar exited while waiting for {label}: {client.returncode}')
            time.sleep(.1)
        raise RuntimeError(f'timed out waiting for {label}')

    def ipc(method, target_name='bead'):
        out = subprocess.run([str(binary), 'ipc', '--socket', str(command_socket), '--json',
                              target_name, method], cwd=ROOT, env=env, capture_output=True,
                             text=True, check=True, timeout=5)
        return json.loads(out.stdout)['result']

    def regions(output):
        with unix_socket.socket(unix_socket.AF_UNIX, unix_socket.SOCK_STREAM) as stream:
            stream.settimeout(5)
            stream.connect(str(command_socket))
            stream.sendall((json.dumps({'version': 1, 'id': 1, 'target': 'bead', 'method': 'regions',
                                        'args': [], 'output': output})+'\n').encode())
            return json.loads(stream.makefile('rb').readline(65537))['result']['regions']

    def calls():
        return [row for row in map(json.loads, calls_log.read_text().splitlines()) if row['event'] == 'call']

    def start_daemon(boot, *flags):
        return subprocess.Popen([sys.executable, str(ROOT/'spikes/fake-bar-daemon.py'),
                                 str(daemon_socket), str(calls_log), boot, *flags],
                                env=env, start_new_session=True)

    def hellos():
        return sum(1 for line in calls_log.read_text().splitlines() if '"event": "hello"' in line)

    def click_region(output, offset_x, suffix, nth=0):
        found = [r for r in regions(output) if r['id'].endswith(suffix)]
        if len(found) <= nth:
            raise RuntimeError(f'no {suffix} region on {output}: {[r["id"] for r in regions(output)]}')
        x, y, w, h = found[nth]['rect']
        for text in (f'move {round(offset_x+x+w/2)} {round(y+h/2)} 3840 1200', 'click'):
            offset = len(pointer.lines)
            pointer.process.stdin.write(text+'\n')
            pointer.process.stdin.flush()
            pointer.wait('POINTER_DONE', offset=offset)
        time.sleep(.6)
        return found[nth]['id']

    def click_region_ws(output, offset_x):
        return click_region(output, offset_x, '.workspace')

    try:
        names = {m['name'] for m in monitors()}
        if 'bead-bench' not in names:
            raise RuntimeError(f'expected measure.py virtual120 bead-bench output, got {sorted(names)}')
        for name in sorted(names-{'bead-bench'}):
            hypr('hyprctl', 'eval', f'hl.monitor({{output="{name}",disabled=true}})')
        daemon = start_daemon('proof-boot-1')
        wait_for(daemon_socket.exists, 'fake daemon socket')
        args = [str(binary), '--bar', '--core-views', '--renderer', 'cpu',
                '--namespace', 'bead-bar-routes-proof', '--command-socket', str(command_socket),
                '--trace-state', '--daemon-socket', str(daemon_socket), '--ui-provider']
        with client_log.open('w') as stream:
            client = subprocess.Popen(args, cwd=ROOT, env=env, stdout=stream,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            wait_for(lambda: 'UI_COMMIT ' in client_log.read_text(errors='replace'), 'first bar commit')
            hypr('hyprctl', 'output', 'create', 'headless', 'bead-extra')
            hypr('hyprctl', 'eval',
                 'hl.monitor({output="bead-extra",mode="2880x1800@120",position="1920x0",scale=1.5})')
            wait_for(lambda: ipc('state').get('surfaces') == 2, 'two bar surfaces')
            wait_for(hellos, 'daemon handshake')
            pointer = lifecycle.Probe([str(target/'release/examples/private-pointer')],
                                      {**env, 'BEAD_PRIVATE_INPUT_PROOF': '1'}, raw/'bar-routes-pointer.log')
            pointer.wait('POINTER_READY')
            time.sleep(2)
            # Each output's workspace button must call with its own output name.
            clicked = {}
            for output, offset in (('bead-bench', 0), ('bead-extra', 1920)):
                before = len(calls())
                clicked[output] = click_region_ws(output, offset)
                got = wait_for(lambda: calls()[before:], f'workspace call from {output}')
                if (got[0]['m'] != 'workspace.activate' or got[0]['p'].get('output') != output
                        or not isinstance(got[0]['p'].get('workspace'), int)):
                    raise RuntimeError(f'wrong call for {output}: {got[0]}')
            result['per_output_clicks'] = clicked
            # Tray rows come from the daemon topic; a menu item asks for its context menu.
            for nth, (method, item) in enumerate((('tray.activate', 'sni:one'),
                                                  ('tray.context_menu', 'sni:two'))):
                before = len(calls())
                click_region('bead-bench', 0, '.item', nth)
                got = wait_for(lambda: calls()[before:], f'{method} call')
                if got[0]['m'] != method or got[0]['p'].get('item') != item:
                    raise RuntimeError(f'wrong tray call for {item}: {got[0]}')
            result['tray_calls'] = True
            # Daemon down: the bar stays up with its action controls disabled, and
            # nothing is queued for later.
            def controls(output):
                return [r['id'] for r in regions(output) if r['id'].endswith(('.workspace', '.item'))]

            daemon.terminate(); daemon.wait(timeout=5)
            wait_for(lambda: not controls('bead-bench') and not controls('bead-extra'),
                     'controls disabled while the daemon is down')
            if ipc('state').get('surfaces') != 2:
                raise RuntimeError('bar surfaces changed while the daemon was down')
            baseline = len(calls())
            seen = hellos()
            daemon = start_daemon('proof-boot-2')
            wait_for(lambda: hellos() > seen, 'daemon reconnect', timeout=30)
            wait_for(lambda: controls('bead-bench') and controls('bead-extra'),
                     'controls restored after reconnect', timeout=30)
            time.sleep(1)
            if len(calls()) != baseline:
                raise RuntimeError('a pending action was replayed after reconnect')
            click_region_ws('bead-bench', 0)
            wait_for(lambda: len(calls()) == baseline+1, 'workspace call after reconnect')
            result['no_replay_after_restart'] = True
            # Registry without methods: controls are disabled, nothing is sent.
            daemon.terminate(); daemon.wait(timeout=5)
            seen = hellos()
            daemon = start_daemon('proof-boot-3', '--no-methods')
            wait_for(lambda: hellos() > seen, 'method-less daemon', timeout=30)
            wait_for(lambda: not controls('bead-bench') and not controls('bead-extra'),
                     'controls disabled without advertised methods')
            time.sleep(1)
            if len(calls()) != baseline+1:
                raise RuntimeError('a control without an advertised method sent a call')
            result['gated_without_methods'] = True
            log = client_log.read_text(errors='replace')
            if 'UI_ERROR ' in log or 'OUTPUT_SESSION_ERROR ' in log:
                raise RuntimeError('bar logged a UI or session error')
            pointer.close(); pointer = None
            stop(client)
            client = None
        result['clean_stop'] = 'STOP surfaces=0' in client_log.read_text(errors='replace')
        if not result['clean_stop']:
            raise RuntimeError('bar did not report STOP surfaces=0')
        result['status'] = 'done'
        print('BAR_ROUTES_PROOF '+json.dumps(result, sort_keys=True), flush=True)
    except Exception as error:
        result['status'] = 'failed'
        result['reason'] = str(error)
        raise
    finally:
        if pointer is not None:
            measure.stop(pointer.process)
        if client is not None:
            stop(client)
        if daemon is not None and daemon.poll() is None:
            daemon.terminate()
        (raw/'bar-routes-result.json').write_text(json.dumps(result, indent=2, sort_keys=True)+'\n')
