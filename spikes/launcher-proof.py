#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fixture launcher on the existing bounded panel lifecycle; nested only."""
import importlib.util,json,os,pathlib,re,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
spec=importlib.util.spec_from_file_location('menu_fixtures',ROOT/'spikes/menu-fixtures.py')
menu_fixtures=importlib.util.module_from_spec(spec);spec.loader.exec_module(menu_fixtures)
def sample(env,raw,compositor,stop):
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_MATCHED_ICONS':'1','BEAD_ICON_PATH':str(ROOT/'shell/tests/icon.svg'),'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    if os.environ.get('BEAD_LAUNCHER_MMAP_THRESHOLD'):
        env['MALLOC_MMAP_THRESHOLD_']=os.environ['BEAD_LAUNCHER_MMAP_THRESHOLD']
    data=[{'id':f'app-{i:04}','name':f'Application {i:04}','kind':'app','description':'Application fixture','icon':'fixture'}for i in range(320)]
    fixture=raw/'menu.json';fixture.write_text(json.dumps({'queries':{'':menu_fixtures.reply('',data),'a':menu_fixtures.reply('a',data)}}));fixture.chmod(0o600)
    sock=pathlib.Path(env['XDG_RUNTIME_DIR'])/'launcher-command.sock'
    log=raw/'launcher.log';client=pointer=None
    command=[binary,'--bar','--launcher-panel','--menu-fixture',str(fixture),'--command-socket',str(sock),'--namespace','bead-launcher-proof','--trace-state','--report',str(raw/'cache-report.json'),'--seconds','180']
    if compositor=='hyprland':command+=['--output','bead-bench']
    def call(method,*args,target='bead.panel',ok=True):
        p=subprocess.run([binary,'ipc','--socket',str(sock),'--json',target,method,*args],env=env,cwd=ROOT,capture_output=True,text=True,timeout=5)
        assert (p.returncode==0)==ok, 'private command result differs'
        return json.loads(p.stdout)['result']if ok else None
    def state_wait(predicate,timeout=8):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            state=call('state')
            if predicate(state):return state
            time.sleep(.02)
        raise RuntimeError('launcher state deadline')
    def ui_query(text,query):
        for line in reversed(text.splitlines()):
            if line.startswith('UI_STATE '):
                data=json.loads(line.removeprefix('UI_STATE '))
                if data.get('launcher.search.text')==query:return True
        return False
    closed_samples=[]
    def closed_guard():
        state_wait(lambda s:not s['resident']and not s['target_open']and s['surfaces']==0)
        time.sleep(.4)
        sample=matrix.idle(client.pid,1)
        sample['cache_bytes']=call('state').get('cache_bytes')
        sample['allocator']=json.loads(call('state',target='nacre')).get('bead',{}).get('heap')
        index=len(closed_samples);closed_samples.append(sample)
        (raw/f'closed-{index}.json').write_text(json.dumps(sample,indent=2)+'\n')
        (raw/f'closed-{index}.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
        if sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']>bar['pss_plus_drm_mib']+5:
            time.sleep(4)
            later=matrix.idle(client.pid,1)
            (raw/f'closed-{index}-later.json').write_text(json.dumps(later,indent=2)+'\n')
            (raw/f'closed-{index}-later.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
        assert sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']<=bar['pss_plus_drm_mib']+5, 'launcher close exceeds bar+5 MiB' 
        return sample
    result={'compositor':compositor,'fixture_results':320,'engine':False,'cycles':[]}
    try:
        with log.open('w')as stream:
            client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            matrix.wait_log(log,lambda t:'UI_PRESENTED generation=1'in t and 'source_error=false'in t)
            assert 'PRESENTATION clock_id=1'in matrix.log_text(log)
            call('state',target='nacre')
            bar=matrix.idle(client.pid,2);result['bar']=bar
            bar['allocator']=json.loads(call('state',target='nacre')).get('bead',{}).get('heap')
            (raw/'bar.json').write_text(json.dumps(bar,indent=2)+'\n')
            (raw/'bar.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
            pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log');pointer.wait('POINTER_READY')
            pointer.command('move 1000 600','POINTER_DONE')
            x,y,w,h=call('state')['prepare_regions'][0]
            pointer.command(f'move {round(x+w/2)} {round(y+h/2)}','POINTER_DONE')
            state=state_wait(lambda s:s['prepared'])
            assert not state['target_open']and state['surfaces']==0,'hover mapped launcher'
            pointer.command('move 1000 600','POINTER_DONE')
            result['unused_hover']=closed_guard()
            for cycle in range(3):
                call('prepare');state_wait(lambda s:s['prepared'])
                offset=len(matrix.log_text(log));began=time.monotonic_ns();call('open')
                matrix.wait_log(log,lambda t:re.search(r'UI_PRESENTED generation=[1-9]\d* .*companion=true',t[offset:])is not None)
                if cycle==0:call('launcher','a',target='nacre')
                matrix.wait_log(log,lambda t:ui_query(t[offset:],'a'))
                time.sleep(.3)
                warm=matrix.idle(client.pid,1)
                current=call('state')
                assert current['target_open']
                assert all(abs(a-b)<=2 for a,b in zip(current['size'],[1164,654])), 'content-sized launcher geometry fed back into percentage limits'
                call('close');closed=closed_guard()
                result['cycles'].append({'cycle':cycle,'warm':warm,'closed':closed,'command_started_ns':began})
            state=json.loads(call('state',target='nacre'))
            assert state['query']=='a'and not state['launcher']and 'launcher'in state['bead']['available']
            call('controls','sound',target='nacre',ok=False)
            saved=json.loads((pathlib.Path(env['XDG_STATE_HOME'])/'bead/panels.json').read_text())['values']
            assert 'launcher.search.text'not in saved,'typed query reached disk state'
            assert 'UI_ERROR'not in log.read_text()
    finally:
        if pointer:pointer.close()
        if client:stop(client)
    assert 'STOP surfaces=0'in log.read_text()
    result.update(valid=True,hover_timeout=True,explicit_prepare=True,query_preserved_after_release=True,query_not_on_disk=True)
    (raw/'launcher-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('LAUNCHER_SOURCE_PROOF compositor='+compositor+' cycles=3 hover_timeout=true prepare=true query_preserved=true closed_bar_plus5=true clean=true',flush=True)
