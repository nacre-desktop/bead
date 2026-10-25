#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run the pinned Bead Source against F1 using measure.py's private session."""
import hashlib
import json
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent

def sample(env, raw, compositor, stop):
    helper = pathlib.Path(os.environ['BEAD_NACRE_HELPER']).resolve()
    expected = os.environ.get('BEAD_NACRE_HELPER_SHA256')
    if expected and hashlib.sha256(helper.read_bytes()).hexdigest() != expected:
        raise RuntimeError('prebuilt helper checksum differs')
    private = dict(env)
    private['NACRE_COMPOSITOR'] = compositor
    for key in ['NOTIFY_SOCKET','WATCHDOG_PID','WATCHDOG_USEC','LISTEN_PID','LISTEN_FDS','LISTEN_FDNAMES']:
        private.pop(key, None)
    runtime = pathlib.Path(private['XDG_RUNTIME_DIR'])
    if compositor == 'niri':
        if not pathlib.Path(private['NIRI_SOCKET']).is_relative_to(runtime):
            raise RuntimeError('compositor socket must be private')
    else:
        if not (runtime/'hypr'/private['HYPRLAND_INSTANCE_SIGNATURE']).is_dir():
            raise RuntimeError('compositor instance must be private')
    directory = runtime/'nacre-adapter'
    directory.mkdir(mode=0o700)
    socket = directory/'daemon.sock'
    private['NACRE_DAEMON_SOCKET'] = str(socket)
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    daemon = None
    rows = []
    try:
        for cycle in range(2):
            with (raw/f'daemon-{cycle}.log').open('w') as log:
                daemon = subprocess.Popen([str(helper),'daemon'],cwd=ROOT,env=private,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                deadline = time.monotonic()+10
                while not socket.exists():
                    if daemon.poll() is not None or time.monotonic()>deadline:
                        raise RuntimeError('private daemon did not bind')
                    time.sleep(.02)
                began=time.monotonic()
                result=subprocess.run([str(target),'--check-daemon',str(socket)],cwd=ROOT,env=private,capture_output=True,text=True,timeout=12)
                (raw/f'client-{cycle}.log').write_text(result.stdout+result.stderr)
                if result.returncode:
                    raise RuntimeError('Source client failed; inspect private client log')
                objects=[line for line in result.stdout.splitlines() if line.startswith('{')]
                assert len(objects)==1, 'expected exactly one Source UI probe result'
                data=json.loads(objects[0])
                assert data['connected'] and data['available'] and data['rendered_pixels']>0
                assert data['output_count']>0 and data['workspace_count']>0
                assert data['boot']==data['daemon_boot']
                rows.append({'cycle':cycle,'boot':data['boot'],'source_ui_probe_ms':(time.monotonic()-began)*1000,'outputs':data['output_count'],'workspaces':data['workspace_count'],'rendered_pixels':data['rendered_pixels']})
            stop(daemon)
            assert not socket.exists(), 'daemon socket must clean up on stop'
            daemon=None
        assert rows[0]['boot']!=rows[1]['boot'], 'daemon restart must change boot'
    finally:
        if daemon:
            stop(daemon)
    (raw/'nacre-source-proof.json').write_text(json.dumps({'compositor':compositor,'cycles':rows,'valid':True},indent=2)+'\n')
    print('NACRE_SOURCE_PROOF compositor='+compositor+' cycles=2 ui_render=true peer=true boot_changed=true clean=true',flush=True)
