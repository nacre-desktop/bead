#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Transactional reload and durable state for the actual lazy sound view."""
import importlib.util,json,os,pathlib,re,shutil,signal,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py');matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell';socket=pathlib.Path(env['XDG_RUNTIME_DIR'])/'reload.sock';log=raw/'panel-reload.log'
    with tempfile.TemporaryDirectory(prefix='bead-panel-reload-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        source=bundle/'shell/sound/main.luau';behavior=source.with_name('behavior.luau');original=source.read_text()
        command=[str(binary),'--bar','--audio-panel','--command-socket',str(socket),'--trace-state']
        if compositor=='hyprland':command+=['--output','bead-bench']
        def call(method):
            r=subprocess.run([str(binary),'ipc','--socket',str(socket),'--json','bead.panel',method]+(['sound'] if method=='open' else []),env=env,cwd=bundle,check=True,capture_output=True,text=True,timeout=5)
            return json.loads(r.stdout)['result']
        def wait(marker,offset=0,companion=False):return matrix.wait_log(log,lambda t:any(marker in l and (not companion or 'companion=true' in l) for l in t[offset:].splitlines()))
        def state(offset):
            return next(json.loads(l[9:]) for l in matrix.log_text(log)[offset:].splitlines() if l.startswith('UI_STATE ') and 'panel.page' in l)
        with log.open('w') as f:
            client=subprocess.Popen(command,cwd=bundle,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                wait('UI_PRESENTED generation=1');call('open');wait('UI_PRESENTED generation=1',0,True);time.sleep(.3)
                latencies=[]
                for i in range(5):
                    offset=len(matrix.log_text(log));written=time.monotonic_ns();source.write_text(original.replace('Sound output',f'Sound output {i}'))
                    generation=i+2
                    wait(f'UI_PRESENTED generation={generation}',offset,True)
                    line=next(l for l in matrix.log_text(log)[offset:].splitlines() if l.startswith(f'UI_PRESENTED generation={generation}') and 'companion=true' in l)
                    latencies.append((int(re.search(r'monotonic_ns=(\d+)',line)[1])-written)/1e6)
                    assert state(offset)['panel.page']=='output'
                offset=len(matrix.log_text(log));source.write_text('return { broken =')
                wait('UI_ERROR',offset);wait('companion=true',offset);assert client.poll() is None
                behavior.write_text(behavior.read_text().replace('"output"','"new-default"'))
                time.sleep(.2);offset=len(matrix.log_text(log));source.write_text(original)
                wait('UI_COMMIT',offset);wait('companion=true',offset)
                assert state(offset)['panel.page']=='output'
                call('close');deadline=time.monotonic()+12
                while call('state')['resident']:
                    if time.monotonic()>deadline:raise RuntimeError('reload panel release deadline')
                    time.sleep(.1)
                offset=len(matrix.log_text(log));call('open');wait('UI_PRESENTED generation=1',offset,True)
                assert state(offset)['panel.page']=='output', 'closed worker lost durable page'
                saved=json.loads((pathlib.Path(env['XDG_STATE_HOME'])/'bead/panels.json').read_text())
                assert saved['values']['panel.page']=='output'
                os.kill(client.pid,signal.SIGKILL);client.wait(timeout=5)
                offset=len(matrix.log_text(log))
                client=subprocess.Popen(command,cwd=bundle,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
                wait('UI_PRESENTED generation=1',offset,True)
                assert call('state')['target_open']
                assert state(offset)['panel.page']=='output'
                result={'crash_restores_open_and_page':True,'reload_file_to_present_ms':latencies,'invalid_keeps_live_client':True,'dependent_recovery_preserves_page':True,'closed_reopen_preserves_page':True,'saved_keys':sorted(saved['values'])}
                (raw/'panel-reload-proof.json').write_text(json.dumps(result,indent=2)+'\n');print('PANEL_RELOAD_PROOF '+json.dumps(result),flush=True)
            finally:stop(client)
        assert 'STOP surfaces=0' in log.read_text()
