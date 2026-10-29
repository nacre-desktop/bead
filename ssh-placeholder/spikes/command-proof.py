#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Command ingress against a private real bar; no live paths or bindings changed."""
import importlib.util,json,os,pathlib,subprocess
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell';socket=pathlib.Path(env['XDG_RUNTIME_DIR'])/'commands.sock';log=raw/'commands.log'
    command=[str(binary),'--bar','--renderer','cpu','--command-socket',str(socket),'--namespace','bead-command-proof']
    if compositor=='hyprland':command+=['--output','bead-bench']
    with log.open('w') as f:
        client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            matrix.wait_log(log,lambda t:'UI_PRESENTED generation=1' in t)
            def call(target,method,*args,ok=True):
                r=subprocess.run([str(binary),'ipc','--socket',str(socket),'--json',target,method,*args],env=env,cwd=ROOT,capture_output=True,text=True,timeout=5)
                assert (r.returncode==0)==ok,(target,method,r.stderr)
                return json.loads(r.stdout) if r.stdout else None
            assert call('bead','ping')['result']['version']==1
            assert call('bead','state')['result']['surfaces']==1
            state=json.loads(call('nacre','state')['result']);assert state['launcherMode']=='apps' and state['launcher'] is False
            assert call('notifs','counts',ok=False)['error']['code']=='unavailable'
            assert call('nacre','previewDashboard','wrong-type',ok=False) is None
            assert socket.stat().st_mode&0o777==0o600
        finally:stop(client)
    assert client.returncode==0 and 'STOP surfaces=0' in log.read_text() and not socket.exists()
    print('COMMAND_PROOF '+compositor+' private_native_bar=true typed_cli=true state_contract=true unavailable_errors=true owner_socket_cleanup=true',flush=True)
