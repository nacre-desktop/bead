#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Full-size bar and distinct GL companion measurements in measure.py containment."""
import importlib.util
import json
import math
import os
import pathlib
import re
import subprocess
import tempfile
import time

ROOT=pathlib.Path(__file__).resolve().parent.parent

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
measure=module('measure',ROOT/'spikes/measure.py')
services=module('services_lifecycle',ROOT/'spikes/services-lifecycle.py')
reference=module('bar_reference',ROOT/'spikes/bar-reference.py')

def percentiles(values):
    v=sorted(values)
    return {key:v[min(len(v)-1,math.ceil((len(v)-1)*p))] for key,p in [('p50',.5),('p95',.95),('p99',.99)]} if v else None

def wait_log(path,predicate,timeout=15):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        text=path.read_text()
        result=predicate(text)
        if result:return result
        time.sleep(.005)
    raise RuntimeError('Bar log timeout '+str(path)+'\n'+path.read_text()[-1500:])

def idle(pid,seconds):
    before=measure.ticks(pid);started=time.monotonic();time.sleep(seconds)
    elapsed=time.monotonic()-started;cpu=measure.ticks(pid)-before
    memory=measure.memory(pid);drm=measure.drm_memory(pid)
    return {'memory':memory,'drm_mib':drm,'pss_plus_drm_mib':memory['pss_mib']+sum((drm or {}).values()),'cpu_ticks':cpu,'seconds':elapsed,'cpu_one_core_percent':100*cpu/os.sysconf('SC_CLK_TCK')/elapsed,'fds':len(list(pathlib.Path(f'/proc/{pid}/fd').iterdir())),'threads':len(list(pathlib.Path(f'/proc/{pid}/task').iterdir()))}

def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-pw','BEAD_PRIVATE_SERVICES_TEST':'1'}
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release'
    rows=[];pw=None;windows=None
    (raw/'source-revision.txt').write_text(subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True))
    seconds=float(os.environ.get('BEAD_MATRIX_SECONDS','60'))
    count=int(os.environ.get('BEAD_MATRIX_ACTIONS','220'))
    smoke=os.environ.get('BEAD_MATRIX_SMOKE')
    try:
        pw=services.pipewire(env,raw)
        windows=subprocess.Popen(['quickshell','--path',str(ROOT/'spikes/backend-windows.qml')],cwd=ROOT,env=env,stdout=(raw/'windows.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        time.sleep(2)
        for renderer in ['cpu','femtovg-gl']:
            for companion in [False,True]:
                case=renderer+'-'+('panel' if companion else 'alone')
                cases=os.environ.get('BEAD_MATRIX_CASES','').split(',')
                if cases != [''] and case not in cases:continue
                label=f'{compositor}-'+case
                path=raw/(label+'.log');client=None;probe=None;tray=None
                command=[str(target/'nacre-shell'),'--bar','--renderer',renderer,'--trace-state','--memory-usage','--namespace','bead-feature6-matrix']+(['--bar-panel'] if companion else [])
                if compositor=='hyprland':command+=['--output','bead-bench']
                try:
                    client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=path.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
                    wait_log(path,lambda t:'UI_COMMIT' in t and 'READY output=' in t)
                    tray=subprocess.Popen([str(target/'examples/tray-fixture')],env=env,stdout=(raw/(label+'-tray.log')).open('w'),stderr=subprocess.STDOUT,start_new_session=True)
                    time.sleep(3)
                    assert client.poll() is None,path.read_text()
                    assert 'UI_ERROR' not in path.read_text(),path.read_text()
                    text=path.read_text()
                    assert ('physical=2880x75' in text or ('physical=2880x75' in text.replace(' ',''))),text[-3000:]
                    if companion:
                        assert text.count('GPU_SHARED devices=1 device_id=0') >= (2 if renderer=='femtovg-gl' else 1),text[-3000:]
                    print('BAR_IDLE '+label+' '+str(seconds)+'s',flush=True)
                    row={'label':label,'renderer':renderer,'companion':companion,'idle':idle(client.pid,seconds)}
                    subprocess.run(['grim',str(raw/(label+'.png'))],env=env,capture_output=True,check=True,timeout=8)
                    # Actual native focus actions drive the bar's title and a fresh scene generation.
                    probe=services.Probe([str(target/'nacre-shell'),'--backend-probe'],env,raw/(label+'-probe.log'))
                    snapshot=probe.wait('BACKEND_SNAPSHOT',lambda s:len([w for w in s['windows'] if w['title'].startswith('Bead fixture')])==2)
                    owned=[w for w in snapshot['windows'] if w['title'].startswith('Bead fixture')]
                    focused=next((w['id'] for w in owned if w['focused']),None);owned.sort(key=lambda w:w['id']==focused)
                    samples=[]
                    for i in range(count):
                        window=owned[i%2];offset=len(path.read_text())
                        result=probe.command({'action':'focus-window','window':window['id']},'BACKEND_RESULT')
                        assert result['ok'] and result['observed'],result
                        def presented(text):
                            tail=text[offset:]
                            if not any(l.startswith('UI_STATE ') and json.loads(l[9:]).get('bead.focused_title')==window['title'] for l in tail.splitlines()):return None
                            commits=re.findall(r'UI_COMMIT generation=(\d+).*monotonic_ns=(\d+) update=(\d+)',tail)
                            if not commits:return None
                            generation,commit_ns,update=commits[-1]
                            matches=re.findall(r'UI_PRESENTED generation='+generation+r' monotonic_ns=(\d+).*companion=false update='+update+r'(?:\s|$)',tail)
                            if not matches:return None
                            event=re.findall(r'UI_EVENT monotonic_ns=(\d+) title='+re.escape(window['title']),tail)
                            return {**result,'ui_event_ns':int(event[-1]) if event else None,'ui_commit_ns':int(commit_ns),'presented_ns':int(matches[-1])}
                        observed=wait_log(path,presented)
                        observed['action_to_presented_ms']=(observed['presented_ns']-observed['action_ns'])/1e6
                        observed['ui_prepare_ms']=(observed['ui_commit_ns']-observed['ui_event_ns'])/1e6 if observed['ui_event_ns'] else None
                        samples.append(observed)
                        time.sleep(.03)
                    row['action_samples']=samples
                    row['state_ms']=percentiles([s['state_update_ms'] for s in samples])
                    row['presented_ms']=percentiles([s['action_to_presented_ms'] for s in samples])
                    row['ui_prepare_ms']=percentiles([s['ui_prepare_ms'] for s in samples if s['ui_prepare_ms'] is not None])
                    row['after_actions']=idle(client.pid,1)
                    probe.close();probe=None
                    if os.environ.get('BEAD_BAR_UI_CHURN'):
                        def tray_state(text,wanted,offset):
                            states=[json.loads(l[9:]) for l in text[offset:].splitlines() if l.startswith('UI_STATE ')]
                            return bool(states) and len(states[-1]['bead.desktop.tray'])==wanted
                        offset=len(path.read_text());stop(tray);tray=None
                        wait_log(path,lambda text:tray_state(text,0,offset))
                        churn=[]
                        for cycle in range(100):
                            offset=len(path.read_text())
                            tray=subprocess.Popen([str(target/'examples/tray-fixture')],env=env,stdout=(raw/(label+'-churn-tray.log')).open('a'),stderr=subprocess.STDOUT,start_new_session=True)
                            wait_log(path,lambda text:tray_state(text,1,offset))
                            offset=len(path.read_text());stop(tray);tray=None
                            wait_log(path,lambda text:tray_state(text,0,offset))
                            if cycle in [9,49,99]:
                                time.sleep(.1)
                                churn.append({'cycle':cycle+1,**services.resources(client.pid)})
                        assert all(s['fds']==churn[0]['fds'] for s in churn),churn
                        assert churn[-1]['pss_mib']-churn[0]['pss_mib']<2,churn
                        resource_lines=[l for l in path.read_text().splitlines() if l.startswith('UI_RESOURCES ')]
                        assert all('watchers=1 subscriptions=1' in l and 'layers=1' in l for l in resource_lines[-100:]),resource_lines[-3:]
                        row['rendered_tray_churn']=churn
                        print('BAR_UI_CHURN '+label+' '+json.dumps(churn),flush=True)
                    # Frame pacing is a separate continuously requested, unchanged-content workload.
                    stop(client);client=None
                    pacing_path=raw/(label+'-pacing.log')
                    paced=subprocess.Popen(command+['--animate','--seconds','8'],cwd=ROOT,env=env,stdout=pacing_path.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
                    try:paced.wait(timeout=20)
                    finally:stop(paced)
                    assert paced.returncode==0,pacing_path.read_text()
                    row['pacing']=[l for l in pacing_path.read_text().splitlines() if l.startswith(('METRICS','ACQUIRE'))]
                    rows.append(row)
                    (raw/'bar-matrix.json').write_text(json.dumps({'compositor':compositor,'scope':'private outputs; compositor presentation feedback, not physical scanout','rows':rows},indent=2)+'\n')
                    print('BAR_RESULT '+json.dumps({k:v for k,v in row.items() if k not in ['action_samples']}),flush=True)
                finally:
                    if probe:stop(probe.process);probe.reader.join(timeout=2)
                    if tray:stop(tray)
                    if client:stop(client)
                assert 'STOP surfaces=0' in path.read_text() and 'UI_ERROR' not in path.read_text(),path.read_text()[-1500:]
                if smoke:return
        if os.environ.get('BEAD_MATRIX_CASES'):return
        # Matched 50px actual QML bar-only fixture, no generic/full-shell comparison.
        with tempfile.TemporaryDirectory(prefix='bead-qml-bar-') as directory:
            root=pathlib.Path(directory);reference.fixture(root)
            path=raw/(compositor+'-qml-bar.log')
            qml=subprocess.Popen(['quickshell','--path',str(root/'shell.qml')],env=env,stdout=path.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
            try:
                time.sleep(4);assert qml.poll() is None,path.read_text()
                print('QML_BAR_IDLE '+compositor+' '+str(seconds)+'s',flush=True)
                rows.append({'label':compositor+'-qml-bar','idle':idle(qml.pid,seconds),'scope':'unchanged NacreHeader visual widgets, deterministic transport shims, no QML tray extension'})
                subprocess.run(['grim','-g','0,0 1920x50',str(raw/(compositor+'-qml-bar.png'))],env=env,capture_output=True,check=True,timeout=8)
            finally:stop(qml)
        # Trace 220 focus actions with recording OFF/ON at identical boundaries.
        for recording in [False,True]:
            label=compositor+'-trace-'+str(recording)
            trace_env={**env,'BEAD_TRACE_BACKEND':'1'}
            if recording:trace_env['BEAD_RECORD_IPC']=str(raw/(label+'-ipc.jsonl'))
            probe=services.Probe([str(target/'nacre-shell'),'--backend-probe'],trace_env,raw/(label+'.log'))
            try:
                s=probe.wait('BACKEND_SNAPSHOT',lambda s:len([w for w in s['windows'] if w['title'].startswith('Bead fixture')])==2)
                owned=[w for w in s['windows'] if w['title'].startswith('Bead fixture')];focused=next((w['id'] for w in owned if w['focused']),None);owned.sort(key=lambda w:w['id']==focused)
                samples=[]
                for i in range(count):
                    result=probe.command({'action':'focus-window','window':owned[i%2]['id']},'BACKEND_RESULT');assert result['ok'] and result['observed'];samples.append(result)
                    time.sleep(.02)
                rows.append({'label':label,'samples':samples,'state_ms':percentiles([s['state_update_ms'] for s in samples]),'request_ms':percentiles([s['request_ms'] for s in samples]),'thread_cpu_ms':percentiles([s['thread_cpu_ms'] for s in samples])})
                print('BACKEND_RECORDING_COMPARE '+json.dumps({k:v for k,v in rows[-1].items() if k!='samples'}),flush=True)
            finally:probe.close()
        (raw/'bar-matrix.json').write_text(json.dumps({'compositor':compositor,'scope':'private outputs; compositor presentation feedback, not physical scanout','rows':rows},indent=2)+'\n')
    finally:
        if windows:stop(windows)
        if pw:stop(pw)
