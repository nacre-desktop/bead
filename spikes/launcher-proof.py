#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fixture launcher on the existing bounded panel lifecycle; nested only."""
import importlib.util,json,os,pathlib,re,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
spec=importlib.util.spec_from_file_location('menu_fixtures',ROOT/'spikes/menu-fixtures.py')
menu_fixtures=importlib.util.module_from_spec(spec);spec.loader.exec_module(menu_fixtures)
def sample(env,raw,compositor,stop,observe=None,bundle_root=None):
    runtime=pathlib.Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime!=pathlib.Path(f'/run/user/{os.getuid()}'),'live runtime refused'
    assert pathlib.Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime),'live display refused'
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_MATCHED_ICONS':'1','BEAD_ICON_PATH':str(ROOT/'shell/tests/icon.svg'),'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    if os.environ.get('BEAD_LAUNCHER_MMAP_THRESHOLD'):
        env['MALLOC_MMAP_THRESHOLD_']=os.environ['BEAD_LAUNCHER_MMAP_THRESHOLD']
    data=[{'id':f'app-{i:04}','name':f'Application {i:04}','kind':'app','description':'Application fixture','icon':'fixture'}for i in range(320)]
    fixture=raw/'menu.json';fixture.write_text(json.dumps({'queries':{'':menu_fixtures.reply('',data),'a':menu_fixtures.reply('a',data)}}));fixture.chmod(0o600)
    sock=pathlib.Path(env['XDG_RUNTIME_DIR'])/'launcher-command.sock'
    log=raw/'launcher.log';client=pointer=keeper=None
    engine_socket=env.get('BEAD_LAUNCHER_ENGINE_SOCKET')
    core_views=os.environ.get('BEAD_CORE_VIEWS_PROOF')=='1'
    release_state=os.environ.get('BEAD_PANEL_IDLE_STATE')=='release'
    query_on_open=os.environ.get('BEAD_LAUNCHER_QUERY_ON_OPEN')=='1'
    source_args=['--daemon-socket',engine_socket]if engine_socket else['--menu-fixture',str(fixture)]
    command=[binary,'--bar','--core-views'if core_views else'--launcher-panel',*source_args,'--command-socket',str(sock),'--namespace','bead-launcher-proof','--trace-state','--report',str(raw/'cache-report.json'),'--seconds','300'if core_views else'180']
    if os.environ.get('BEAD_LAUNCHER_UI_PROVIDER')=='1':
        assert engine_socket,'shared UI provider requires the real daemon Source'
        command+=['--ui-provider']
    if compositor=='hyprland':command+=['--output','bead-bench']
    def call(method,*args,target='bead.panel',ok=True):
        p=subprocess.run([binary,'ipc','--socket',str(sock),'--json',target,method,*args],env=env,cwd=ROOT,capture_output=True,text=True,timeout=5)
        if (p.returncode==0)!=ok:
            (raw/'ipc-failure.json').write_text(json.dumps({'target':target,'method':method,'args':args,'returncode':p.returncode,'stdout':p.stdout[-4096:],'stderr':p.stderr[-4096:],'client_returncode':client.poll()if client else None,'socket_exists':sock.exists(),'monotonic_ns':time.monotonic_ns()},indent=2)+'\n')
            raise AssertionError('private command result differs; see ipc-failure.json')
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
                if data.get('launcher.search.text')==query:
                    return not engine_socket or (not data.get('launcher.error') and bool(data.get('launcher.apps')))
        return False
    def committed_and_presented(text):
        for match in re.finditer(r'UI_COMMIT generation=(\d+)[^\n]* update=(\d+) companion=true source_error=false',text):
            presented=re.search(r'UI_PRESENTED generation='+match[1]+r' monotonic_ns=(\d+)[^\n]*companion=true update='+match[2]+r'\b',text[match.end():])
            if presented:return int(presented[1])
        return False
    def query_presented(text,query):
        # Join the requested state to a following companion commit/presentation,
        # rather than accepting an unrelated earlier frame in the same window.
        for match in re.finditer(r'^UI_STATE (.+)$',text,re.M):
            data=json.loads(match[1])
            if data.get('launcher.search.text')==query and (not engine_socket or (not data.get('launcher.error') and data.get('launcher.apps'))):
                stamp=committed_and_presented(text[match.end():])
                if stamp:return stamp
        return False
    closed_samples=[]
    def closed_guard(closed_at=None):
        state_wait(lambda s:not s['resident']and not s['target_open']and s['surfaces']==0)
        released_at=time.monotonic_ns()
        time.sleep(.4)
        sample=matrix.idle(client.pid,1)
        if closed_at is not None:sample['closed_idle_observed_after_ms']=(released_at-closed_at)/1e6
        if observe:sample['external']=observe()
        sample['cache_bytes']=call('state').get('cache_bytes')
        if release_state:
            assert json.loads(call('state',target='nacre'))['query']=='','closed-idle launcher retained its view state'
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
    result={'compositor':compositor,'fixture_results':None if engine_socket else 320,'engine':bool(engine_socket),'cycles':[]}
    try:
        if os.environ.get('BEAD_LAUNCHER_UI_PROVIDER')=='1':
            # Match input capabilities for control and candidate before the bar
            # baseline. Registering a keyboard only during candidate Escape
            # would wrongly charge its IME/seat setup to the new service view.
            keeper=subprocess.Popen(['wtype','-s','600000',''],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
            time.sleep(.25)
            assert keeper.poll() is None,'private keyboard keeper exited'
            result['keyboard_at_start']=True
        with log.open('w')as stream:
            client=subprocess.Popen(command,cwd=bundle_root or ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            matrix.wait_log(log,lambda t:'UI_PRESENTED generation=1'in t and 'source_error=false'in t)
            assert 'PRESENTATION clock_id=1'in matrix.log_text(log)
            call('state',target='nacre')
            bar=matrix.idle(client.pid,2);result['bar']=bar
            if observe:bar['external']=observe()
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
            if query_on_open:
                call('prepare');state_wait(lambda s:s['prepared'])
                result['unused_explicit_prepare']=closed_guard()
            for cycle in range(3):
                if not query_on_open:
                    call('prepare');state_wait(lambda s:s['prepared'])
                offset=len(matrix.log_text(log));began=time.monotonic_ns()
                if query_on_open:call('launcher','a',target='nacre')
                else:call('open')
                matrix.wait_log(log,lambda t:re.search(r'UI_PRESENTED generation=[1-9]\d* .*companion=true',t[offset:])is not None)
                if not query_on_open and (cycle==0 or release_state):call('launcher','a',target='nacre')
                matrix.wait_log(log,lambda t:ui_query(t[offset:],'a'))
                time.sleep(.3)
                warm=matrix.idle(client.pid,1)
                if observe:warm['external']=observe()
                current=call('state')
                warm['cache_bytes']=current.get('cache_bytes')
                assert current['target_open']
                assert all(abs(a-b)<=2 for a,b in zip(current['size'],[1164,654])), 'content-sized launcher geometry fed back into percentage limits'
                if cycle==0 and os.environ.get('BEAD_PANEL_QUICK_REOPEN')=='1':
                    offset=len(matrix.log_text(log))
                    call('close');call('open')
                    state_wait(lambda s:s['prepared'] and s['target_open'])
                    matrix.wait_log(log,lambda t:query_presented(t[offset:],'a'))
                    assert json.loads(call('state',target='nacre'))['query']=='a'
                    assert 'PANEL_VIEW_STATE_RELEASED'not in matrix.log_text(log)[offset:]
                    result['quick_reopen_retains_state']=True
                closed_at=time.monotonic_ns();call('close');closed=closed_guard(closed_at)
                closed['close_to_guard_ms']=(time.monotonic_ns()-closed_at)/1e6
                result['cycles'].append({'cycle':cycle,'warm':warm,'closed':closed,'command_started_ns':began})
            state=json.loads(call('state',target='nacre'))
            assert state['query']==(''if release_state else'a')and not state['launcher']and 'launcher'in state['bead']['available']
            if core_views:
                result['service_views']={'hover':{},'cycles':[]}
                for view in ['calendar','sound']:
                    regions=call('state')['prepare_views']
                    r=next(r for r in regions if r['view']==view)
                    x,y,w,h=r['rect']
                    pointer.command(f'move {round(x+w/2)} {round(y+h/2)}','POINTER_DONE')
                    prepared=state_wait(lambda s:s['view']==view and s['prepared'])
                    assert not prepared['target_open'] and prepared['surfaces']==0,'named hover mapped a surface'
                    pointer.command('move 1000 600','POINTER_DONE')
                    result['service_views']['hover'][view]=closed_guard()
                for cycle in range(int(os.environ.get('BEAD_CORE_VIEW_CYCLES','20'))):
                    view=['calendar','sound','launcher'][cycle%3]
                    offset=len(matrix.log_text(log));began=time.monotonic_ns()
                    if view=='calendar':
                        call('popout',view,str([20,960,1900][(cycle//3)%3]),target='nacre')
                    else:
                        call('open',view)
                    state_wait(lambda s:s['view']==view and s['target_open'] and s['prepared'])
                    presented=matrix.wait_log(log,lambda t:committed_and_presented(t[offset:]))
                    legacy=json.loads(call('state',target='nacre'))
                    assert legacy['launcher']==(view=='launcher')
                    assert legacy['edgeMenu']==(''if view=='launcher'else view)
                    if cycle<3:
                        time.sleep(.3)  # Inspect the settled visual, not an animation's first pose.
                        subprocess.run(['grim',str(raw/(view+'.png'))],env=env,capture_output=True,check=True,timeout=10)
                    if view=='calendar':
                        matrix.wait_log(log,lambda t:any('"calendar.ready":true'in line for line in t[offset:].splitlines()if line.startswith('UI_STATE ')))
                        if cycle==0:
                            before=call('state')
                            call('open','unknown-view',ok=False)
                            call('open','sound','missing-output',ok=False)
                            after=call('state')
                            for key in ['view','target_open','resident','surfaces']:
                                assert after[key]==before[key],'invalid request retired a current view'
                    warm=matrix.idle(client.pid,.2)
                    if observe:warm['external']=observe()
                    offset=len(matrix.log_text(log))
                    if cycle==0:
                        pointer.command('move 1000 600','POINTER_DONE')
                        pointer.command('click','POINTER_DONE')
                        close_method='outside pointer'
                    elif cycle==1:
                        # A newly registered virtual keyboard needs the same
                        # seat/focus handshake as the accepted sound proof.
                        subprocess.run(['wtype','-s','200','-k','Escape'],env=env,capture_output=True,check=True,timeout=5)
                        close_method='Escape'
                    else:
                        call('close');close_method='private IPC'
                    state_wait(lambda s:not s['target_open'])
                    closed_presented=matrix.wait_log(log,lambda t:committed_and_presented(t[offset:]))
                    closed=closed_guard()
                    result['service_views']['cycles'].append({'cycle':cycle,'view':view,'warm':warm,'closed':closed,'command_to_first_present_ms':(presented-began)/1e6,'close_presented_ns':closed_presented,'close_method':close_method})
                    (raw/'service-view-progress.json').write_text(json.dumps(result['service_views'],indent=2)+'\n')
                # Deliberately retire an unprepared worker; only the final epoch
                # may commit. No stale-result acceptance or relaxed marker test.
                offset=len(matrix.log_text(log))
                for view in ['calendar','sound','calendar','launcher']:
                    call('open',view)
                state_wait(lambda s:s['view']=='launcher' and s['target_open'] and s['prepared'])
                matrix.wait_log(log,lambda t:query_presented(t[offset:],''if release_state else'a'))
                call('close');result['service_views']['after_rapid_switch']=closed_guard()
            else:
                call('controls','sound',target='nacre',ok=False)
            saved=json.loads((pathlib.Path(env['XDG_STATE_HOME'])/'bead/panels.json').read_text())['values']
            assert 'launcher.search.text'not in saved,'typed query reached disk state'
            assert 'UI_ERROR'not in log.read_text()
            if engine_socket and observe:
                time.sleep(35)
                sample=matrix.idle(client.pid,1);sample['external']=observe()
                result['settled_closed']=sample
                (raw/'settled-closed.json').write_text(json.dumps(sample,indent=2)+'\n')
                assert sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']<=bar['pss_plus_drm_mib']+5
                assert sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']<=25,'settled UI exceeds owner25MiB ceiling'
                baseline=bar['pss_plus_drm_mib']+bar['memory']['swap_pss_mib']+bar['external']['pss_plus_drm_swap_mib']
                total=sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']+sample['external']['pss_plus_drm_swap_mib']
                assert total<=baseline+5, 'settled UI + daemon/workers exceeds matched bar group+5 MiB'
    finally:
        try:
            if pointer:pointer.close()
        finally:
            try:
                if client:stop(client)
            finally:
                if keeper:stop(keeper)
    assert client.returncode==0,'launcher did not exit cleanly'
    assert 'STOP surfaces=0'in log.read_text()
    result.update(valid=True,hover_timeout=True,explicit_prepare=True,query_preserved_after_release=not release_state,query_released_after_idle=release_state,query_on_open=query_on_open,query_not_on_disk=True)
    (raw/'launcher-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('LAUNCHER_SOURCE_PROOF compositor='+compositor+' cycles=3 hover_timeout=true prepare=true query_released_after_idle='+str(release_state).lower()+' closed_bar_plus5=true clean=true',flush=True)
