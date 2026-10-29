#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Interactive client of the existing nested/private measure.py session."""
import importlib.util
import json
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import re
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


matrix = load('preview_matrix', ROOT / 'spikes/bar-matrix.py')
clocks = load('preview_clocks', ROOT / 'spikes/process-clock.py')


# `nacrectl call --timeout 2000` can legitimately take connect 2 s + handshake 5 s + call 2 s; the
# subprocess limit must exceed that, or a slow host crashes the preview with a bare TimeoutExpired (B0088).
CALL_PROCESS_LIMIT = 12
CALL_DEADLINE = 60


def private_call(nacrectl, env, method, params):
    """One private-daemon call; every failure is a RuntimeError the callers retry until their deadline."""
    started = time.monotonic()
    try:
        reply = subprocess.run([nacrectl, 'call', '--timeout', '2000', method, json.dumps(params)],
                               env=env, capture_output=True, text=True, timeout=CALL_PROCESS_LIMIT)
    except subprocess.TimeoutExpired:
        raise RuntimeError('private daemon call %s did not finish in %ds (slow or overloaded host)'
                           % (method, CALL_PROCESS_LIMIT)) from None
    if reply.returncode:
        raise RuntimeError('private daemon call failed: %s after %.1fs: %s'
                           % (method, time.monotonic() - started, reply.stderr.strip()[:200]))
    return json.loads(reply.stdout)


def display_alive(path):
    """False once the nested compositor's Wayland socket is gone or refuses connections (the compositor exited,
    for instance Alt+Shift+Q). A zombie child keeps no listener, so this is what the preview watches."""
    probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    probe.settimeout(.2)
    try:
        probe.connect(str(path))
        return True
    except (FileNotFoundError, ConnectionRefusedError):
        return False
    except OSError:
        return True  # busy or slow is not gone
    finally:
        probe.close()


TOUR = [('bar', None, None), ('launcher', 'nacre', ['launcher', '']), ('dashboard', 'drawers', ['toggle', 'dashboard']),
        ('settings', 'nacre', ['settings']), ('sidebar', 'nacre', ['left']), ('session', 'nacre', ['menu', 'power', ''])]

# Keep this list aligned with the core-view PanelSpecs registered in main.rs.
# This smoke opens each actual view entry through the shell's panel IPC/runtime.
PANEL_ROUTE_VIEWS = [
    'launcher', 'sound', 'calendar', 'peek-calendar',
    'overview', 'palette', 'keybindings', 'wallpaper', 'session',
    'network', 'bluetooth', 'battery', 'peek-network', 'peek-bluetooth',
    'peek-battery', 'peek-audio', 'updates', 'peek-updates',
    'dashboard-today', 'dashboard-weather', 'dashboard-media', 'dashboard-system',
    'dashboard-notifications', 'dashboard-devices', 'dashboard-network',
    'dashboard-clipboard', 'notifications', 'control', 'peek-notifications',
    'pearl-guide',
]


def sample_full(env, raw, compositor, stop):
    """The whole shell as `bead-shell start` runs it (one process, one UI provider, in-process notices)
    plus a standalone daemon, in a private nested session. Nothing here reaches a live session."""
    binary = Path(os.environ['BEAD_HARDENING_BINARY']).resolve()
    helper = Path(os.environ['BEAD_PREVIEW_DAEMON']).resolve()
    runtime = Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime != Path(f'/run/user/{os.getuid()}'), 'live runtime refused'
    assert Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime), 'live UI display refused'
    assert str(Path(f'/run/user/{os.getuid()}/bus')) not in env['DBUS_SESSION_BUS_ADDRESS'], 'live bus refused'
    assert Path(env['NIRI_SOCKET']).resolve().is_relative_to(runtime)
    daemon = bar = None
    directory = tempfile.TemporaryDirectory(prefix="bead-preview-ui-")
    try:
        root = Path(directory.name)
        shutil.copytree(Path(os.environ.get('BEAD_PREVIEW_SHELL', str(ROOT / 'shell'))), root / 'shell')
        for name in ['home', 'data', 'config', 'state/nacre', 'cache']:
            (root / name).mkdir(mode=0o700, parents=True)
        (runtime / 'nacre').mkdir(mode=0o700, exist_ok=True)
        (root / 'state/nacre/shell/notifications/images').mkdir(mode=0o700, parents=True)
        endpoint = runtime / 'nacre/daemon.sock'  # the key binds of the private niri use this path
        commands = runtime / 'bead-command.sock'
        private = dict(env, HOME=str(root / 'home'), XDG_DATA_HOME=str(root / 'data'),
                       XDG_CONFIG_HOME=str(root / 'config'), XDG_STATE_HOME=str(root / 'state'),
                       XDG_CACHE_HOME=str(root / 'cache'), NACRE_DAEMON_SOCKET=str(endpoint),
                       NACRE_COMPOSITOR=compositor, NACRE_NIRI_HOVER_STREAM='0',
                       DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],
                       PIPEWIRE_REMOTE='bead-private-unavailable', PULSE_SERVER='unix:' + str(root / 'no-pulse'))
        tools = os.environ.get('BEAD_PREVIEW_TOOLS')
        if tools:
            private['NACRE_HELPER_TOOLS'] = tools
        presentation = os.environ.get('BEAD_PREVIEW_PRESENTATION')
        if presentation:  # read-only copy of one record; the image it names is only read
            shutil.copyfile(presentation, root / 'state/nacre/presentation.json')
            (root / 'state/nacre/presentation.json').chmod(0o600)
        daemon = subprocess.Popen([str(helper), 'daemon', '--notifications=daemon', '--behaviours=daemon'], cwd=root, env=private,
                                  stdout=(raw / 'preview-daemon.log').open('w'), stderr=subprocess.STDOUT,
                                  start_new_session=True)

        def call(method, params):
            return private_call(os.environ['BEAD_PREVIEW_NACRECTL'], private, method, params)

        def wait_call(method, params):
            deadline = time.monotonic() + CALL_DEADLINE
            last = ''
            while time.monotonic() < deadline:
                if daemon.poll() is not None:
                    raise RuntimeError('private daemon exited; inspect preview log')
                try:
                    return call(method, params)
                except RuntimeError as error:
                    last = str(error)
                    time.sleep(.1)
            raise RuntimeError('private daemon deadline: %s (%ds; last error: %s)' % (method, CALL_DEADLINE, last))

        def notify(summary, body, expire=8000):
            subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.freedesktop.Notifications',
                            '--object-path', '/org/freedesktop/Notifications',
                            '--method', 'org.freedesktop.Notifications.Notify', 'Bead preview', '0', '',
                            summary, body, '[]', '{}', str(expire)], env=private, capture_output=True, timeout=5)

        wait_call('daemon.status', {})
        # The exact production flags of packaging/bin/bead-shell, plus --notices (in-process notices/OSD).
        flags = ['--bar', '--core-views', '--ui-provider', '--notices', '--daemon-socket', str(endpoint),
                 '--command-socket', str(commands)]
        if os.environ.get('BEAD_PREVIEW_DASHBOARD_FIXTURES') == '1':
            flags += ['--dashboard-fixtures', str(root / 'shell/tests/fixtures/dashboard')]
        if os.environ.get('BEAD_PREVIEW_CPU') == '1':
            flags += ['--renderer', 'cpu', '--panel-renderer', 'cpu']
        bar = subprocess.Popen([str(binary)] + flags, cwd=root, env=private,
                               stdout=(raw / 'preview-bar.log').open('w'), stderr=subprocess.STDOUT,
                               start_new_session=True)
        wait_call('ui.dispatch', {'target': 'nacre', 'method': 'state', 'args': []})
        matrix.wait_log(raw / 'preview-bar.log', lambda t: 'READY output=' in t)
        time.sleep(1)
        notify('Bead preview', 'Notifications are shown by the shell itself (B0030).')
        shots = os.environ.get('BEAD_PREVIEW_SHOTS')
        if shots:
            out = Path(shots)
            out.mkdir(parents=True, exist_ok=True)

            def grab(name):
                subprocess.run(['grim', str(out / (name + '.png'))], env=private, check=True, timeout=20)

            def dispatch(target, args):
                try:
                    call('ui.dispatch', {'target': target[0], 'method': target[1], 'args': args})
                except RuntimeError:
                    print('PREVIEW_TOUR dispatch failed: ' + target[1], flush=True)
            time.sleep(2)
            grab('bar')
            notify('Bead preview', 'Screenshot notification.')
            time.sleep(2)
            grab('notification')
            time.sleep(7)
            for name, target, args in TOUR[1:]:
                dispatch((target, args[0]), args[1:])
                time.sleep(3)
                grab(name)
                dispatch(('nacre', 'close'), []) if name != 'settings' else subprocess.run(
                    [str(binary), 'ipc', '--socket', str(commands), '--json', 'bead.window', 'close'], cwd=root,
                    env=private, capture_output=True, timeout=5)
                time.sleep(1.5)
            # The exact command lines of the private niri's key binds (a key press cannot be injected
            # into the nested niri without a focused parent client); the key names are checked by `niri validate`.
            import re as _re
            binds = dict(_re.findall(r'Alt\+(\w+) repeat=false \{ spawn-sh ("(?:[^"\\]|\\.)*")',
                                     Path(os.environ['BEAD_PREVIEW_NIRI_CONFIG']).read_text()))
            for key, name in [('Space', 'keybind-launcher'), ('O', 'keybind-dashboard')]:
                code = subprocess.run(['sh', '-c', json.loads(binds[key])], env=env, capture_output=True, timeout=10).returncode
                print('PREVIEW_KEYS Alt+%s rc=%s' % (key, code), flush=True)
                time.sleep(3)
                grab(name)
                dispatch(('nacre', 'close'), [])
                time.sleep(1.5)
            print('PREVIEW_TOUR_DONE ' + str(out), flush=True)
        else:
            print('PREVIEW_READY full=true', flush=True)
            tick = 0
            while bar.poll() is None and daemon.poll() is None:
                time.sleep(.1)
                tick += 1
                if tick % 5 == 0 and not display_alive(env['WAYLAND_DISPLAY']):
                    print('PREVIEW_COMPOSITOR_GONE tearing down the shell and daemon', flush=True)
                    break
    finally:
        if bar is not None:
            stop(bar)
        clocks.stop_owned_tree(daemon, stop)
        directory.cleanup()
        print('PREVIEW_STOPPED owned_UI_and_daemon_workers_stopped=true', flush=True)


def sample(env, raw, compositor, stop):
    if os.environ.get('BEAD_PREVIEW_FULL') == '1':
        return sample_full(env, raw, compositor, stop)
    binary = Path(os.environ['BEAD_HARDENING_BINARY']).resolve()
    helper = Path(os.environ['BEAD_PREVIEW_DAEMON']).resolve()
    runtime = Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime != Path(f'/run/user/{os.getuid()}'), 'live runtime refused'
    assert env['DBUS_SESSION_BUS_ADDRESS'] and 'BEAD_PARENT_WAYLAND' in env
    assert Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime), 'live UI display refused'
    assert str(Path(f'/run/user/{os.getuid()}/bus')) not in env['DBUS_SESSION_BUS_ADDRESS'], 'live bus refused'
    if compositor == 'niri':
        assert Path(env['NIRI_SOCKET']).resolve().is_relative_to(runtime)
    else:
        assert (runtime / 'hypr' / env['HYPRLAND_INSTANCE_SIGNATURE'] / '.socket.sock').is_socket()
    daemon = bar = notices = None
    ready = False
    directory = tempfile.TemporaryDirectory(prefix="bead-preview-ui-")
    try:
        root = Path(directory.name)
        shutil.copytree(Path(os.environ.get('BEAD_PREVIEW_SHELL', str(ROOT / 'shell'))), root / 'shell')
        for name in ['home', 'data', 'config', 'state', 'bar-state', 'notice-state', 'cache', 'runtime', 'bin']:
            (root / name).mkdir(mode=0o700)
        endpoint = root / 'runtime/nacre/daemon.sock'
        commands = root / 'runtime/bar.sock'
        notice_commands = root / 'runtime/notices.sock'
        notice_bus = root / 'runtime/notification-bus.sock'
        (root / 'bin/nacrectl').symlink_to(helper)
        private = dict(env, HOME=str(root / 'home'), XDG_DATA_HOME=str(root / 'data'),
                       XDG_CONFIG_HOME=str(root / 'config'), XDG_STATE_HOME=str(root / 'state'),
                       XDG_CACHE_HOME=str(root / 'cache'), NACRE_DAEMON_SOCKET=str(endpoint),
                       NACRE_COMPOSITOR=compositor, NACRE_NIRI_HOVER_STREAM='0',
                       DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],
                       PIPEWIRE_REMOTE='bead-private-unavailable',
                       PIPEWIRE_RUNTIME_DIR=str(root / 'runtime'),
                       PULSE_SERVER='unix:' + str(root / 'runtime/no-pulse'),
                       BEAD_PRIVATE_SERVICES_TEST='1', BEAD_PRIVATE_INPUT_PROOF='1')
        if os.environ.get('BEAD_PREVIEW_SMOKE') == '1':
            private['BEAD_PROFILE_INPUT'] = 'clocks'
        private.pop('NACRE_QS_CONFIG', None)
        bar_env=dict(private,XDG_STATE_HOME=str(root/'bar-state'))
        notice_env=dict(private,XDG_STATE_HOME=str(root/'notice-state'))
        daemon = subprocess.Popen([str(helper), 'daemon'], cwd=root, env=private,
                                  stdout=(raw / 'preview-daemon.log').open('w'), stderr=subprocess.STDOUT,
                                  start_new_session=True)

        def call(method, params):
            return private_call(str(root / 'bin/nacrectl'), private, method, params)

        def wait_call(method, params):
            deadline = time.monotonic() + CALL_DEADLINE
            last = ''
            while time.monotonic() < deadline:
                if daemon.poll() is not None:
                    raise RuntimeError('private daemon exited; inspect preview log')
                try:
                    return call(method, params)
                except RuntimeError as error:
                    last = str(error)
                    time.sleep(.05)
            raise RuntimeError('private daemon/UI registration deadline: %s (%ds; last error: %s)' % (method, CALL_DEADLINE, last))

        wait_call('daemon.status', {})
        output_args=['--output','bead-bench'] if compositor=='hyprland' and os.environ.get('BEAD_PREVIEW_SMOKE')=='1' else []
        notices = subprocess.Popen([str(binary), '--notification-bus', str(notice_bus),
                                    '--notification-history', str(root / 'notice-state/history.json'),
                                    '--command-socket', str(notice_commands), '--renderer', 'cpu',
                                    '--namespace', 'bead-preview-notices']+output_args, cwd=root, env=notice_env,
                                   stdout=(raw / 'preview-notices.log').open('w'), stderr=subprocess.STDOUT,
                                   start_new_session=True)
        dashboard_fixtures = os.environ.get('BEAD_PREVIEW_DASHBOARD_FIXTURES') == '1'
        panel_route_smoke = os.environ.get('BEAD_PREVIEW_SMOKE') == '1'
        dashboard_args = ['--dashboard-fixtures', str(root / 'shell/tests/fixtures/dashboard')] if dashboard_fixtures else []
        bar = subprocess.Popen([str(binary), '--bar', '--core-views', '--renderer', 'cpu',
                                '--panel-renderer', 'cpu', '--daemon-socket', str(endpoint),
                                '--command-socket', str(commands), '--ui-provider',
                                '--ui-notification-socket', str(notice_commands),
                                '--namespace', 'bead-preview-bar']+dashboard_args+output_args+(['--trace-state'] if os.environ.get('BEAD_PREVIEW_RECONNECT')=='1' or (dashboard_fixtures and os.environ.get('BEAD_PREVIEW_SMOKE')=='1') else []), cwd=root, env=bar_env,
                               stdout=(raw / 'preview-bar.log').open('w'), stderr=subprocess.STDOUT,
                               start_new_session=True)
        if not panel_route_smoke:
            state = wait_call('ui.dispatch', {'target': 'nacre', 'method': 'state', 'args': []})
            if isinstance(state, str):
                state = json.loads(state)
            assert {'calendar', 'sound-output', 'network-status', 'bluetooth-status', 'battery-status'} <= set(state['bead']['available']), 'preview needs the matching core-views binary and bundle'
            wait_call('ui.dispatch', {'target': 'notifs', 'method': 'counts', 'args': []})
        matrix.wait_log(raw/'preview-bar.log',lambda t:'READY output=' in t)
        matrix.wait_log(raw/'preview-notices.log',lambda t:'READY output=' in t)
        def prepared_and_presented(text):
            ready=re.search(r'MENU_RESULT ok=true catalog_rev=[1-9]\d* count=\d+',text)
            if not ready:return False
            tail=text[ready.end():]
            commit=re.search(r'UI_COMMIT generation=(\d+)[^\n]* update=(\d+) companion=true source_error=false',tail)
            return bool(commit and re.search(r'UI_PRESENTED generation='+commit[1]+r' [^\n]*companion=true update='+commit[2]+r'\b',tail[commit.end():]))
        if os.environ.get('BEAD_PREVIEW_RECONNECT') == '1':
            old_boot = call('daemon.status', {})['boot']
            clocks.stop_owned_tree(daemon, stop)
            assert daemon.returncode == 0
            offset = len(matrix.log_text(raw/'preview-bar.log'))
            offline = time.monotonic()
            matrix.wait_log(raw/'preview-bar.log', lambda t:'"bar.reconnecting":true' in t[offset:], timeout=5)
            delay = time.monotonic()-offline
            assert delay >= 1.8, 'reconnecting mark appeared before the two-second delay'
            # Joining the state to presentation proves the bar rendered the mark.
            def state_presented(text, value):
                state=re.search(r'UI_STATE [^\n]*"bar.reconnecting":'+value+r'[^\n]*',text)
                if not state:return False
                tail=text[state.end():]
                commit=re.search(r'UI_COMMIT generation=(\d+)[^\n]* update=(\d+) companion=false source_error=false',tail)
                return bool(commit and re.search(r'UI_PRESENTED generation='+commit[1]+r' [^\n]*companion=false update='+commit[2]+r'\b',tail[commit.end():]))
            matrix.wait_log(raw/'preview-bar.log', lambda t:state_presented(t[offset:], 'true'))
            restarted = time.monotonic()
            offset = len(matrix.log_text(raw/'preview-bar.log'))
            daemon = subprocess.Popen([str(helper),'daemon'],cwd=root,env=private,
                stdout=(raw/'preview-daemon-restarted.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
            new_boot = wait_call('daemon.status', {})['boot']
            assert old_boot != new_boot
            wait_call('ui.dispatch', {'target':'nacre','method':'state','args':[]})
            matrix.wait_log(raw/'preview-bar.log',lambda t:state_presented(t[offset:], 'false'))
            fresh_upper = time.monotonic()-restarted
            assert fresh_upper <= 1, 'bar connection recovery exceeded one second from daemon launch'
            (raw/'preview-reconnect.json').write_text(json.dumps({'valid':True,'old_boot':old_boot,'new_boot':new_boot,
                'mark_observed_seconds_after_stop':delay,'bar_recovered_seconds_after_launch_upper_bound':fresh_upper,
                'ui_provider_reregistered':True,'presented_mark_and_recovery':True},indent=2)+'\n')
        if not panel_route_smoke:
            offset=len(matrix.log_text(raw/'preview-bar.log'))
            call('ui.dispatch', {'target': 'nacre', 'method': 'launcher', 'args': ['']})
            if os.environ.get('BEAD_PREVIEW_SMOKE')=='1':
                matrix.wait_log(raw/'preview-bar.log',lambda t:prepared_and_presented(t[offset:]))
            else:
                matrix.wait_log(raw/'preview-bar.log',lambda t:re.search(r'UI_PRESENTED generation=[1-9]\d* [^\n]*companion=true',t[offset:])is not None)
        if not panel_route_smoke:
            notify = subprocess.run(['gdbus', 'call', '--address', 'unix:path=' + str(notice_bus),
                                     '--dest', 'org.freedesktop.Notifications', '--object-path', '/org/freedesktop/Notifications',
                                     '--method', 'org.freedesktop.Notifications.Notify', "'Bead preview'", '0', "''",
                                     "'Foundation preview'", "'This notification stays inside the nested window.'", '[]', '{}', '8000'],
                                    env=private, capture_output=True, text=True, timeout=4)
            if notify.returncode:
                raise RuntimeError('private notification injection failed')
            counts = wait_call('ui.dispatch', {'target': 'notifs', 'method': 'counts', 'args': []})
            if isinstance(counts, str):
                counts = json.loads(counts)
            assert counts['retained'] >= 1
        assert 'UI_ERROR' not in (raw/'preview-bar.log').read_text(), 'bar reported a UI error'
        assert 'UI_ERROR' not in (raw/'preview-notices.log').read_text(), 'notices reported a UI error'
        if dashboard_fixtures and os.environ.get('BEAD_PREVIEW_SMOKE') == '1' and not panel_route_smoke:
            def page_call(method, *args):
                result=subprocess.run([str(binary),'ipc','--socket',str(commands),'--json','bead.panel',method,*args],cwd=root,env=bar_env,capture_output=True,text=True,timeout=5)
                if result.returncode:raise RuntimeError('private dashboard panel IPC failed: '+method)
            for page in ['weather', 'media', 'system']:
                offset=len(matrix.log_text(raw/'preview-bar.log'))
                page_call('open','dashboard-'+page)
                def page_presented(text):
                    for state in re.finditer(r'^UI_STATE (.+)$',text,re.M):
                        values=json.loads(state[1])
                        if values.get('dashboard.page')!=page or values.get('panel.open') is not True:continue
                        tail=text[state.end():]
                        for commit in re.finditer(r'UI_COMMIT generation=(\d+)[^\n]* update=(\d+) companion=true source_error=false',tail):
                            if re.search(r'UI_PRESENTED generation='+commit[1]+r' [^\n]*companion=true update='+commit[2]+r'\b',tail[commit.end():]):return True
                    return False
                matrix.wait_log(raw/'preview-bar.log',lambda t:page_presented(t[offset:]))
            page_call('close')
        if os.environ.get('BEAD_PREVIEW_SMOKE') == '1':
            route_log = raw / 'preview-bar.log'

            def panel_call(method, *args):
                result = subprocess.run(
                    [str(binary), 'ipc', '--socket', str(commands), '--json',
                     'bead.panel', method, *args],
                    cwd=root, env=bar_env, capture_output=True, text=True, timeout=8)
                if result.returncode:
                    raise RuntimeError(f'panel route {method} {args} failed: {result.stderr.strip()}')
                return json.loads(result.stdout)

            def panel_state():
                return panel_call('state')['result']

            def wait_panel(predicate, label):
                deadline = time.monotonic() + 12
                last = None
                while time.monotonic() < deadline:
                    last = panel_state()
                    if predicate(last):
                        return last
                    time.sleep(.05)
                raise RuntimeError(f'panel route {label} state timeout: {last}')

            def assert_clean_ui(route):
                text = route_log.read_text()
                errors = [line for line in text.splitlines()
                          if line.startswith('UI_ERROR ') or 'unknown state ' in line]
                if errors:
                    raise AssertionError(f'{route}: shell reported UI errors: {errors[-8:]}')

            initial_panel = panel_state()
            if initial_panel.get('target_open'):
                panel_call('close')
            wait_panel(lambda state: not state.get('target_open') and not state.get('resident'),
                       'initial panel retirement')

            def wait_presented(offset, route):
                matrix.wait_log(
                    route_log,
                    lambda text: re.search(
                        r'UI_PRESENTED generation=\d+ [^\n]*companion=true',
                        text[offset:]) is not None,
                    timeout=20)
                assert_clean_ui(route)

            results = []
            for route in PANEL_ROUTE_VIEWS:
                offset = len(matrix.log_text(route_log))
                panel_call('open', route)
                wait_panel(lambda state: state.get('target_open') and state.get('resident'), route)
                wait_presented(offset, route)
                panel_call('close')
                wait_panel(lambda state: not state.get('target_open'), route + ' close')
                results.append({'route': route, 'opened': True, 'closed': True})
                assert_clean_ui(route)

            # Wait for the closed Runtime to retire, then create a fresh session
            # against the same unchanged private daemon payload and open it again.
            panel_call('open', 'network')
            wait_panel(lambda state: state.get('target_open') and state.get('resident'), 'network warmup')
            panel_call('close')
            wait_panel(lambda state: not state.get('target_open'), 'network close')
            wait_panel(lambda state: not state.get('resident'), 'network session retirement')
            offset = len(matrix.log_text(route_log))
            panel_call('open', 'network')
            wait_panel(lambda state: state.get('target_open') and state.get('resident'),
                       'network fresh session after identical source data')
            wait_presented(offset, 'network fresh session')
            panel_call('close')
            wait_panel(lambda state: not state.get('target_open'), 'network fresh close')
            assert_clean_ui('fresh session after identical source data')
            results.append({'route': 'network', 'fresh_session_after_unchanged_source': True})
            assert 'UI_ERROR' not in (raw / 'preview-notices.log').read_text()
            (raw / 'panel-route-smoke.json').write_text(json.dumps({
                'actual_shell_tree': True,
                'routes': results,
                'source_updates_injected_between_sessions': 0,
                'ui_errors': 0,
                'unknown_state_errors': 0,
            }, indent=2) + '\n')
            print('PANEL_ROUTE_SMOKE ' + json.dumps({'routes': len(results), 'ui_errors': 0,
                                                    'unknown_state_errors': 0}), flush=True)
        ready = True
        if os.environ.get('BEAD_PREVIEW_SMOKE') == '1':
            masks=[]
            for task in Path(f'/proc/{bar.pid}/task').iterdir():
                fields=dict(line.split(':',1) for line in (task/'status').read_text().splitlines() if ':' in line)
                masks.append({'name':fields['Name'].strip(),'blocked':fields['SigBlk'].strip()})
            (raw/'preview-thread-masks.json').write_text(json.dumps(masks,indent=2)+'\n')
        print('PREVIEW_READY core_views=true daemon_menu=true daemon_ui_controls=true private_notification_host=bead no_input_injection=true', flush=True)
        smoke = os.environ.get('BEAD_PREVIEW_SMOKE') == '1'
        if smoke:
            if not panel_route_smoke:
                offset=len(matrix.log_text(raw/'preview-bar.log'))
                call('ui.dispatch', {'target': 'nacre', 'method': 'launcher', 'args': ['a']})
                matrix.wait_log(raw/'preview-bar.log',lambda t:prepared_and_presented(t[offset:]))
                call('ui.dispatch', {'target': 'notifs', 'method': 'clear', 'args': []})
                call('ui.dispatch', {'target': 'nacre', 'method': 'close', 'args': []})
                time.sleep(3)
        else:
            while bar.poll() is None and notices.poll() is None and daemon.poll() is None:
                time.sleep(.1)
    finally:
        # A compositor killed during teardown can leave a stale socket file.
        # Probe only the already-validated private display, never the parent.
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as probe:
            probe.settimeout(.05)
            try:
                probe.connect(env['WAYLAND_DISPLAY'])
                display_gone=False
            except (FileNotFoundError,ConnectionRefusedError):
                display_gone=True
            except TimeoutError:
                display_gone=False
        if display_gone:
            for process in [bar,notices]:
                if process is not None:
                    try:process.wait(timeout=3)
                    except subprocess.TimeoutExpired:pass
        if bar is not None:
            stop(bar)
        if notices is not None:
            stop(notices)
        clocks.stop_owned_tree(daemon, stop)
        directory.cleanup()
        (raw/'preview-stop.json').write_text(json.dumps({'bar':bar.returncode if bar else None,'notices':notices.returncode if notices else None,'daemon':daemon.returncode if daemon else None},indent=2)+'\n')
        if ready:
            if os.environ.get('BEAD_PREVIEW_SMOKE')=='1' or not display_gone:
                assert bar.returncode==0 and 'STOP surfaces=0' in (raw/'preview-bar.log').read_text(),'bar did not shut down cleanly'
                assert notices.returncode==0 and 'STOP surfaces=0' in (raw/'preview-notices.log').read_text(),'notices did not shut down cleanly'
            else:
                # Closing the nested window disconnects its clients. A display
                # error (1) is expected here; signals/aborts are still failures.
                assert bar.returncode in [0,1] and notices.returncode in [0,1]
        print('PREVIEW_STOPPED owned_UI_and_daemon_workers_stopped=true', flush=True)
