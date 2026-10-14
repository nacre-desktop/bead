#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Restart only the private compositor, keeping an IPC subscriber alive.

The private stable alias models socket replacement at the configured endpoint.
Real session socket names/environment are not discovered or rewritten.
"""
import json
import pathlib
import subprocess
import threading
import time


def sample(env, launch_env, command, compositor, target, raw, stop):
    runtime = pathlib.Path(env['XDG_RUNTIME_DIR'])
    name = 'niri' if 'NIRI_SOCKET' in env else 'hyprland'
    private_env = env.copy()
    if name == 'niri':
        alias = runtime / 'bead-restart.sock'
        alias.symlink_to(env['NIRI_SOCKET'])
        private_env['NIRI_SOCKET'] = str(alias)
    else:
        alias = runtime / 'hypr' / 'bead-restart'
        alias.symlink_to(runtime / 'hypr' / env['HYPRLAND_INSTANCE_SIGNATURE'])
        private_env['HYPRLAND_INSTANCE_SIGNATURE'] = alias.name
    probe = subprocess.Popen([str(target / 'nacre-shell'), '--backend-probe'], env=private_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1, start_new_session=True)
    snapshots = []
    results = []
    condition = threading.Condition()

    def drain():
        with (raw / f'{name}-restart.log').open('w') as log:
            for line in probe.stdout:
                log.write(line)
                log.flush()
                if line.startswith('BACKEND_SNAPSHOT '):
                    with condition:
                        snapshots.append(json.loads(line.split(' ', 1)[1]))
                        condition.notify_all()
                elif line.startswith('BACKEND_RESULT '):
                    with condition:
                        results.append(json.loads(line.split(' ', 1)[1]))
                        condition.notify_all()

    reader = threading.Thread(target=drain)
    reader.start()
    replacement = None

    def wait(predicate, offset=0):
        deadline = time.monotonic() + 20
        with condition:
            while True:
                for snapshot in snapshots[offset:]:
                    if predicate(snapshot):
                        return snapshot
                if probe.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Private compositor restart timeout; inspect restart log')
                condition.wait(.05)

    try:
        old = wait(lambda s: s['connected'])
        offset = len(snapshots)
        stop(compositor)
        wait(lambda s: not s['connected'], offset)
        with (raw / f'{name}-restarted-compositor.log').open('w') as log:
            replacement = subprocess.Popen(command, env=launch_env, stdout=log, stderr=log, start_new_session=True)
        deadline = time.monotonic() + 20
        while True:
            candidates = list(runtime.glob('niri.*.sock')) if name == 'niri' else [p.parent for p in runtime.glob('hypr/*/.socket.sock') if not p.parent.is_symlink()]
            candidates = [p for p in candidates if p != alias and p.exists()]
            if len(candidates) == 1:
                alias.unlink()
                alias.symlink_to(candidates[0])
                break
            if replacement.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Replacement private compositor socket not found')
            time.sleep(.05)
        new = wait(lambda s: s['connected'] and s['epoch'] != old['epoch'], offset)
        assert not {w['id'] for w in old['workspaces']} & {w['id'] for w in new['workspaces']}
        assert old['workspaces'], 'Restart proof requires an existing old workspace'
        probe.stdin.write(json.dumps({'action':'switch-workspace','workspace':old['workspaces'][0]['id']})+'\n')
        probe.stdin.flush()
        deadline = time.monotonic() + 5
        with condition:
            while not results:
                if time.monotonic() > deadline: raise RuntimeError('Stale-ID action timeout')
                condition.wait(.05)
        assert not results[-1]['ok'] and 'unknown/stale ID' in results[-1]['error']
        print(f'BACKEND_RESTART {name} real-compositor-exit/relaunch reconnect=true stale-IDs-rejected=true endpoint=private-stable-alias', flush=True)
        return replacement
    except BaseException:
        if replacement is not None:
            stop(replacement)
        raise
    finally:
        if probe.poll() is None:
            probe.stdin.write('stop\n')
            probe.stdin.flush()
        try:
            probe.wait(timeout=5)
        except subprocess.TimeoutExpired:
            stop(probe)
        reader.join(timeout=5)
        alias.unlink(missing_ok=True)
