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


def default_external_observer(observer, engine_socket):
    if observer is not None:
        return observer
    if engine_socket:
        raise RuntimeError('a shared launcher engine requires an external-process observer')
    return lambda: {'processes': [], 'pss_plus_drm_swap_mib': 0.0,
                    'source': 'UI-only menu fixture; no external process'}

def build_launcher_command(binary,core_views,source_args,sock,raw,dashboard_views,status_views,
                           dashboard_fixtures,provider,engine_socket,compositor,acceptance):
    lifetime=1800 if acceptance is not None else (300 if core_views else 180)
    command=[binary,'--bar','--core-views'if core_views else'--launcher-panel',*source_args,
             '--command-socket',str(sock),'--namespace','bead-launcher-proof','--trace-state',
             '--report',str(raw/'cache-report.json'),'--seconds',str(lifetime)]
    renderer=os.environ.get('BEAD_REFERENCE_RENDERER','production')
    if renderer not in ('production','cpu'):
        raise RuntimeError('BEAD_REFERENCE_RENDERER must be production or cpu')
    command += ['--renderer', 'auto' if renderer == 'production' else 'cpu']
    if renderer == 'cpu': command += ['--panel-renderer', 'cpu']
    if dashboard_views:
        assert core_views and status_views,'dashboard proof requires full core views'
        if dashboard_fixtures:command+=['--dashboard-fixtures',dashboard_fixtures]
    if provider:
        assert engine_socket,'shared UI provider requires the real daemon Source'
        command+=['--ui-provider']
    if compositor=='hyprland':command+=['--output','bead-bench']
    return command

def renderer_evidence(pid, log, requested, panel_active=False):
    """Record the measured UI's renderer choice and live GPU mappings."""
    maps=pathlib.Path(f'/proc/{pid}/maps').read_text()
    argv=pathlib.Path(f'/proc/{pid}/cmdline').read_bytes().decode(errors='replace').split('\0')
    argv=[arg for arg in argv if arg]
    return classify_renderer(argv,maps,log.read_text(errors='replace'),requested,panel_active)

def unreclaimable(row):
    """B0081 headline: anon + shmem + SwapPss + DRM (PSS and clean file-backed are information only)."""
    m=row['memory']
    return m['pss_anon_mib']+m['pss_shmem_mib']+m['swap_pss_mib']+sum((row.get('drm_mib') or {}).values())

def expired_anon_delta_mib(start, expired):
    return expired['memory']['pss_anon_mib']-start['memory']['pss_anon_mib']

def expired_anon_within_budget(start, expired, budget_mib=10):
    return expired_anon_delta_mib(start, expired)<=budget_mib

SEPARATE_PROCESS_FLAGS = ('--app', '--ui', '--audio-panel', '--notification-bus', '--backend-probe')

def measured_process_kind(argv):
    """The memory gate measures the always-on shell only: `nacre-shell --bar --core-views ...`
    (bar, frame, lazy panels, notices/OSD overlay). Settings, sidebar, Colors and Welcome run as
    separate `--app` processes and are not part of the unreclaimable-memory target; any other argv is refused."""
    exe = pathlib.Path(argv[0]).name if argv else ''
    if exe != 'nacre-shell' or '--bar' not in argv[1:] or '--core-views' not in argv[1:]:
        return 'not-always-on-shell'
    if argv[1:2] in (['helper'], ['ipc']) or any(flag in argv[1:] for flag in SEPARATE_PROCESS_FLAGS):
        return 'not-always-on-shell'
    return 'always-on-shell'

def classify_renderer(argv, maps, text, requested, panel_active=False):
    """Pure classifier. RENDER_POLICY/BUFFER markers exist only in newer builds; the
    frozen control build emits none, so its CPU panel is affirmed from argv
    (`--renderer auto` is CPU for --bar/--core-views) plus a /proc maps scan with no
    GL/Vulkan libraries. Mapped GL libraries without a marker stay 'unknown' (blocked)."""
    libraries=sorted({pathlib.Path(line.split()[-1]).name for line in maps.splitlines()
                      if '/' in line.split()[-1] and re.search(r'(libGL|libEGL|iris_dri|libgallium|LLVM|vulkan)',line,re.I)})
    selected=argv[argv.index('--renderer')+1] if '--renderer' in argv and argv.index('--renderer')+1<len(argv) else None
    main='cpu' if ('--bar' in argv and '--core-views' in argv
                   and ((requested=='production' and selected=='auto')
                        or (requested=='cpu' and selected=='cpu'))) else 'unknown'
    panel='inactive' if not panel_active else 'unknown'
    if panel_active and re.search(r'RENDER_POLICY .*transition=gl-to-cpu', text): panel='cpu-fallback'
    elif panel_active and (re.search(r'RENDER_POLICY .*companion=true',text) or re.search(r'BUFFER companion=true .*transport=GL',text)): panel='native-gl'
    elif panel_active and requested=='cpu' and '--panel-renderer' in argv and argv[argv.index('--panel-renderer')+1:argv.index('--panel-renderer')+2]==['cpu']: panel='cpu'
    elif panel_active and requested=='production' and not libraries and 'GPU_SHARED' not in text and not re.search(r'transport=GL',text): panel='cpu (configured dashboard renderer)'
    wallpaper_active=bool(re.search(r'WALLPAPER_SURFACE_COMMITTED',text))
    return {'requested':requested,'main_bar':main,'opened_panel':panel,'measured_argv':argv,
            'measured_process':measured_process_kind(argv),
            'wallpaper_surface_committed':wallpaper_active,
            'gpu_libraries':libraries,'evidence_source':'measured PID /proc maps plus Bead renderer log markers'}

def fail_on_ui_error(text):
    # A Luau/runtime error never recovers; report it instead of waiting out the timeout.
    for line in text.splitlines():
        if line.startswith('UI_ERROR'):raise RuntimeError('UI_ERROR before first presentation: '+line[:300])
    return False


def launcher_open_evidence(text, offset, started_ns):
    """Extract measured first-content latency and presentation frames for one open."""
    return panel_open_evidence(text, offset, started_ns, 'launcher')


def panel_open_evidence(text, offset, started_ns, surface):
    """Extract one surface's first-content marker and presentation frames."""
    segment = text[offset:]
    latency = re.search(
        r'^PANEL_OPEN_LATENCY name=' + re.escape(surface) + r' ms_to_first_content_frame=([0-9.]+)',
        segment,
        re.M,
    )
    frames = [int(value) for value in re.findall(r'^PANEL_PRESENTED monotonic_ns=(\d+)', segment, re.M)]
    first_presented_ms = (frames[0] - started_ns) / 1_000_000 if frames else None
    return {
        'content_latency_ms': float(latency[1]) if latency else None,
        'first_presented_ms': first_presented_ms,
        'presented_frames': len(frames),
    }

def sample(env,raw,compositor,stop,observe=None,bundle_root=None):
    acceptance=dashboard_residency.acceptance_config(os.environ)
    bundle_root=pathlib.Path(bundle_root or os.environ.get('BEAD_HARDENING_BUNDLE_ROOT',ROOT)).resolve()
    runtime=pathlib.Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime!=pathlib.Path(f'/run/user/{os.getuid()}'),'live runtime refused'
    assert pathlib.Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime),'live display refused'
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_MATCHED_ICONS':'1','BEAD_ICON_PATH':str(bundle_root/'shell/tests/icon.svg'),'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    if os.environ.get('BEAD_LAUNCHER_MMAP_THRESHOLD'):
        env['MALLOC_MMAP_THRESHOLD_']=os.environ['BEAD_LAUNCHER_MMAP_THRESHOLD']
    data=[{'id':f'app-{i:04}','name':f'Application {i:04}','kind':'app','description':'Application fixture','icon':'fixture'}for i in range(320)]
    fixture=raw/'menu.json';fixture.write_text(json.dumps({'queries':{q:menu_fixtures.reply(q,data)for q in['','a','ap']}}));fixture.chmod(0o600)
    sock=pathlib.Path(env['XDG_RUNTIME_DIR'])/'launcher-command.sock'
    log=raw/'launcher.log';client=pointer=keeper=None
    engine_socket=env.get('BEAD_LAUNCHER_ENGINE_SOCKET')
    observe=default_external_observer(observe,engine_socket)
    core_views=os.environ.get('BEAD_CORE_VIEWS_PROOF')=='1'
    status_views=os.environ.get('BEAD_CORE_STATUS_PROOF')=='1'
    dashboard_views=os.environ.get('BEAD_DASHBOARD_VIEWS_PROOF')=='1'
    dashboard_pages=os.environ.get('BEAD_DASHBOARD_PAGES','today,weather,media,system').split(',')
    assert dashboard_pages and len(set(dashboard_pages))==len(dashboard_pages) and all(p in ['today','weather','media','system'] for p in dashboard_pages)
    dashboard_default=os.environ.get('BEAD_DASHBOARD_DEFAULT_VIEW','today')
    assert dashboard_default in dashboard_pages
    release_state=os.environ.get('BEAD_PANEL_IDLE_STATE')=='release'
    query_on_open=os.environ.get('BEAD_LAUNCHER_QUERY_ON_OPEN')=='1'
    reopen_input=os.environ.get('BEAD_LAUNCHER_REOPEN_INPUT_PROOF')=='1'
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
    def fixture_query_presented(text,query=None,expected_rows=None):
        for match in re.finditer(r'^UI_STATE (.+)$',text,re.M):
            data=json.loads(match[1])
            if (query is not None and data.get('launcher.search.text')!=query) or data.get('panel.open') is not True:
                continue
            if expected_rows is not None and len(data.get('launcher.apps',[]))!=expected_rows:
                continue
            if re.search(r'^PANEL_PRESENTED\b',text[match.end():],re.M):return True
        return False
    def query_frame_presented(text,query,expected_rows=None):
        if engine_socket:return query_presented(text,query,expected_rows)
        return fixture_query_presented(text,query,expected_rows)
    def input_frame_presented(text,symbol):
        key=re.search(rf'^INPUT_KEY sym=XK_{re.escape(symbol)}\b',text,re.M)
        return key is not None and re.search(r'^PANEL_PRESENTED\b',text[key.end():],re.M)is not None
    # A failing CONTROL may finish collecting data, but can NEVER pass the proof.
    # Pin this mode to the immutable accepted baseline; candidates remain strict.
    record_control=os.environ.get('BEAD_OBSERVE_FAILED_CONTROL')=='1'
    record_memory_measurements=os.environ.get('BEAD_MEMORY_REGRESSION_MEASURE_ONLY')=='1'
    memory_regression_only=os.environ.get('BEAD_MEMORY_REGRESSION_ONLY')=='1'
    if record_control:
        assert not release_state and hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()=='5764dd202c8034c777cbf86fa989d0124613c08b8fe26d2a5806c94d6b4510cb'
    budget_failures=[]
    def budget(actual,limit,code):
        if actual>limit:
            if not record_control and not record_memory_measurements:raise AssertionError(code)
            budget_failures.append({'code':code,'actual_mib':actual,'limit_mib':limit})
    closed_samples=[]
    def memory_snapshot(label,seconds=1):
        sample=matrix.idle(client.pid,seconds)
        sample['cache_bytes']=call('state').get('cache_bytes')
        sample['allocator']=json.loads(call('state',target='nacre')).get('bead',{}).get('heap')
        (raw/(label+'.json')).write_text(json.dumps(sample,indent=2)+'\n')
        (raw/(label+'.smaps')).write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
        return sample
    def closed_guard(closed_at=None):
        remembered_query=json.loads(call('state',target='nacre'))['query']
        state_wait(lambda s:not s['resident']and not s['target_open']and s['surfaces']==0)
        released_at=time.monotonic_ns()
        time.sleep(.4)
        sample=memory_snapshot(f'closed-{len(closed_samples)}',1)
        assert sample['memory']['swap_pss_mib']==0,'nonzero UI SwapPss after launcher close'
        if closed_at is not None:sample['closed_idle_observed_after_ms']=(released_at-closed_at)/1e6
        if observe:sample['external']=observe()
        if release_state:
            assert json.loads(call('state',target='nacre'))['query']==remembered_query,'closed-idle launcher lost its bounded input state'
        index=len(closed_samples);closed_samples.append(sample)
        if sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib']>bar['pss_plus_drm_mib']+5:
            time.sleep(4)
            later=matrix.idle(client.pid,1)
            (raw/f'closed-{index}-later.json').write_text(json.dumps(later,indent=2)+'\n')
            (raw/f'closed-{index}-later.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
        budget(sample['pss_plus_drm_mib']+sample['memory']['swap_pss_mib'],bar['pss_plus_drm_mib']+5,'launcher close exceeds bar+5 MiB')
        return sample
    result={'compositor':compositor,'fixture_results':None if engine_socket else 320,'engine':bool(engine_socket),'cycles':[]}
    try:
        if os.environ.get('BEAD_LAUNCHER_UI_PROVIDER')=='1' or reopen_input:
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
            matrix.wait_log(log,lambda t:fail_on_ui_error(t)or('UI_PRESENTED generation=1'in t and 'source_error=false'in t))
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
                bar=memory_snapshot('bar',2)
            requested_renderer=os.environ.get('BEAD_REFERENCE_RENDERER','production')
            bar['backend']=renderer_evidence(client.pid,log,requested_renderer)
            (raw/'render-backend-start.json').write_text(json.dumps(bar['backend'],indent=2)+'\n')
            if bar['backend']['measured_process']!='always-on-shell':
                raise RuntimeError('measured PID is not the always-on shell: '+' '.join(bar['backend']['measured_argv']))
            if os.environ.get('BEAD_REFERENCE_ABSOLUTE_GATE')=='1':
                if bar.get('drm_mib') is None:
                    raise RuntimeError('reference baseline lacks DRM fdinfo; absolute laptop verdict is unavailable')
                if unreclaimable(bar)>=9.5:
                    raise RuntimeError('reference baseline reached the 9.5 MiB unreclaimable stop line')
            result['bar']=bar
            assert bar['memory']['swap_pss_mib']==0,'nonzero UI SwapPss in bar baseline'
            if observe:bar['external']=observe()
            (raw/'bar.json').write_text(json.dumps(bar,indent=2)+'\n')
            (raw/'bar.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
            if os.environ.get('BEAD_DASHBOARD_RESIDENCY') == '1':
                result=dashboard_residency.sample(client,raw,bar,matrix,call,state_wait,log,dashboard_presented,committed_and_presented,observe,dashboard_views,result.get('acceptance_baseline'),keeper_check,
                                                  lambda state: renderer_evidence(client.pid,log,requested_renderer,
                                                                                  panel_active=bool(state.get('target_open'))))
                keeper_check('before success cleanup')
                stop(client)
                assert client.returncode==0,'residency UI did not exit cleanly'
                assert 'STOP surfaces=0' in log.read_text()
                client=None
                return result
            pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log');pointer.wait('POINTER_READY')
            if os.environ.get('BEAD_LAUNCHER_STUCK_PROOF') == '1':
                stuck = {'actions': [], 'frames_required': 2, 'first_content_frame_limit_ms': 16.7}
                views = ['launcher'] + [
                    view for view in os.environ.get('BEAD_STUCK_EXTRA_VIEWS', '').split(',') if view
                ]
                for view in views:
                    surface = {'dashboard-today': 'dashboard'}.get(view, view)
                    panel_log = raw / 'app-chat.log' if view == 'ai' else log
                    last_open_evidence = None
                    actions = ['escape', 'outside', 'toggle'] if view == 'launcher' else ['escape', 'outside']
                    for action in actions:
                        offset = len(matrix.log_text(log))
                        if view == 'ai':
                            def app_state():
                                if not panel_log.exists():
                                    return None
                                states = re.findall(
                                    r'^UI_STATE (.*)$', panel_log.read_text(errors='replace'), re.M
                                )
                                if not states:
                                    return None
                                state = json.loads(states[-1])
                                return {
                                    'target_open': bool(state.get('panel.open')),
                                    'prepared': 'PANEL_PRESENTED ' in panel_log.read_text(errors='replace'),
                                }
                        else:
                            app_state = lambda: call('state')

                        current_state = app_state()
                        if not current_state or not current_state.get('target_open'):
                            started_ns = time.monotonic_ns()
                            if view == 'launcher':
                                call('launcher', '', target='nacre')
                            elif view == 'ai':
                                call('left', target='nacre')
                            else:
                                call('open', view)
                            if view == 'ai':
                                deadline = time.monotonic() + 8
                                state = None
                                while time.monotonic() < deadline:
                                    state = app_state()
                                    if state and state.get('target_open') and state.get('prepared'):
                                        break
                                    time.sleep(.02)
                                if not state or not state.get('target_open'):
                                    raise RuntimeError('AI panel did not open')
                            else:
                                state_wait(lambda state: state['target_open'] and state['prepared'])
                            evidence_reader = lambda: panel_open_evidence(
                                panel_log.read_text(errors='replace'), 0 if view == 'ai' else offset,
                                started_ns, surface,
                            )
                            try:
                                matrix.wait_log(panel_log, lambda text: panel_open_evidence(
                                    text, 0 if view == 'ai' else offset, started_ns, surface
                                )['presented_frames'] > 1, timeout=8)
                            except RuntimeError:
                                pass
                            time.sleep(.35)
                            last_open_evidence = evidence_reader()
                            typed = True
                            if view == 'launcher':
                                subprocess.run(['wtype', '-s', '20', '-M', 'ctrl', '-k', 'a', '-m', 'ctrl',
                                                '-k', 'BackSpace', 'a'], env=env, capture_output=True,
                                               check=True, timeout=5)
                                try:
                                    matrix.wait_log(log, lambda text: ui_query(text[offset:], 'a'), timeout=8)
                                except RuntimeError:
                                    typed = False
                        else:
                            typed = True
                        evidence = last_open_evidence or {
                            'content_latency_ms': None,
                            'first_presented_ms': None,
                            'presented_frames': 0,
                        }
                        if action == 'escape':
                            subprocess.run(['wtype', '-s', '20', '-k', 'Escape'], env=env,
                                           capture_output=True, check=True, timeout=5)
                        elif action == 'outside':
                            pointer.command('move 1900 1180' if view == 'ai' else 'move 20 20', 'POINTER_DONE')
                            pointer.command('click', 'POINTER_DONE')
                        elif view == 'launcher':
                            subprocess.run(
                                ['wtype', '-s', '20', '-M', 'logo', '-k', 'space', '-m', 'logo'],
                                env=env, capture_output=True, check=True, timeout=5,
                            )
                        elif view == 'ai':
                            call('left', target='nacre')
                        else:
                            call('toggle', view)
                        closed = False
                        deadline = time.monotonic() + 1.5
                        while time.monotonic() < deadline:
                            state = app_state()
                            closed_now = not state['target_open'] if state else view == 'ai'
                            if closed_now:
                                closed = True
                                break
                            time.sleep(.02)
                        stuck['actions'].append({
                            'view': view,
                            'action': action,
                            'closed': closed,
                            'typed': typed,
                            **evidence,
                        })
                stuck['valid'] = all(
                    row['closed'] and row['typed']
                    for row in stuck['actions']
                ) and all(
                    any(
                        row['view'] == view
                        and row['presented_frames'] > 1
                        and row['content_latency_ms'] is not None
                        and row['content_latency_ms'] <= stuck['first_content_frame_limit_ms']
                        for row in stuck['actions']
                    )
                    for view in views
                )
                (raw / 'launcher-stuck-proof.json').write_text(json.dumps(stuck, indent=2) + '\n')
                if not stuck['valid'] and os.environ.get('BEAD_STUCK_RECORD_ONLY') != '1':
                    raise AssertionError('launcher open/close/animation proof failed; see launcher-stuck-proof.json')
                if client.poll() is None:
                    stop(client)
                stuck['shell_returncode'] = client.returncode
                if client.returncode != 0 and os.environ.get('BEAD_STUCK_RECORD_ONLY') != '1':
                    raise AssertionError('launcher did not exit cleanly')
                assert 'STOP surfaces=0' in log.read_text()
                (raw / 'launcher-stuck-proof.json').write_text(json.dumps(stuck, indent=2) + '\n')
                client = None
                return {'valid': stuck['valid'], 'launcher_stuck': stuck}
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
                elif reopen_input and cycle>0:call('open')
                elif query_on_open:call('launcher','a',target='nacre')
                else:call('open')
                matrix.wait_log(log,lambda t:(re.search(r'UI_PRESENTED generation=[1-9]\d* .*companion=true',t[offset:])is not None)
                    if engine_socket else fixture_query_presented(t[offset:]))
                if memory_regression_only:
                    if cycle>0:
                        subprocess.run(['wtype','-s','20','-k','BackSpace'],env=env,capture_output=True,check=True,timeout=6)
                        if os.environ.get('BEAD_MEMORY_REGRESSION_INPUT_MARKERS')=='1':
                            matrix.wait_log(log,lambda t:input_frame_presented(t[offset:],'BackSpace'))
                        else:matrix.wait_log(log,lambda t:ui_query(t[offset:],''))
                    subprocess.run(['wtype','-s','20','a'],env=env,capture_output=True,check=True,timeout=6)
                    if os.environ.get('BEAD_MEMORY_REGRESSION_INPUT_MARKERS')=='1':
                        matrix.wait_log(log,lambda t:input_frame_presented(t[offset:],'a'))
                    else:matrix.wait_log(log,lambda t:ui_query(t[offset:],'a'))
                else:
                    if not query_on_open and (cycle==0 or release_state) and not(reopen_input and cycle>0):call('launcher','a',target='nacre')
                    matrix.wait_log(log,lambda t:ui_query(t[offset:],'a'))
                if reopen_input and cycle>0:
                    key_offset=len(matrix.log_text(log))
                    subprocess.run(['wtype','-s','20','p'],env=env,capture_output=True,check=True,timeout=6)
                    matrix.wait_log(log,lambda t:query_presented(t[key_offset:],'ap'))
                    subprocess.run(['wtype','-s','20','-k','BackSpace'],env=env,capture_output=True,check=True,timeout=6)
                    matrix.wait_log(log,lambda t:query_presented(t[key_offset:],'a'))
                    result['idle_reopen_input_preserved_and_focused']=True
                if expected_query_rows is not None:
                    matrix.wait_log(log,lambda t:query_frame_presented(t[offset:],'a',expected_query_rows))
                    result['expected_query_rows']=expected_query_rows
                if reopen_input and cycle==0:
                    subprocess.run(['grim',str(raw/'launcher-opening.png')],env=env,capture_output=True,check=True,timeout=8)
                time.sleep(.3)
                if reopen_input and cycle==0:
                    subprocess.run(['grim',str(raw/'launcher-open.png')],env=env,capture_output=True,check=True,timeout=8)
                warm=memory_snapshot(f'launcher-open-{cycle}',1)
                assert warm['memory']['swap_pss_mib']==0,'nonzero UI SwapPss with launcher open'
                if observe:warm['external']=observe()
                current=call('state')
                assert current['target_open']
                if os.environ.get('BEAD_MEMORY_REGRESSION_ONLY')!='1':
                    assert all(abs(a-b)<=2 for a,b in zip(current['size'],[1164,654])), 'content-sized launcher geometry fed back into percentage limits'
                if cycle==0 and os.environ.get('BEAD_PANEL_QUICK_REOPEN')=='1':
                    offset=len(matrix.log_text(log))
                    call('close');call('open')
                    state_wait(lambda s:s['prepared'] and s['target_open'])
                    matrix.wait_log(log,lambda t:query_presented(t[offset:],'a'))
                    assert json.loads(call('state',target='nacre'))['query']=='a'
                    assert 'PANEL_VIEW_STATE_RELEASED'not in matrix.log_text(log)[offset:]
                    result['quick_reopen_retains_state']=True
                closed_at=time.monotonic_ns();call('close')
                if reopen_input and cycle==0:
                    subprocess.run(['grim',str(raw/'launcher-closing.png')],env=env,capture_output=True,check=True,timeout=8)
                closed=closed_guard(closed_at)
                if reopen_input and cycle==0:
                    subprocess.run(['grim',str(raw/'launcher-closed.png')],env=env,capture_output=True,check=True,timeout=8)
                closed['close_to_guard_ms']=(time.monotonic_ns()-closed_at)/1e6
                result['cycles'].append({'cycle':cycle,'warm':warm,'closed':closed,'command_started_ns':began})
            expiry_seconds=float(os.environ.get('BEAD_MEMORY_REGRESSION_EXPIRY_SECONDS','0'))
            if expiry_seconds:
                if not 1<=expiry_seconds<=180:raise ValueError('memory regression expiry must be between 1 and 180 seconds')
                time.sleep(expiry_seconds)
                expired=memory_snapshot('launcher-expired-closed',1)
                result['expired_closed']=expired
                delta=expired_anon_delta_mib(bar,expired)
                result['expired_closed_anon_delta_mib']=delta
                result['expired_closed_anon_within_10_mib']=expired_anon_within_budget(bar,expired)
                if not result['expired_closed_anon_within_10_mib'] and not record_memory_measurements:
                    raise AssertionError(f'launcher expiry retained {delta:.1f} MiB anon above bar (limit 10 MiB)')
            state=json.loads(call('state',target='nacre'))
            assert (state['query']=='a'or os.environ.get('BEAD_MEMORY_REGRESSION_INPUT_MARKERS')=='1')and not state['launcher']and 'launcher'in state['bead']['available']
            if core_views and os.environ.get('BEAD_MEMORY_REGRESSION_ONLY')!='1':
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
            elif not core_views:
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
                budget(unreclaimable(sample),10,'settled UI exceeds the 10 MiB unreclaimable ceiling')
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
    result.update(valid=not budget_failures,budget_failures=budget_failures,observed_failed_control=record_control,hover_timeout=True,explicit_prepare=True,query_preserved_after_release=True,view_data_released_after_idle=release_state,query_on_open=query_on_open,query_not_on_disk=True)
    (raw/'launcher-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('LAUNCHER_SOURCE_PROOF compositor='+compositor+' cycles=3 hover_timeout=true prepare=true input_preserved=true view_data_released_after_idle='+str(release_state).lower()+' valid='+str(not budget_failures).lower()+' clean=true',flush=True)
    return result
