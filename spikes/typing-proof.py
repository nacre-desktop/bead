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
def sample(env,raw,compositor,stop):
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_PROFILE_INPUT':'1','BEAD_MATCHED_ICONS':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    rows=[]
    with tempfile.TemporaryDirectory(prefix='bead-type-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(pathlib.Path(os.environ.get('BEAD_TYPING_BUNDLE',str(ROOT/'shell'))),bundle/'shell')
        env['BEAD_ICON_PATH']=str(bundle/'shell/sample.svg')
        catalog=bundle/'catalog.json';catalog.write_text(json.dumps([{'id':f'proof-{i:04}','name':f'Application {i:04}','icon':'fixture'} for i in range(320)]))
        path=raw/'typing.log';pointer=None;client=None
        command=[binary,'--renderer',os.environ.get('BEAD_TYPING_RENDERER','femtovg-gl'),'--namespace','bead-typing','--ui','ui/main.luau','--catalog',str(catalog),'--backend','none','--test-keyboard']
        if compositor=='hyprland':command+=['--output','bead-bench']
        try:
            with path.open('w') as log:
                client=subprocess.Popen(command,cwd=bundle,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                matrix.wait_log(path,lambda t:'UI_PRESENTED generation=1' in t,timeout=30)
                time.sleep(1)
                if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,check=True,capture_output=True,timeout=5)
                pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log')
                pointer.wait('POINTER_READY');time.sleep(.3);pointer.command('move 1000 300','POINTER_DONE');time.sleep(.2)
                subprocess.run(['wtype','-s','300','-M','shift','-k','Tab','-m','shift','-s','300'],env=env,check=True,timeout=5)
                time.sleep(.2)
                time.sleep(.3)
                # Repeated edits preserve a populated result set; never time no-match rows.
                for index in range(int(os.environ.get('BEAD_TYPING_KEYS','80'))):
                    offset=len(matrix.log_text(path));key=['-k','BackSpace'] if index%2 else ['a']
                    subprocess.run(['wtype','-s','20',*key],env=env,check=True,timeout=5)
                    def ready(text):
                        segment=text[offset:]
                        return re.search(r'UI_PRESENTED .*update=(\d+) cpu_ns=(\d+)',segment) is not None
                    matrix.wait_log(path,ready,timeout=6)
                    segment=matrix.log_text(path)[offset:]
                    received=re.findall(r'INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)',segment)
                    assert len(received)==1,('not exactly one key',received)
                    start,cpu=map(int,received[0]);commits=re.findall(r'UI_COMMIT .*update=(\d+).*',segment)
                    presentations=re.findall(r'UI_PRESENTED generation=(\d+) monotonic_ns=(\d+) refresh_ns=(\d+) companion=(\w+) update=(\d+) cpu_ns=(\d+)',segment)
                    assert commits,'edit did not commit'
                    found=[p for p in presentations if p[4] in commits and int(p[1])>=start]
                    assert found,'no presentation for edit commit'
                    gen,ns,refresh,companion,update,endcpu=found[0]
                    profiles=[{'stage':s,'wall_ms':float(w),'cpu_ms':float(c)} for s,w,c in re.findall(r'INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)',segment)]
                    rows.append({'index':index,'input_ns':start,'presented_ns':int(ns),'latency_ms':(int(ns)-start)/1e6,'cpu_ms':(int(endcpu)-cpu)/1e6,'update':int(update),'refresh_ns':int(refresh),'profiles':profiles})
                    time.sleep(.04)
                assert 'PRESENTATION clock_id=1' in matrix.log_text(path)
                result={'compositor':compositor,'catalog_size':320,'keys':len(rows),'latency_ms':matrix.percentiles([r['latency_ms'] for r in rows]),'cpu_ms':matrix.percentiles([r['cpu_ms'] for r in rows]),'memory':matrix.measure.memory(client.pid),'drm_mib':matrix.measure.drm_memory(client.pid),'rows':rows}
                (raw/'typing-proof.json').write_text(json.dumps(result,indent=2)+'\n')
                print('TYPING_PROOF '+json.dumps({k:v for k,v in result.items() if k!='rows'}),flush=True)
        finally:
            if pointer:pointer.close()
            if client:stop(client)
        assert 'UI_ERROR' not in path.read_text()
    return result
