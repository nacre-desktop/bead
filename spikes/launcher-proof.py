#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fixture launcher on the existing bounded panel lifecycle; nested only."""
import hashlib,importlib.util,json,os,pathlib,re,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
spec=importlib.util.spec_from_file_location('menu_fixtures',ROOT/'spikes/menu-fixtures.py')
menu_fixtures=importlib.util.module_from_spec(spec);spec.loader.exec_module(menu_fixtures)
spec=importlib.util.spec_from_file_location('dashboard_residency',ROOT/'spikes/dashboard-residency.py')
dashboard_residency=importlib.util.module_from_spec(spec);spec.loader.exec_module(dashboard_residency)

def start_keyboard_keeper(requested, acceptance, env):
    if not requested:
        return None
    duration_ms=1800000 if acceptance else 600000
    return subprocess.Popen(['wtype','-s',str(duration_ms),''],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)

def check_keyboard_keeper(keeper, stage):
    if keeper is not None and keeper.poll() is not None:
        raise AssertionError('private keyboard keeper exited '+stage)

def stop_keyboard_keeper(keeper, stop):
    if keeper is not None:
        stop(keeper)

def build_launcher_command(binary,core_views,source_args,sock,raw,dashboard_views,status_views,
                           dashboard_fixtures,provider,engine_socket,compositor,acceptance):
    lifetime=1800 if acceptance is not None else (300 if core_views else 180)
    command=[binary,'--bar','--core-views'if core_views else'--launcher-panel',*source_args,
             '--command-socket',str(sock),'--namespace','bead-launcher-proof','--trace-state',
             '--report',str(raw/'cache-report.json'),'--seconds',str(lifetime)]
    if dashboard_views:
        assert core_views and status_views,'dashboard proof requires full core views'
        if dashboard_fixtures:command+=['--dashboard-fixtures',dashboard_fixtures]
    if provider:
        assert engine_socket,'shared UI provider requires the real daemon Source'
        command+=['--ui-provider']
    if compositor=='hyprland':command+=['--output','bead-bench']
    return command

def sample(env,raw,compositor,stop,observe=None,bundle_root=None):
    acceptance=dashboard_residency.acceptance_config(os.environ)
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
    status_views=os.environ.get('BEAD_CORE_STATUS_PROOF')=='1'
    dashboard_views=os.environ.get('BEAD_DASHBOARD_VIEWS_PROOF')=='1'
    dashboard_pages=os.environ.get('BEAD_DASHBOARD_PAGES','today,weather,media,system').split(',')
    assert dashboard_pages and len(set(dashboard_pages))==len(dashboard_pages) and all(p in ['today','weather','media','system'] for p in dashboard_pages)
    dashboard_default=os.environ.get('BEAD_DASHBOARD_DEFAULT_VIEW','today')
    assert dashboard_default in dashboard_pages
    release_state=os.environ.get('BEAD_PANEL_IDLE_STATE')=='release'
    query_on_open=os.environ.get('BEAD_LAUNCHER_QUERY_ON_OPEN')=='1'
    expected_query_rows=os.environ.get('BEAD_EXPECT_QUERY_ROWS')
    if expected_query_rows is not None:
        expected_query_rows=int(expected_query_rows)
        assert 1<=expected_query_rows<=2048,'invalid expected query row count'
    restored_mode=os.environ.get('BEAD_EXPECT_RESTORED_MODE')
    if restored_mode is not None:
        assert restored_mode in ['search','actions']
        private_state=pathlib.Path(env['XDG_STATE_HOME']).resolve()
        # The UI uses the outer compositor sandbox; bundle_root belongs to
        # the separate daemon fixture and intentionally has different XDG paths.
        private_root=runtime.parent
        assert private_root.name.startswith('bd-') and runtime.name=='r','non-fixture runtime refused'
        assert private_state==private_root/'state' and private_state.is_dir(),'non-fixture state refused'
        assert private_root.stat().st_uid==os.getuid() and private_root.stat().st_mode&0o077==0,'non-private sandbox refused'
        path=private_state/'bead/panels.json';path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        path.write_text(json.dumps({'version':1,'values':{'panel.open':False,'launcher.route':restored_mode}}))
        path.chmod(0o600)
    source_args=['--daemon-socket',engine_socket]if engine_socket else['--menu-fixture',str(fixture)]
    dashboard_fixtures=os.environ.get('BEAD_DASHBOARD_FIXTURES')
    command=build_launcher_command(binary,core_views,source_args,sock,raw,dashboard_views,status_views,
                                   dashboard_fixtures,os.environ.get('BEAD_LAUNCHER_UI_PROVIDER')=='1',
                                   engine_socket,compositor,acceptance)
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
    def dashboard_presented(text,page):
        for match in re.finditer(r'^UI_STATE (.+)$',text,re.M):
            data=json.loads(match[1])
            if data.get('dashboard.page')==page and data.get('panel.open') is True:
                stamp=committed_and_presented(text[match.end():])
                if stamp:return stamp
        return False
    def query_presented(text,query,expected_rows=None):
        # Join the requested state to a following companion commit/presentation,
        # rather than accepting an unrelated earlier frame in the same window.
        for match in re.finditer(r'^UI_STATE (.+)$',text,re.M):
            data=json.loads(match[1])
            if data.get('launcher.search.text')==query and data.get('panel.open') is True and (not engine_socket or (not data.get('launcher.error') and data.get('launcher.apps'))) and (expected_rows is None or len(data.get('launcher.apps',[]))==expected_rows):
                stamp=committed_and_presented(text[match.end():])
                if stamp:return stamp
        return False
    # A failing CONTROL may finish collecting data, but can NEVER pass the proof.
    # Pin this mode to the immutable accepted baseline; candidates remain strict.
    record_control=os.environ.get('BEAD_OBSERVE_FAILED_CONTROL')=='1'
    if record_control:
        assert not release_state and hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()=='5764dd202c8034c777cbf86fa989d0124613c08b8fe26d2a5806c94d6b4510cb'
    budget_failures=[]
    def budget(actual,limit,code):
        if actual>limit:
            if not record_control:raise AssertionError(code)
            budget_failures.append({'code':code,'actual_mib':actual,'limit_mib':limit})
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
        budget(sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib'],bar['pss_plus_drm_mib']+5,'launcher close exceeds bar+5 MiB')
        return sample
    result={'compositor':compositor,'fixture_results':None if engine_socket else 320,'engine':bool(engine_socket),'cycles':[]}
    try:
        if os.environ.get('BEAD_LAUNCHER_UI_PROVIDER')=='1':
            # Match input capabilities for control and candidate before the bar
            # baseline. Registering a keyboard only during candidate Escape
            # would wrongly charge its IME/seat setup to the new service view.
            keeper=start_keyboard_keeper(True,acceptance is not None,env)
            time.sleep(.25)
            check_keyboard_keeper(keeper,'at start')
            result['keyboard_at_start']=True
        def keeper_check(stage):
            check_keyboard_keeper(keeper,stage)
        with log.open('w')as stream:
            client=subprocess.Popen(command,cwd=bundle_root or ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            matrix.wait_log(log,lambda t:'UI_PRESENTED generation=1'in t and 'source_error=false'in t)
            assert 'PRESENTATION clock_id=1'in matrix.log_text(log)
            call('state',target='nacre')
            if acceptance:
                def mark_dashboard_sampling_start():
                    (raw/'dashboard-residency-sampling-start.json').write_text(json.dumps({
                        'started_monotonic_ns':time.monotonic_ns(),
                        'reason':'first 60-second B0002 acceptance baseline observation begins'
                    },indent=2)+'\n')
                bar,baseline_timing=dashboard_residency.acceptance_baseline(client.pid,matrix.idle,acceptance['warmup_seconds'],time.monotonic,time.sleep,keeper_check,mark_dashboard_sampling_start)
                result['acceptance_baseline']=baseline_timing
                (raw/'dashboard-residency-baseline.json').write_text(json.dumps(baseline_timing,indent=2)+'\n')
            else:
                bar=matrix.idle(client.pid,2)
            if os.environ.get('BEAD_REFERENCE_ABSOLUTE_GATE')=='1':
                if bar.get('drm_mib') is None:
                    raise RuntimeError('reference baseline lacks DRM fdinfo; absolute laptop verdict is unavailable')
                if bar['pss_plus_drm_mib']+bar['memory']['swap_pss_mib']>=24.5:
                    raise RuntimeError('reference baseline reached the 24.5 MiB stop line')
            result['bar']=bar
            if observe:bar['external']=observe()
            bar['allocator']=json.loads(call('state',target='nacre')).get('bead',{}).get('heap')
            (raw/'bar.json').write_text(json.dumps(bar,indent=2)+'\n')
            (raw/'bar.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
            if os.environ.get('BEAD_DASHBOARD_RESIDENCY') == '1':
                result=dashboard_residency.sample(client,raw,bar,matrix,call,state_wait,log,dashboard_presented,committed_and_presented,observe,dashboard_views,result.get('acceptance_baseline'),keeper_check)
                keeper_check('before success cleanup')
                stop(client)
                assert client.returncode==0,'residency UI did not exit cleanly'
                assert 'STOP surfaces=0' in log.read_text()
                client=None
                return result
            pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log');pointer.wait('POINTER_READY')
            pointer.command('move 1000 600','POINTER_DONE')
            x,y,w,h=call('state')['prepare_regions'][0]
            pointer.command(f'move {round(x+w/2)} {round(y+h/2)}','POINTER_DONE')
            state=state_wait(lambda s:s['prepared'])
            assert not state['target_open']and state['surfaces']==0,'hover mapped launcher'
            pointer.command('move 1000 600','POINTER_DONE')
            result['unused_hover']=closed_guard()
            if os.environ.get('BEAD_PANEL_PREPARE_STRESS')=='1':
                call('prepare');state_wait(lambda s:s['prepared'])
                result['unused_explicit_prepare']=closed_guard()
            for cycle in range(3):
                if not query_on_open or cycle==0:
                    call('prepare');state_wait(lambda s:s['prepared'])
                offset=len(matrix.log_text(log));began=time.monotonic_ns()
                if cycle==0 and restored_mode is not None:
                    call('open')
                    def restored_presented(text):
                        for match in re.finditer(r'^UI_STATE (.+)$',text[offset:],re.M):
                            data=json.loads(match[1])
                            if data.get('panel.open') is True and data.get('launcher.mode')==restored_mode and data.get('launcher.route')==restored_mode and data.get('launcher.search.text')=='' and not data.get('launcher.error'):
                                if committed_and_presented(text[offset:][match.end():]):return True
                        return False
                    try:matrix.wait_log(log,restored_presented,timeout=8)
                    except RuntimeError:
                        states=[json.loads(m[1]) for m in re.finditer(r'^UI_STATE (.+)$',matrix.log_text(log)[offset:],re.M)]
                        seen=[{'mode':d.get('launcher.mode'),'route':d.get('launcher.route'),'query_empty':d.get('launcher.search.text')==''}for d in states if d.get('panel.open')is True]
                        (raw/'restored-mode-failure.json').write_text(json.dumps({'expected':restored_mode,'seen':seen[-8:]},indent=2)+'\n')
                        raise
                    result['restored_mode']=restored_mode
                    call('launcher','a',target='nacre')
                elif query_on_open:call('launcher','a',target='nacre')
                else:call('open')
                matrix.wait_log(log,lambda t:re.search(r'UI_PRESENTED generation=[1-9]\d* .*companion=true',t[offset:])is not None)
                if not query_on_open and (cycle==0 or release_state):call('launcher','a',target='nacre')
                matrix.wait_log(log,lambda t:ui_query(t[offset:],'a'))
                if expected_query_rows is not None:
                    matrix.wait_log(log,lambda t:query_presented(t[offset:],'a',expected_query_rows))
                    result['expected_query_rows']=expected_query_rows
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
                result['service_views']={'hover':{},'cycles':[],'status_bar_click_open':os.environ.get('BEAD_STATUS_BAR_CLICK_PROOF')=='1','placement_bar_click_open':os.environ.get('BEAD_PLACEMENT_BAR_CLICK_PROOF')=='1'}
                hover_views=['calendar','sound']+(['network','bluetooth','battery']if status_views else[])
                sequence=['calendar','sound','launcher']+(['network','bluetooth','battery']if status_views else[])
                if dashboard_views:
                    hover_views+=['dashboard-'+dashboard_default]
                    sequence+=['dashboard-'+page for page in dashboard_pages]
                for view in hover_views:
                    regions=call('state')['prepare_views']
                    r=next(r for r in regions if r['view']==view)
                    x,y,w,h=r['rect']
                    pointer.command(f'move {round(x+w/2)} {round(y+h/2)}','POINTER_DONE')
                    prepared=state_wait(lambda s:s['view']==view and s['prepared'])
                    assert not prepared['target_open'] and prepared['surfaces']==0,'named hover mapped a surface'
                    pointer.command('move 1000 600','POINTER_DONE')
                    result['service_views']['hover'][view]=closed_guard()
                for cycle in range(int(os.environ.get('BEAD_CORE_VIEW_CYCLES','20'))):
                    view=sequence[cycle%len(sequence)]
                    offset=len(matrix.log_text(log));began=time.monotonic_ns()
                    if (view in ['network','bluetooth','battery'] and os.environ.get('BEAD_STATUS_BAR_CLICK_PROOF')=='1') or (view=='calendar' and os.environ.get('BEAD_PLACEMENT_BAR_CLICK_PROOF')=='1'):
                        regions=call('state')['prepare_views']
                        region=next(r for r in regions if r['view']==view)
                        x,y,w,h=region['rect']
                        pointer.command(f'move {round(x+w/2)} {round(y+h/2)}','POINTER_DONE')
                        pointer.command('click','POINTER_DONE')
                    elif view in ['calendar','network','bluetooth','battery']:
                        call('popout',view,str([20,960,1900][(cycle//len(sequence))%3]),target='nacre')
                    else:
                        call('open',view)
                    state_wait(lambda s:s['view']==view and s['target_open'] and s['prepared'])
                    presented=matrix.wait_log(log,lambda t:dashboard_presented(t[offset:],view.removeprefix('dashboard-'))if view.startswith('dashboard-')else committed_and_presented(t[offset:]))
                    legacy=json.loads(call('state',target='nacre'))
                    assert legacy['launcher']==(view=='launcher')
                    if view.startswith('dashboard-') and os.environ.get('BEAD_DASHBOARD_LEGACY_STATE','1')=='1':
                        assert legacy['dashboard'] and legacy['edgeMenu']==''
                        assert legacy['tab']==['today','weather','media','system'].index(view.removeprefix('dashboard-'))
                    else:
                        assert legacy['edgeMenu']==(''if view=='launcher'else view)
                    if cycle<len(sequence):
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
                    if view in ['network','bluetooth','battery']:
                        title={'network':'Network','bluetooth':'Bluetooth','battery':'Battery'}[view]
                        def status_presented(text):
                            for match in re.finditer(r'^UI_STATE (.+)$',text[offset:],re.M):
                                data=json.loads(match[1])
                                if data.get('status.kind')==view and data.get('status.heading')==title and isinstance(data.get('status.ready'),bool) and data.get('panel.open')is True:
                                    stamp=committed_and_presented(text[offset:][match.end():])
                                    if stamp:return stamp
                            return False
                        presented=matrix.wait_log(log,status_presented)
                        geometry=call('state')
                        assert all(abs(a-b)<=2 for a,b in zip(geometry['size'],[374,286])),'status popout geometry differs from content+shadow'
                        (raw/(view+'-geometry.json')).write_text(json.dumps(geometry,indent=2)+'\n')
                    reference_seconds=os.environ.get('BEAD_CALENDAR_REFERENCE_SECONDS') if view=='calendar' else None
                    if view in ['network','bluetooth','battery']:reference_seconds=os.environ.get('BEAD_STATUS_REFERENCE_SECONDS')
                    if reference_seconds is not None:
                        duration=float(reference_seconds)
                        assert 1<=duration<=60,'reference duration outside bounded sample'
                        time.sleep(4)  # Match the static QML reference's warmup.
                    else:duration=.2
                    warm=matrix.idle(client.pid,duration)
                    if status_views:
                        warm['cache_bytes']=call('state').get('cache_bytes')
                        if view in ['network','bluetooth','battery']:
                            states=[json.loads(m[1]) for m in re.finditer(r'^UI_STATE (.+)$',matrix.log_text(log)[offset:],re.M)]
                            data=next(d for d in reversed(states)if d.get('status.kind')==view and d.get('panel.open')is True)
                            warm['status_data']={key:value for key,value in data.items()if key.startswith('status.')}
                    if reference_seconds is not None:warm['reference_seconds']=duration
                    if observe:warm['external']=observe()
                    offset=len(matrix.log_text(log))
                    close_index=(cycle+cycle//len(sequence))%3 if status_views else cycle
                    if close_index==0:
                        pointer.command('move 1000 1000'if status_views else'move 1000 600','POINTER_DONE')
                        pointer.command('click','POINTER_DONE')
                        close_method='outside pointer'
                    elif close_index==1:
                        # A newly registered virtual keyboard needs the same
                        # seat/focus handshake as the accepted sound proof.
                        subprocess.run(['wtype','-s','200','-k','Escape'],env=env,capture_output=True,check=True,timeout=5)
                        close_method='Escape'
                    else:
                        call('close');close_method='private IPC'
                    state_wait(lambda s:not s['target_open'])
                    closed_presented=matrix.wait_log(log,lambda t:committed_and_presented(t[offset:]))
                    closed=closed_guard()
                    result['service_views']['cycles'].append({'cycle':cycle,'view':view,'warm':warm,'closed':closed,'command_to_first_present_ms':(presented-began)/1e6,'close_presented_ns':closed_presented,'close_method':close_method,'open_method':'bar pointer'if view in ['network','bluetooth','battery']and os.environ.get('BEAD_STATUS_BAR_CLICK_PROOF')=='1'else'private IPC'})
                    (raw/'service-view-progress.json').write_text(json.dumps(result['service_views'],indent=2)+'\n')
                if dashboard_views and os.environ.get('BEAD_DASHBOARD_LEGACY_STATE','1')=='1':
                    result['dashboard_legacy']=[]
                    for index,page in enumerate(['today','weather','media','system']):
                        offset=len(matrix.log_text(log))
                        call('tab',str(index),target='nacre')
                        state_wait(lambda state:state['view']=='dashboard-'+page and state['target_open'] and state['prepared'])
                        matrix.wait_log(log,lambda text:dashboard_presented(text[offset:],page))
                        state=json.loads(call('state',target='nacre'))
                        assert state['dashboard'] and state['tab']==index and state['edgeMenu']==''
                        call('toggle','dashboard',target='drawers')
                        result['dashboard_legacy'].append({'tab':index,'page':page,'closed':closed_guard()})
                    offset=len(matrix.log_text(log));call('toggle','dashboard',target='drawers')
                    state_wait(lambda state:state['view']=='dashboard-today' and state['target_open'] and state['prepared'])
                    matrix.wait_log(log,lambda text:dashboard_presented(text[offset:],'today'))
                    call('toggle','dashboard',target='drawers');result['dashboard_toggle_closed']=closed_guard()
                    call('tab','4',target='nacre',ok=False)
                    call('tab','-1',target='nacre',ok=False)
                    assert not call('state')['target_open'],'unsupported tab changed visibility'
                # Deliberately retire an unprepared worker; only the final epoch
                # may commit. No stale-result acceptance or relaxed marker test.
                offset=len(matrix.log_text(log))
                rapid=['calendar','sound','network','bluetooth','battery','calendar','launcher']if status_views else['calendar','sound','calendar','launcher']
                if dashboard_views:rapid[-1:-1]=['dashboard-'+page for page in dashboard_pages]
                for view in rapid:
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
                budget(sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib'],bar['pss_plus_drm_mib']+5,'settled UI exceeds bar+5 MiB')
                budget(sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib'],25,'settled UI exceeds owner25MiB ceiling')
                baseline=bar['pss_plus_drm_mib']+bar['memory']['swap_pss_mib']+bar['external']['pss_plus_drm_swap_mib']
                total=sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']+sample['external']['pss_plus_drm_swap_mib']
                budget(total,baseline+5,'settled UI + daemon/workers exceeds matched bar group+5 MiB')
        keeper_check('before success cleanup')
    finally:
        try:
            if pointer:pointer.close()
        finally:
            try:
                if client:stop(client)
            finally:
                stop_keyboard_keeper(keeper,stop)
    assert client.returncode==0,'launcher did not exit cleanly'
    assert 'STOP surfaces=0'in log.read_text()
    result.update(valid=not budget_failures,budget_failures=budget_failures,observed_failed_control=record_control,hover_timeout=True,explicit_prepare=True,query_preserved_after_release=not release_state,query_released_after_idle=release_state,query_on_open=query_on_open,query_not_on_disk=True)
    (raw/'launcher-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('LAUNCHER_SOURCE_PROOF compositor='+compositor+' cycles=3 hover_timeout=true prepare=true query_released_after_idle='+str(release_state).lower()+' valid='+str(not budget_failures).lower()+' clean=true',flush=True)
    return result
