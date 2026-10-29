#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run the real 30-second numeric-only diagnostic and retain closed-view samples."""
import importlib.util,json,os,pathlib,stat,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py');matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    report=raw/'report.json';report.unlink(missing_ok=True)
    socket=pathlib.Path(env['XDG_RUNTIME_DIR'])/'report.sock';log=raw/'report.log'
    command=[str(binary),'--bar','--audio-panel','--command-socket',str(socket),'--report',str(report),'--seconds','1']
    if compositor=='hyprland':command+=['--output','bead-bench']
    with log.open('w') as f:
        client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            matrix.wait_log(log,lambda t:'UI_PRESENTED generation=1' in t)
            def call(method):subprocess.run([str(binary),'ipc','--socket',str(socket),'bead.panel',method,'sound'] if method=='open' else [str(binary),'ipc','--socket',str(socket),'bead.panel',method],env=env,cwd=ROOT,check=True,capture_output=True,timeout=5)
            call('open');time.sleep(3);call('close');time.sleep(3);call('open')
            client.wait(timeout=35)
            assert client.returncode==0
            value=json.loads(report.read_text())
            assert 29.5<=value['duration_seconds']<35, value['duration_seconds']
            assert stat.S_IMODE(report.stat().st_mode)==0o600
            assert set(value)=={'version','duration_seconds','cpu_seconds','surfaces'}
            for surface in value['surfaces']:
                assert set(surface)=={'panel','frames','processing_ms','presentation_ms','callback_ms'}
                assert all(isinstance(n,(int,float)) for key in ['processing_ms','presentation_ms','callback_ms'] for n in surface[key])
            assert sum(s['panel'] for s in value['surfaces'])==2, 'closed panel history lost'
            print('REPORT_PROOF '+json.dumps({'seconds':value['duration_seconds'],'cpu_seconds':value['cpu_seconds'],'surface_lifetimes':len(value['surfaces']),'numeric_only':True,'mode':'0600'}),flush=True)
        finally:stop(client)
