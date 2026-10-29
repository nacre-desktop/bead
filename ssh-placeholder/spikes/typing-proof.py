#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""One edit in flight; input receipt to exact committed/presented update.
CPU is whole process from input receipt to presentation callback. This includes
all application threads and idle callback overhead, but excludes injector CPU.
"""
import importlib.util,json,os,pathlib,re,shutil,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
spec=importlib.util.spec_from_file_location('menu_fixtures',ROOT/'spikes/menu-fixtures.py')
menu_fixtures=importlib.util.module_from_spec(spec);spec.loader.exec_module(menu_fixtures)
def sample(env,raw,compositor,stop):
    profile_mode=os.environ.get('BEAD_TYPING_PROFILE','1')
    assert profile_mode in ('1','clocks'),'unknown typing profile mode'
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_PROFILE_INPUT':os.environ.get('BEAD_TYPING_PROFILE','1'),'BEAD_MATCHED_ICONS':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    steady=os.environ.get('BEAD_TYPING_STEADY_LIST')=='1'
    wire_debug=os.environ.get('BEAD_TYPING_WIRE_DEBUG')=='1'
    if wire_debug:env['WAYLAND_DEBUG']='1'
    rows=[]
    with tempfile.TemporaryDirectory(prefix='bead-type-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(pathlib.Path(os.environ.get('BEAD_TYPING_BUNDLE',str(ROOT/'shell'))),bundle/'shell')
        env['BEAD_ICON_PATH']=str(bundle/'shell/sample.svg')
        rows_fixture=[{'id':f'proof-{i:04}','name':f'Application {i:04}','label':f'Application {i:04}','icon':str(ROOT/'shell/sample.svg'),'kind':'app','description':'Application fixture','breadcrumb':'All apps','operation':'app','disabled':False} for i in range(320)]
        legacy=os.environ.get('BEAD_TYPING_LEGACY')=='1'
        replies={'':{'rows':rows_fixture},'a':{'rows':rows_fixture},'ap':{'rows':rows_fixture}}
        changed=os.environ.get('BEAD_TYPING_CHANGED_RESULTS')=='1'
        if changed:
            assert not legacy,'changed results require the exact-query fixture Source'
            replies['a']={'rows':rows_fixture[:160]};replies['ap']={'rows':rows_fixture[160:]}
        catalog=bundle/'menu-fixture.json';catalog.write_text(json.dumps(rows_fixture if legacy else {'queries':{q:menu_fixtures.reply(q,r['rows'])for q,r in replies.items()}}))
        path=raw/'typing.log';pointer=None;client=None;keeper=None
        command=[binary,'--renderer',os.environ.get('BEAD_TYPING_RENDERER','cpu'),'--namespace','bead-typing','--ui','ui/main.luau','--catalog' if legacy else '--menu-fixture',str(catalog),'--backend','none','--test-keyboard']
        if compositor=='hyprland':command+=['--output','bead-bench']
        try:
            if os.environ.get('BEAD_KEYBOARD_KEEPER')=='1':
                keeper=subprocess.Popen(['wtype','-s','600000',''],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
                time.sleep(.1)
                assert keeper.poll() is None,'private keyboard keeper exited'
            with path.open('w') as log:
                client=subprocess.Popen(command,cwd=bundle,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                matrix.wait_log(path,lambda t:'UI_PRESENTED generation=1' in t,timeout=30)
                time.sleep(1)
                if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,check=True,capture_output=True,timeout=5)
                pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log')
                pointer.wait('POINTER_READY');time.sleep(.3);pointer.command('move 1000 300','POINTER_DONE');time.sleep(.2)
                if legacy:subprocess.run(['wtype','-s','300','-M','shift','-k','Tab','-m','shift','-s','300'],env=env,check=True,timeout=5)
                time.sleep(.2)
                time.sleep(.3)
                key_count=int(os.environ.get('BEAD_TYPING_KEYS','80'))
                def key_at(index):
                    return (['a']if index==0 else ['p']if index%2 else ['-k','BackSpace'])if steady else (['-k','BackSpace']if index%2 else['a'])
                # Repeated edits preserve a populated result set; never time no-match rows.
                for index in range(key_count):
                    offset=len(matrix.log_text(path));key=key_at(index)
                    subprocess.run(['wtype','-s','20',*key],env=env,check=True,timeout=5)
                    def ready(text):
                        segment=text[offset:]
                        return 'INPUT_RECEIVED'in segment and re.search(r'UI_PRESENTED .*update=(\d+) cpu_ns=(\d+)',segment) is not None
                    matrix.wait_log(path,ready,timeout=6)
                    segment=matrix.log_text(path)[offset:]
                    assert 'UI_ERROR' not in segment,'failed candidate during typing'
                    received=re.findall(r'INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)',segment)
                    assert len(received)==1,('not exactly one key',received)
                    start,cpu=map(int,received[0]);commits=re.findall(r'UI_COMMIT .*update=(\d+).*source_error=false',segment)
                    presentations=re.findall(r'UI_PRESENTED generation=(\d+) monotonic_ns=(\d+) refresh_ns=(\d+) companion=(\w+) update=(\d+) cpu_ns=(\d+)',segment)
                    assert commits,'edit did not commit'
                    found=[p for p in presentations if p[4] in commits and int(p[1])>=start]
                    assert found,'no presentation for edit commit'
                    gen,ns,refresh,companion,update,endcpu=found[0]
                    assert int(endcpu)>cpu>0 and int(ns)>start>0,'invalid input/presentation clocks'
                    profiles=[{'stage':s,'wall_ms':float(w),'cpu_ms':float(c)} for s,w,c in re.findall(r'INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)',segment)]
                    if profile_mode=='clocks':assert not profiles,'clock-only run emitted stage traces'
                    window=segment[segment.index('INPUT_RECEIVED'):];window=window[:window.index('UI_PRESENTED')]
                    rows.append({'index':index,'input_ns':start,'presented_ns':int(ns),'latency_ms':(int(ns)-start)/1e6,'cpu_ms':(int(endcpu)-cpu)/1e6,'update':int(update),'refresh_ns':int(refresh) or None,'profiles':profiles,'keymap_events_after_input':len(re.findall(r'wl_keyboard[^\n]*keymap\(',window))if wire_debug else None})
                    time.sleep(float(os.environ.get('BEAD_TYPING_PAUSE','.04')))
                assert 'PRESENTATION clock_id=1' in matrix.log_text(path)
                idle=None
                if os.environ.get('BEAD_TYPING_IDLE_SECONDS'):
                    idle=matrix.idle(client.pid,float(os.environ['BEAD_TYPING_IDLE_SECONDS']))
                    subprocess.run(['grim',str(raw/'typing-native.png')],env=env,capture_output=True,check=True,timeout=8)
                result={'compositor':compositor,'catalog_size':320,'idle':idle,'source':'legacy catalog fixture' if legacy else 'exact menu.query fixtures; no engine','workload':'first grid-to-list key, then a/ap populated list'if steady else'alternating empty/a grid-list transitions','helper_cpu_ms':None,'unknown_refresh_predictions':sum(r['refresh_ns'] is None for r in rows),'profile_mode':os.environ.get('BEAD_TYPING_PROFILE','1'),'changed_result_ids':changed,'fixture_result_counts':{q:len(r['rows'])for q,r in replies.items()},'keys':len(rows),'latency_ms':matrix.percentiles([r['latency_ms'] for r in rows]),'cpu_ms':matrix.percentiles([r['cpu_ms'] for r in rows]),'memory':matrix.measure.memory(client.pid),'drm_mib':matrix.measure.drm_memory(client.pid),'rows':rows}
                result.update(input_driver='per-key wtype; presentation gated',wire_debug=wire_debug)

        finally:
            try:
                if pointer:pointer.close()
            finally:
                for process in [client,keeper]:
                    if process:stop(process)
        assert 'UI_ERROR' not in path.read_text()
    for key in ['cpu_ms','latency_ms']:
        values=[r[key] for r in rows];result[key]['mean']=sum(values)/len(values);result[key]['max']=max(values)
    result['valid']=True
    (raw/'typing-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('TYPING_PROOF '+json.dumps({k:v for k,v in result.items() if k!='rows'}),flush=True)
    return result
