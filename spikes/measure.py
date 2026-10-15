#!/usr/bin/env python3
"""Measure task-owned clients in nested niri and stop every process on exit.

SPDX-License-Identifier: MIT OR Apache-2.0
"""
import argparse
import json
import os
import pathlib
import re
import select
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent


def ticks(pid):
    fields = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return int(fields[11]) + int(fields[12])


def memory(pid):
    fields = {}
    for line in pathlib.Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
        if ':' in line:
            key, value = line.split(':', 1)
            if value.strip().endswith('kB'):
                fields[key] = int(value.split()[0])
    return {'pss_mib': fields['Pss'] / 1024, 'uss_mib': sum(fields.get(k, 0) for k in ['Private_Clean', 'Private_Dirty', 'Private_Hugetlb']) / 1024,
            'swap_pss_mib': fields.get('SwapPss',0)/1024, 'swap_mib':fields.get('Swap',0)/1024,
            'pss_anon_mib':fields.get('Pss_Anon',0)/1024,'pss_file_mib':fields.get('Pss_File',0)/1024,'pss_shmem_mib':fields.get('Pss_Shmem',0)/1024}


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def drm_memory(pid):
    # Deduplicate descriptors sharing a DRM client, prefer modern totals to legacy aliases.
    clients={}
    for p in pathlib.Path(f'/proc/{pid}/fdinfo').glob('*'):
        try:fields=dict(line.split(':',1) for line in p.read_text().splitlines() if line.startswith('drm-'))
        except (OSError,ValueError):continue
        if not any(k.startswith(('drm-total-','drm-memory-')) for k in fields):continue
        key=(fields.get('drm-driver',''),fields.get('drm-pdev',''),fields.get('drm-client-id',p.name))
        values={}
        for prefix in ['drm-memory-','drm-total-']:
            for k,v in fields.items():
                if k.startswith(prefix):
                    words=v.split();scale={'KiB':1024,'MiB':1024**2,'GiB':1024**3,'B':1}.get(words[1] if len(words)>1 else 'B',1)
                    values[k[len(prefix):]]=int(words[0])*scale/1024**2
        clients[key]=values
    regions={}
    for values in clients.values():
        for k,v in values.items():regions[k]=regions.get(k,0)+v
    return regions or None

def sample_retained(env,raw,target,mode,layout,run,seconds,virtual120):
    label=f'{mode}-{layout}-{run}'
    qml=mode.startswith('qml')
    ui=mode.startswith(('ui-','anim-')) or mode=='qml-ui'
    animated=mode.startswith('anim-') or os.environ.get('BEAD_ANIMATION')=='1'
    env.update({'BEAD_IDLE':'1' if ui and not animated else '0','BEAD_START_OPEN':'1','BEAD_CONTENT_SIZE':'1' if layout=='content' else '0','BEAD_LAYER':'1','BEAD_PALETTE_PATH':str(ROOT/'spikes/palette-stress.json'),'BEAD_SHELL_DIR':str(ROOT/'shell'),'BEAD_ICON_PATH':'/usr/share/icons/Adwaita/scalable/places/folder.svg'})
    # Primary variants use their own callback statistics with tracing disabled.
    # Qt is a startup control: trace is required for the same first-frame marker.
    env['WAYLAND_DEBUG']='1' if qml and not os.environ.get('BEAD_QML_PROBE') else '0'
    if qml:
        command=['quickshell','--path',os.environ.get('BEAD_QML_PANEL',str(ROOT/('spikes/panel-reload.qml' if ui else 'spikes/panel.qml')))]
        if os.environ.get('BEAD_QML_PROBE'):env['LD_PRELOAD']=os.environ['BEAD_QML_PROBE']
    else:
        renderer=mode.removeprefix('ui-').removeprefix('anim-').replace('-memory','').removesuffix('-3')
        command=[str(target/'nacre-shell'),'--animate','--start-open','--renderer',renderer,'--palette',str(ROOT/'spikes/palette-stress.json'),'--namespace','bead-dev-retained']
        if ui:command+=['--ui',os.environ.get('BEAD_UI_ENTRY','ui/main.luau')]
        if ui and not animated:command.remove('--animate')
        if os.environ.get('BEAD_CATALOG'):command+=['--catalog',os.environ['BEAD_CATALOG']]
        if os.environ.get('BEAD_BACKEND'):command+=['--backend','auto']
        if '-memory' in mode:command+=['--memory-usage']
        if mode.endswith('-3'):command+=['--surfaces','3']
        if layout=='content':command+=['--content-size']
        if virtual120:command+=['--output','bead-bench']
    started=time.monotonic();data=bytearray();ready=threading.Event();first=[];pending=set()
    path=raw/f'{label}.log'
    client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,bufsize=0)
    def drain():
        with path.open('wb') as log:
            for line in client.stdout:
                log.write(line);data.extend(line)
                if qml and os.environ.get('BEAD_QML_PROBE'):
                    if line.startswith(b'QML_READY') and not first:first.append(time.monotonic());ready.set()
                elif qml:
                    request=re.search(rb'wl_surface[@#]\d+\.frame\((?:new id )?wl_callback[@#](\d+)',line)
                    done=re.search(rb'wl_callback[@#](\d+)\.done(?:,\s*)?\(',line)
                    if request:pending.add(request[1])
                    if done and done[1] in pending and not first:first.append(time.monotonic());ready.set()
                elif line.startswith(b'READY') and not first:first.append(time.monotonic());ready.set()
    reader=threading.Thread(target=drain);reader.start()
    print(f'{label}: private PID {client.pid}; stop: kill -TERM {client.pid}',flush=True)
    peak_pss=0.;peak_gpu={}
    try:
        deadline=started+45
        while not ready.wait(.02):
            if client.poll() is not None or time.monotonic()>deadline:raise RuntimeError(f'{label}: startup failed, exit={client.poll()}; inspect ignored raw log')
            m=memory(client.pid);peak_pss=max(peak_pss,m['pss_mib'])
            for k,v in (drm_memory(client.pid) or {}).items():peak_gpu[k]=max(peak_gpu.get(k,0),v)
        if ui and not qml and (b'UI_COMMIT' not in data or b'UI_ERROR' in data):raise RuntimeError('UI candidate did not commit; fallback/error panel is not a valid benchmark')
        time.sleep(5)
        before_mem=memory(client.pid);before_gpu=drm_memory(client.pid);before_ticks=ticks(client.pid);before=time.monotonic()
        (raw/f'{label}-before.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
        print(f'{label}: sampling {seconds:g}s, first populated callback {(first[0]-started)*1000:.3f}ms',flush=True)
        while time.monotonic()<before+seconds:
            if client.poll() is not None:raise RuntimeError(f'{label}: exited during sample')
            m=memory(client.pid);peak_pss=max(peak_pss,m['pss_mib'])
            for k,v in (drm_memory(client.pid) or {}).items():peak_gpu[k]=max(peak_gpu.get(k,0),v)
            time.sleep(min(1,max(0,before+seconds-time.monotonic())))
        elapsed=time.monotonic()-before
        if os.environ.get('BEAD_CAPTURE'):
            subprocess.run(['grim',str(raw/(label+'.png'))],env=env,capture_output=True,timeout=5,check=True)
        after_mem=memory(client.pid);after_gpu=drm_memory(client.pid);peak_pss=max(peak_pss,after_mem['pss_mib'],before_mem['pss_mib']);cpu=100*(ticks(client.pid)-before_ticks)/os.sysconf('SC_CLK_TCK')/elapsed
        (raw/f'{label}-after.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
    finally:
        stop(client);reader.join(timeout=5)
        if reader.is_alive():raise RuntimeError('client log reader did not stop')
    drm_total=sum((after_gpu or {}).values());total=after_mem['pss_mib']+drm_total
    print(f'{label}: total_PSS_plus_DRM={total:.3f}MiB DRM_total={drm_total:.3f}MiB CPU={cpu:.3f}% startup_ms={(first[0]-started)*1000:.3f} PSS/USS={after_mem["pss_mib"]:.3f}/{after_mem["uss_mib"]:.3f} peak_PSS={peak_pss:.3f} GPU_before={before_gpu} GPU_after={after_gpu} GPU_peak={peak_gpu or None} exit={client.returncode}',flush=True)
    for line in data.decode(errors='replace').splitlines():
        if line.startswith(('PROFILE','RESOURCES','BUFFER','METRICS','ACQUIRE','STOP','FALLBACK','GPU_SHARED','QML_PRESENTED','QML_PRESENTATION','QML_UI_PRESENTED')):print(label+': '+line,flush=True)
    if not qml and (client.returncode!=0 or b'STOP surfaces=0' not in data):raise RuntimeError('renderer did not exit cleanly')

def sample_reload(env,raw,target,renderer):
    # Only the temporary bundle is edited; reference repos and checkout UI stay intact.
    with tempfile.TemporaryDirectory(prefix='bead-ui-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        behavior=bundle/'shell/legacy-ui/behavior.luau'
        behavior.write_text(behavior.read_text().replace('["launcher.search.text"] = ""','["launcher.search.text"] = "saved 😀"').replace('["launcher.scroll.y"] = 0','["launcher.scroll.y"] = 42'))
        command=[os.environ.get('BEAD_NATIVE_BINARY',str(target/'nacre-shell')),'--ui','legacy-ui/main.json','--renderer',renderer,'--trace-state','--namespace','bead-dev-reload']
        log=[];condition=threading.Condition();path=raw/('reload-'+renderer+'.log')
        client=subprocess.Popen(command,cwd=bundle,env={**env,'WAYLAND_DEBUG':'0'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
        def drain():
            with path.open('w') as stream:
                for line in client.stdout:
                    stream.write(line);stream.flush()
                    with condition:log.append(line.rstrip());condition.notify_all()
        reader=threading.Thread(target=drain);reader.start()
        def wait(marker,offset=0):
            deadline=time.monotonic()+15
            with condition:
                while True:
                    found=next((line for line in log[offset:] if marker in line),None)
                    if found:return found
                    if client.poll() is not None or time.monotonic()>deadline:raise RuntimeError(f'private reload proof: missing {marker}; inspect ignored raw log')
                    condition.wait(.1)
        def shot(label):
            try:subprocess.run(['grim',str(raw/(label+'.png'))],env=env,capture_output=True,timeout=5,check=True)
            except (subprocess.TimeoutExpired,subprocess.CalledProcessError):print(label+': screenshot unavailable',flush=True)
        try:
            first=wait('UI_PRESENTED generation=1');initial=next(json.loads(line.split(' ',1)[1]) for line in log if line.startswith('UI_STATE '))
            assert initial['launcher.search.text']=='saved 😀' and initial['launcher.scroll.y']==42 and initial['launcher.open'] is True
            shot('reload-valid-'+renderer)
            source=bundle/'shell/legacy-ui/content.json';original=source.read_bytes();latencies=[]
            for i in range(5):
                offset=len(log);written=time.monotonic_ns();source.write_bytes(original.replace(b'Application 01',f'Application {i+30:02}'.encode()))
                result=wait('UI_PRESENTED generation=',offset)
                ns=int(re.search(r'monotonic_ns=(\d+)',result)[1]);latencies.append((ns-written)/1e6)
                state=next(json.loads(line.split(' ',1)[1]) for line in log[offset:] if line.startswith('UI_STATE '));assert state==initial
            offset=len(log);source.write_text('{\n broken')
            wait('UI_ERROR',offset);wait('UI_PRESENTED generation=',offset);shot('reload-error-'+renderer)
            assert client.poll() is None
            offset=len(log);behavior.write_text(behavior.read_text().replace('saved 😀','reset must not win').replace('= 42','= 0'))
            # Two dependent edits are invalid until the broken file is fixed.
            time.sleep(.2);assert client.poll() is None
            offset=len(log);written=time.monotonic_ns();source.write_bytes(original)
            wait('UI_COMMIT',offset);result=wait('UI_PRESENTED generation=',offset)
            state=next(json.loads(line.split(' ',1)[1]) for line in log[offset:] if line.startswith('UI_STATE '));assert state==initial
            shot('reload-recovered-'+renderer)
            print(f'RELOAD_PROOF renderer={renderer} edits={len(latencies)} file_to_presentation_ms={latencies} preserved_state=true broken_dependency_kept_old_ui=true recovered=true',flush=True)
        finally:stop(client);reader.join(timeout=5)
        if client.returncode!=0:raise RuntimeError(f'reload client exit {client.returncode}')

def sample_backend(env,raw,target,compositor):
    logs=[];condition=threading.Condition();path=raw/('backend-'+compositor+'.log')
    (raw/(compositor+'-ipc.jsonl')).write_text('')
    probe=subprocess.Popen([str(target/'nacre-shell'),'--backend-probe'],cwd=ROOT,env={**env,'BEAD_RECORD_IPC':str(raw/(compositor+'-ipc.jsonl'))},stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
    def drain():
        with path.open('w') as stream:
            for line in probe.stdout:
                stream.write(line);stream.flush()
                with condition:logs.append(line.rstrip());condition.notify_all()
    reader=threading.Thread(target=drain);reader.start();clients=[]
    def wait(predicate,offset=0):
        deadline=time.monotonic()+20
        with condition:
            while True:
                for line in logs[offset:]:
                    if line.startswith('BACKEND_SNAPSHOT '):
                        snapshot=json.loads(line.split(' ',1)[1])
                        if predicate(snapshot):return snapshot
                if probe.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Backend proof snapshot timeout; inspect ignored log')
                condition.wait(.05)
    def action(value):
        offset=len(logs);probe.stdin.write(json.dumps(value)+'\n');probe.stdin.flush()
        deadline=time.monotonic()+10
        with condition:
            while True:
                line=next((line for line in logs[offset:] if line.startswith('BACKEND_RESULT ')),None)
                if line:return json.loads(line.split(' ',1)[1])
                if probe.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Backend action timeout')
                condition.wait(.05)
    try:
        snapshot=wait(lambda s:s['connected']);assert snapshot['outputs'] and snapshot['workspaces']
        fixtures=raw/(compositor+'-fixtures');fixtures.mkdir(exist_ok=True)
        if compositor=='hyprland':
            for name in ['monitors','workspaces','clients','activewindow']:
                p=subprocess.run(['hyprctl','-j',name],env=env,capture_output=True,text=True,check=True,timeout=5);(fixtures/(name+'.json')).write_text(p.stdout)
        else:
            for name in ['outputs','workspaces','windows']:
                p=subprocess.run(['niri','msg','-j',name],env=env,capture_output=True,text=True,check=True,timeout=5);(fixtures/(name+'.json')).write_text(p.stdout)
        windows=subprocess.Popen(['quickshell','--path',str(ROOT/'spikes/backend-windows.qml')],cwd=ROOT,env=env,stdout=(raw/(compositor+'-windows.log')).open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        clients.append(windows);snapshot=wait(lambda s:len([w for w in s['windows'] if w['title'].startswith('Bead fixture')])==2)
        owned=[w for w in snapshot['windows'] if w['title'].startswith('Bead fixture')];latencies=[]
        focused=next(w['id'] for w in owned if w['focused'])
        owned.sort(key=lambda w:w['id']==focused)
        for i in range(20):
            result=action({'action':'focus-window','window':owned[i%2]['id']});assert result['ok'] and result['observed'];latencies.append(result['state_update_ms'])
        result=action({'action':'window-trash','window':owned[0]['id']});assert not result['ok'] and 'unsupported' in result['error']
        snapshot=wait(lambda s:len([w for w in s['windows'] if w['title'].startswith('Bead fixture')])==2)
        if compositor=='hyprland':
            offset=len(logs)
            subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({workspace=2}))'],env=env,check=True,capture_output=True,timeout=5)
            snapshot=wait(lambda s:any(w['index']==2 for w in s['workspaces']),offset)
        candidates=[w for w in snapshot['workspaces'] if owned[0]['workspace']!=w['id']]
        if candidates:
            moved=action({'action':'move-window','window':owned[0]['id'],'workspace':candidates[0]['id'],'follow':False});assert moved['ok'] and moved['observed']
            switched=action({'action':'switch-workspace','workspace':candidates[0]['id']});assert switched['ok'] and switched['observed']
            print('BACKEND_MOVE_SWITCH '+compositor+' pass',flush=True)
        assert candidates, 'Move proof requires a second existing workspace'
        for floating in [True,False]:
            result=action({'action':'set-floating','window':owned[0]['id'],'floating':floating});assert result['ok'] and result['observed']
        if compositor=='hyprland':
            for fullscreen in [True,False]:
                result=action({'action':'set-fullscreen','window':owned[0]['id'],'fullscreen':fullscreen});assert result['ok'] and result['observed']
            original_ids={w['id'] for w in owned};offset=len(logs)
            subprocess.run(['hyprctl','output','create','headless','bead-hotplug'],env=env,check=True,capture_output=True,timeout=5)
            added=wait(lambda s:any(o['name']=='bead-hotplug' for o in s['outputs']),offset)
            output=next(o for o in added['outputs'] if o['name']=='bead-hotplug')
            result=action({'action':'move-window-to-output','window':owned[0]['id'],'output':output['id'],'follow':True});assert result['ok'] and result['observed']
            assert original_ids.issubset({w['id'] for w in added['windows']})
            offset=len(logs);subprocess.run(['hyprctl','output','remove','bead-hotplug'],env=env,check=True,capture_output=True,timeout=5)
            removed=wait(lambda s:all(o['name']!='bead-hotplug' for o in s['outputs']),offset)
            assert original_ids.issubset({w['id'] for w in removed['windows']})
            print('BACKEND_HOTPLUG hyprland add/move/remove stable-window-IDs=true',flush=True)
        if os.environ.get('NACRE_REFERENCE'):
            import importlib.util
            spec=importlib.util.spec_from_file_location('backend_reference',ROOT/'spikes/backend-reference.py')
            reference=importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)
            reference.sample(env,raw,compositor,memory,drm_memory,ticks,stop)
        last=action({'action':'close-window','window':owned[0]['id']});assert last['ok'] and last['observed']
        wait(lambda s:all(w['id']!=owned[0]['id'] for w in s['windows']))
        print(f'BACKEND_PROOF {compositor} windows/focus/close/unsupported=true focus_state_update_p50_p99={sorted(latencies)[9]:.3f}/{sorted(latencies)[-1]:.3f}ms samples={len(latencies)}',flush=True)
        # UI proof uses the same actual event stream; it is not a protocol stub.
        ui=subprocess.Popen([str(target/'nacre-shell'),'--ui','backend/main.luau','--backend','auto','--renderer','auto','--namespace','bead-dev-backend','--trace-state','--no-keyboard-focus'],cwd=ROOT,env=env,stdout=(raw/(compositor+'-backend-ui.log')).open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        clients.append(ui);time.sleep(2)
        result=action({'action':'focus-window','window':owned[1]['id']});assert result['ok'] and result['observed']
        deadline=time.monotonic()+5
        while True:
            text=(raw/(compositor+'-backend-ui.log')).read_text()
            states=[json.loads(l[9:]) for l in text.splitlines() if l.startswith('UI_STATE ')]
            if states and states[-1].get('bead.focused_title')==owned[1]['title']:break
            if time.monotonic()>deadline:raise RuntimeError('Bound focused title missing in live Luau UI')
            time.sleep(.05)
        print('BACKEND_UI '+compositor+' bound-workspaces/title=true',flush=True)
        time.sleep(.5);subprocess.run(['grim',str(raw/(compositor+'-backend-ui.png'))],env=env,capture_output=True,timeout=5)
        # A full idle measurement is separate from action timing/fixture activity.
        before=ticks(ui.pid);begun=time.monotonic();time.sleep(2 if os.environ.get('BEAD_SKIP_IDLE') else 60);elapsed=time.monotonic()-begun
        m=memory(ui.pid);drm=drm_memory(ui.pid);cpu=100*(ticks(ui.pid)-before)/os.sysconf('SC_CLK_TCK')/elapsed
        print(f'BACKEND_IDLE {compositor} UI_CPU={cpu:.3f}% seconds={elapsed:.3f} PSS_plus_DRM={m["pss_mib"]+sum((drm or {}).values()):.3f}MiB PSS/USS={m["pss_mib"]:.3f}/{m["uss_mib"]:.3f} DRM={sum((drm or {}).values()):.3f}',flush=True)
    finally:
        for client in reversed(clients):stop(client)
        if probe.poll() is None:probe.stdin.write('stop\n');probe.stdin.flush()
        try:probe.wait(timeout=5)
        except subprocess.TimeoutExpired:stop(probe)
        reader.join(timeout=5)
    if probe.returncode!=0:raise RuntimeError('Backend probe exit not clean')

def sample_input(env,raw,target):
    with tempfile.TemporaryDirectory(prefix='bead-input-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        command=[str(target/'nacre-shell'),'--ui','ui/main.luau','--catalog',str(ROOT/'spikes/launcher-fixture.json'),'--renderer','auto','--namespace','bead-dev-input','--test-keyboard','--trace-state','--palette',str(ROOT/'spikes/palette-stress.json')]
        if env.get('HYPRLAND_INSTANCE_SIGNATURE'):command+=['--output','bead-bench']
        log=[];condition=threading.Condition();path=raw/'input-proof.log'
        client=subprocess.Popen(command,cwd=bundle,env={**env,"WAYLAND_DEBUG":"1"},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
        def drain():
            with path.open('w') as stream:
                for line in client.stdout:
                    stream.write(line);stream.flush()
                    with condition:log.append(line.rstrip());condition.notify_all()
        reader=threading.Thread(target=drain);reader.start()
        def wait(marker,offset=0):
            deadline=time.monotonic()+20
            with condition:
                while True:
                    found=next((l for l in log[offset:] if marker in l),None)
                    if found:return found
                    if client.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Input proof missing '+marker)
                    condition.wait(.05)
        def state():return json.loads(next(l.split(' ',1)[1] for l in reversed(log) if l.startswith('UI_STATE ')))
        def type_command(*args):subprocess.run(['wtype','-s','300',*args,'-s','300'],env=env,capture_output=True,text=True,check=True,timeout=10);time.sleep(.2)
        try:
            wait('UI_PRESENTED generation=1');time.sleep(.5)
            if env.get('HYPRLAND_INSTANCE_SIGNATURE'):subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,capture_output=True,check=True,timeout=5)
            type_command('-M','shift','-k','Tab','-m','shift');assert state()['bead.focused'].endswith('.search')
            type_command('saved');assert state()['launcher.search.text']=='saved'
            original=(bundle/'shell/ui/main.luau').read_text();offset=len(log)
            (bundle/'shell/ui/main.luau').write_text(original.replace('text="Applications"','text="Applications · reloaded"'))
            wait('UI_COMMIT',offset);assert state()['launcher.search.text']=='saved' and state()['bead.focused'].endswith('.search')
            type_command('-M','ctrl','-k','a','-m','ctrl','after');assert state()['launcher.search.text']=='after'
            type_command('-M','ctrl','-k','a','-k','c','-m','ctrl')
            type_command('-M','ctrl','-k','a','-m','ctrl','x');type_command('-M','ctrl','-k','a','-k','v','-m','ctrl');assert state()['launcher.search.text']=='after'
            offset=len(log);(bundle/'shell/ui/main.luau').write_text('return broken source')
            wait('UI_ERROR',offset);type_command('-k','BackSpace');assert state()['launcher.search.text']=='afte'
            offset=len(log);(bundle/'shell/ui/main.luau').write_text(original);wait('UI_COMMIT',offset);assert state()['launcher.search.text']=='afte'
            # Activate the bridge only on this task's private D-Bus.
            activation=subprocess.run(['gdbus','call','--session','--dest','org.a11y.Bus','--object-path','/org/a11y/bus','--method','org.freedesktop.DBus.Properties.Set','org.a11y.Status','IsEnabled','<true>'],env=env,capture_output=True,text=True,timeout=5)
            if activation.returncode:print('AT-SPI private activation error:',activation.stderr[:250],flush=True)
            print(f'INPUT_PROOF native_keyboard=true focus/type/select-all/backspace/reload/error-recovery=true text_input_v3={any("IME_PROTOCOL" in l for l in log)} atspi_activation_exit={activation.returncode}',flush=True)
            subprocess.run(['grim',str(raw/'input-proof.png')],env=env,capture_output=True,timeout=5)
        finally:stop(client);reader.join(timeout=5)
        if client.returncode!=0:raise RuntimeError('Input proof did not stop cleanly')

def sample_gate_b(env, raw, target, renderer):
    with tempfile.TemporaryDirectory(prefix='bead-gate-b-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        source=bundle/'shell/ui/main.luau';original=source.read_text()
        catalog=ROOT/'spikes/launcher-fixture.json'
        command=[str(target/'nacre-shell'),'--ui','ui/main.luau','--catalog',str(catalog),'--renderer',renderer,'--namespace','bead-dev-gate-b','--palette',str(ROOT/'spikes/palette-stress.json')]
        if env.get('HYPRLAND_INSTANCE_SIGNATURE'):command+=['--output','bead-bench']
        log=[];condition=threading.Condition();path=raw/('gate-b-'+renderer+'.log')
        client=subprocess.Popen(command,cwd=bundle,env={**env,'WAYLAND_DEBUG':'0'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
        def drain():
            with path.open('w') as stream:
                for line in client.stdout:
                    stream.write(line);stream.flush()
                    with condition:log.append(line.rstrip());condition.notify_all()
        reader=threading.Thread(target=drain);reader.start()
        def wait(marker,offset=0):
            deadline=time.monotonic()+20
            with condition:
                while True:
                    found=next((line for line in log[offset:] if marker in line),None)
                    if found:return found
                    if client.poll() is not None or time.monotonic()>deadline:raise RuntimeError('GateB missing '+marker+'; inspect ignored raw log')
                    condition.wait(.05)
        def counts():
            line=next(line for line in reversed(log) if line.startswith('UI_RESOURCES'))
            values=dict((key,int(value)) for key,value in re.findall(r'(\w+)=(\d+)',line))
            platform=next((l for l in reversed(log) if l.startswith('UI_PLATFORM')), '')
            values.update(('platform_'+k,int(v)) for k,v in re.findall(r'(\w+)=(\d+)',platform))
            return values
        def mem():
            values=memory(client.pid);drm=drm_memory(client.pid)
            return values['pss_mib']+sum((drm or {}).values()),values['pss_mib'],sum((drm or {}).values())
        try:
            wait('UI_PRESENTED generation=1');time.sleep(2)
            # Prewarm both source strings before the reference sample, then do100 measured edits.
            for i in range(4):
                offset=len(log);source.write_text(original.replace('text="Applications"', 'text="Applications · reloaded"') if i%2 else original)
                wait('UI_COMMIT',offset);wait('UI_PRESENTED generation=',offset)
            time.sleep(1);initial_counts=counts();before=mem();before_fds=len(list(pathlib.Path(f'/proc/{client.pid}/fd').iterdir()))
            samples=[];latencies=[]
            for i in range(100):
                offset=len(log);written=time.monotonic_ns()
                source.write_text(original.replace('text="Applications"','text="Applications · reloaded"') if i%2 else original)
                wait('UI_COMMIT',offset);line=wait('UI_PRESENTED generation=',offset)
                presented=int(re.search(r'monotonic_ns=(\d+)',line)[1]);latencies.append((presented-written)/1e6)
                assert counts()==initial_counts,'owned resources grew or subscriptions changed'
                if i%10==9:samples.append(mem()[0])
            time.sleep(1);after=mem();after_fds=len(list(pathlib.Path(f'/proc/{client.pid}/fd').iterdir()))
            assert before_fds==after_fds,'FD count grew'
            # PSS/DRM include allocator and driver pools, so report real drift rather than fabricate exact zero.
            print(f'GATE_B reloads=100 renderer={renderer} before_PSS_plus_DRM={before[0]:.3f} after={after[0]:.3f} delta={after[0]-before[0]:.3f}MiB PSS_before_after={before[1]:.3f}/{after[1]:.3f} DRM_before_after={before[2]:.3f}/{after[2]:.3f} FDs={before_fds}/{after_fds} counts={initial_counts} ten_reload_samples={samples} latency_p50_p99={sorted(latencies)[49]:.3f}/{sorted(latencies)[98]:.3f}ms',flush=True)
            if after[0]-before[0]>2:print('GATE_B verdict=concern memory drift exceeds2MiB; investigate, not a no-growth claim',flush=True)
            else:print('GATE_B verdict=pass owned counts/FDs stable; memory bounded within2MiB measurement tolerance',flush=True)
        finally:stop(client);reader.join(timeout=5)
        if client.returncode!=0:raise RuntimeError('GateB client did not exit cleanly')

def sample_qml_reload(env,raw):
    with tempfile.TemporaryDirectory(prefix='bead-qml-ui-') as directory:
        fixture=pathlib.Path(directory)/'shell.qml'
        source=(ROOT/'spikes/panel-reload.qml').read_text().replace('property string searchText: ""','property string searchText: "saved 😀"').replace('property real scrollY: 0','property real scrollY: 42')
        fixture.write_text(source)
        env.update({'LD_PRELOAD':os.environ['BEAD_QML_PROBE'],'WAYLAND_DEBUG':'0','BEAD_IDLE':'1','BEAD_CONTENT_SIZE':'1','BEAD_LAYER':'1','BEAD_PALETTE_PATH':str(ROOT/'spikes/palette-stress.json'),'BEAD_SHELL_DIR':str(ROOT/'shell'),'BEAD_ICON_PATH':'/usr/share/icons/Adwaita/scalable/places/folder.svg'})
        client=subprocess.Popen(['quickshell','--path',str(fixture)],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
        log=[];condition=threading.Condition()
        def drain():
            with (raw/'qml-reload.log').open('w') as output:
                for line in client.stdout:
                    output.write(line);output.flush()
                    with condition:log.append(line.strip());condition.notify_all()
        reader=threading.Thread(target=drain);reader.start()
        def wait(marker,offset=0):
            deadline=time.monotonic()+15
            with condition:
                while True:
                    found=next((line for line in log[offset:] if marker in line),None)
                    if found:return found
                    if client.poll() is not None or time.monotonic()>deadline:raise RuntimeError('QML reload proof failed: '+marker)
                    condition.wait(.1)
        try:
            wait('QML_UI_PRESENTED generation=1 ');latencies=[]
            for generation in range(2,7):
                offset=len(log);written=time.monotonic_ns()
                fixture.write_text(source.replace('testGeneration: 1',f'testGeneration: {generation}').replace('Application 01',f'Application {generation+30:02}').replace('saved 😀','reset should lose').replace('scrollY: 42','scrollY: 0'))
                result=wait(f'QML_UI_PRESENTED generation={generation} ',offset)
                ns=int(re.search(r'monotonic_ns=(\d+)',result)[1]);latencies.append((ns-written)/1e6)
                state=json.loads(wait('QML_STATE ',offset).split('QML_STATE ',1)[1]);assert state=={'open':True,'search':'saved 😀','scroll':42}
            offset=len(log);fixture.write_text('import QtQuick\nShellRoot { broken syntax ???')
            time.sleep(.4);assert client.poll() is None
            offset=len(log);fixture.write_text(source.replace('testGeneration: 1','testGeneration: 7'))
            wait('QML_UI_PRESENTED generation=7 ',offset)
            print(f'QML_RELOAD_PROOF edits={len(latencies)} file_to_presentation_ms={latencies} persistent_properties=true syntax_error_kept_running=true recovered=true',flush=True)
        finally:stop(client);reader.join(timeout=5)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seconds', type=float, default=60)
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--compositor', choices=['niri','hyprland'], default='niri')
    p.add_argument('--parent', choices=['headless','desktop'], default='headless')
    p.add_argument('--bar-ui', action='store_true')
    p.add_argument('--physical-pacing', action='store_true', help='Explicitly approved passive niri pacing acceptance only')
    p.add_argument('--bar-reference', action='store_true')
    p.add_argument('--lifecycle', action='store_true')
    p.add_argument('--palette', type=pathlib.Path, default=None)
    p.add_argument('--qml-reload-test',action='store_true')
    p.add_argument('--demo-test',action='store_true',help='Smoke-test the owner demo inside the private compositor')
    p.add_argument('--reload-test',action='store_true',help='Private temporary UI bundle reload/state/error proof')
    p.add_argument('--retained', action='store_true', help='One round of retained CPU/femtovg/Vello; private compositor only')
    p.add_argument('--layouts',nargs='+',choices=['full','content'],default=['full','content'])
    p.add_argument('--renderer', choices=['cpu','femtovg','femtovg-gl','vello','auto'],default='cpu')
    p.add_argument('--animate', action='store_true')
    p.add_argument('--content-size', action='store_true')
    p.add_argument('--virtual120', action='store_true', help='Isolated Hyprland virtual 2880x1800/1.5/120Hz output')
    p.add_argument('--panel', action='store_true', help='Static Feature 2 loaded-font panel, nested only')
    p.add_argument('--modes', nargs='+', default=None)
    p.add_argument('--in-private-bus', action='store_true')
    args = p.parse_args()
    if args.physical_pacing and not (args.bar_ui and args.parent == 'desktop' and args.compositor == 'niri' and os.environ.get('BEAD_PHYSICAL_NIRI_APPROVED') == '1'):
        p.error('Physical pacing requires --bar-ui --parent desktop --compositor niri and explicit owner approval')
    if args.modes is None:args.modes=['cpu','femtovg','vello','qml'] if args.retained else ['tiny-skia','vello','femtovg','qml']
    if (args.retained or args.reload_test or args.qml_reload_test or args.demo_test) and args.parent!='headless':p.error('Retained comparison is private only; live desktop needs separate owner approval')
    if not args.in_private_bus:
        # D-Bus activation inherits its daemon's environment: isolate XDG BEFORE
        # starting the bus, not just before launching its clients.
        with tempfile.TemporaryDirectory(prefix='bead-bus-') as directory:
            env=os.environ.copy()
            display=env.get('WAYLAND_DISPLAY','')
            if display and not display.startswith('/'): display=str(pathlib.Path(env['XDG_RUNTIME_DIR'])/display)
            env['BEAD_PARENT_WAYLAND']=display
            env['BEAD_PARENT_NIRI']=env.get('NIRI_SOCKET', '')
            env['BEAD_FONT_DIR']=str(pathlib.Path(os.environ.get('XDG_DATA_HOME',str(pathlib.Path.home()/'.local/share')))/'fonts')
            for key, name in [('XDG_RUNTIME_DIR','runtime'),('XDG_CONFIG_HOME','config'),('XDG_CACHE_HOME','cache'),('XDG_STATE_HOME','state'),('XDG_DATA_HOME','data')]:
                path=pathlib.Path(directory)/name
                path.mkdir(mode=0o700)
                env[key]=str(path)
            env['XDG_DATA_DIRS']='/usr/local/share:/usr/share'
            return subprocess.call(['dbus-run-session', '--', sys.executable, __file__, '--in-private-bus', *(['--physical-pacing'] if args.physical_pacing else []), '--seconds', str(args.seconds), '--runs', str(args.runs), '--compositor',args.compositor,'--parent',args.parent, *(['--bar-ui'] if args.bar_ui else []),*(['--bar-reference'] if args.bar_reference else []),*(['--lifecycle'] if args.lifecycle else []),*(['--retained'] if args.retained else []),*(['--reload-test'] if args.reload_test else []),*(['--qml-reload-test'] if args.qml_reload_test else []),*(['--demo-test'] if args.demo_test else []),'--layouts',*args.layouts,'--renderer',args.renderer,*(['--animate'] if args.animate else []),*(['--content-size'] if args.content_size else []),*(['--virtual120'] if args.virtual120 else []),*(['--panel'] if args.panel else []),*(['--palette',str(args.palette)] if args.palette else []),'--modes', *args.modes],env=env)
    raw = pathlib.Path(os.environ.get('BEAD_RAW_DIR', str(ROOT / 'spikes/results/raw')))
    if args.retained:raw=raw/('retained-'+args.compositor)
    elif 'bead' in args.modes: raw=raw/(args.compositor+'-panel' if args.panel else args.compositor)
    raw.mkdir(parents=True, exist_ok=True)
    save_baseline = not (args.bar_reference or args.bar_ui) and 'bead' not in args.modes and not (args.retained or args.reload_test or args.qml_reload_test or args.demo_test)
    tools = pathlib.Path(os.environ.get('BEAD_RESEARCH_TOOLS',str(ROOT/'.tools')))
    result = {'date': '2026-10-13', 'surface': {'width': 512, 'height': 32, 'scale': 1, 'outputs': 1, 'namespace': 'bead-dev-spike'},
              'method': 'client /proc smaps_rollup; CPU ticks over monotonic interval; READY is frame callback for Rust, QML component completion (not comparable first-frame); separate nested niri/private D-Bus/XDG', 'runs': []}
    # Hyprland's instance signature consumes most of Unix's 108-byte socket path.
    with tempfile.TemporaryDirectory(prefix='bd-') as directory:
        private = pathlib.Path(directory)
        env = os.environ.copy()
        for key, name in [('XDG_RUNTIME_DIR','r'), ('XDG_CONFIG_HOME','config'), ('XDG_CACHE_HOME','cache'), ('XDG_STATE_HOME','state'), ('XDG_DATA_HOME','data')]:
            path = private / name
            path.mkdir(mode=0o700)
            env[key] = str(path)
        env['XDG_DATA_DIRS'] = '/usr/local/share:/usr/share'
        for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY']:
            env.pop(key, None)
        env['WGPU_BACKEND'] = 'vulkan'
        env['WGPU_POWER_PREF'] = 'low'
        env['QSG_INFO']='1'
        from xml.sax.saxutils import escape
        fonts=private/'fonts.conf'
        fonts.write_text('<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd"><fontconfig><include ignore_missing="yes">/etc/fonts/fonts.conf</include><dir>'+escape(env.get('BEAD_FONT_DIR','/usr/share/fonts'))+'</dir></fontconfig>')
        env['FONTCONFIG_FILE']=str(fonts)
        if args.physical_pacing:
            # Reuse the private bus/XDG/font containment above; only the two
            # explicitly authorized compositor endpoints refer to the desktop.
            for key in ['BEAD_RECORD_IPC', 'BEAD_TRACE_BACKEND', 'BEAD_SERVICE_DIAGNOSTICS', 'WAYLAND_DEBUG']:
                env.pop(key, None)
            os.environ.pop('BEAD_GL_DEBUG', None)
            os.environ.pop('BEAD_PANEL_RENDERER', None)
            env['WAYLAND_DISPLAY'] = env.get('BEAD_PARENT_WAYLAND', '')
            env['NIRI_SOCKET'] = env.get('BEAD_PARENT_NIRI', '')
            if not env['WAYLAND_DISPLAY'] or not env['NIRI_SOCKET']:
                raise RuntimeError('Physical niri endpoints unavailable')
            outputs = json.loads(subprocess.check_output(['niri', 'msg', '-j', 'outputs'], env=env, text=True, timeout=5))
            selected = []
            for name, output in outputs.items():
                index = output.get('current_mode')
                if index is None: continue
                mode = output['modes'][index]
                if mode['width'] == 2880 and mode['height'] == 1800 and output.get('logical', {}).get('scale') == 1.5:
                    selected.append((name, mode['refresh_rate']/1000))
            if len(selected) != 1:
                raise RuntimeError('Expected exactly one active 2880x1800/1.5 physical output')
            name, hz = selected[0]
            print('AUTHORIZED_PASSIVE_PACING size=2880x1800 scale=1.5 hz='+str(hz), flush=True)
            import importlib.util
            if os.environ.get('BEAD_HARDENING') == 'panel-pacing':
                os.environ['BEAD_PROOF_OUTPUT'] = name
                spec = importlib.util.spec_from_file_location('panel_pacing', ROOT/'spikes/panel-pacing.py')
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                module.sample(env, raw, 'niri', stop)
                return 0
            os.environ.update({'BEAD_PROOF_OUTPUT': name, 'BEAD_PROOF_HZ': str(hz), 'BEAD_TEST_PASSIVE': '1', 'BEAD_PACING_ONLY': '1', 'BEAD_PACING_CASES': 'cpu,femtovg-gl'})
            import importlib.util
            spec = importlib.util.spec_from_file_location('hardening', ROOT/'spikes/hardening.py')
            module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
            module.sample(env, raw, 'niri', stop)
            return 0
        # An offscreen headless parent avoids visible preview-window closure
        # interrupting unattended measurements. niri remains the client-facing
        # compositor; the labwc process is outside measured client PSS/CPU.
        parent_runtime = private / 'parent-runtime'
        parent_runtime.mkdir(mode=0o700)
        parent_config = private / 'labwc'
        parent_config.mkdir()
        (parent_config/'autostart').write_text('')
        if os.environ.get('BEAD_BAR_MATRIX') and args.compositor == 'niri':
            (parent_config/'rc.xml').write_text('<labwc_config><windowRules><windowRule identifier="*"><decorations>no</decorations><action name="ResizeTo" width="2880" height="1800"/><action name="MoveTo" x="0" y="0"/></windowRule></windowRules></labwc_config>')
        parent_env = env.copy()
        parent_env.update({'XDG_RUNTIME_DIR':str(parent_runtime), 'WLR_BACKENDS':'headless', 'WLR_HEADLESS_OUTPUTS':'1', 'WLR_RENDERER':'gles2',
                           'LD_LIBRARY_PATH':str(tools/'native/usr/lib')})
        parent_env.pop('WAYLAND_DISPLAY', None)
        parent_log = (raw/'labwc.log').open('w')
        parent=None
        if args.parent=='desktop':
            env['WAYLAND_DISPLAY']=env['BEAD_PARENT_WAYLAND']
            if not env['WAYLAND_DISPLAY']: raise RuntimeError('No parent Wayland desktop')
        else:
            parent = subprocess.Popen([shutil.which('labwc') or str(tools/'native/usr/bin/labwc'), '--config-dir', str(parent_config)],env=parent_env,stdout=parent_log,stderr=parent_log,start_new_session=True)
            deadline = time.monotonic()+20
            while time.monotonic()<deadline:
                sockets = [f for f in parent_runtime.glob('wayland-*') if stat.S_ISSOCK(f.stat().st_mode)]
                if sockets:
                    env['WAYLAND_DISPLAY']=str(sockets[0])
                    break
                if parent.poll() is not None:
                    raise RuntimeError('Offscreen parent exited; inspect ignored labwc.log')
                time.sleep(0.1)
            else:
                stop(parent)
                raise RuntimeError('Offscreen parent did not start')
        log = (raw / f'{args.compositor}.log').open('w')
        if args.compositor=='niri':
            command=['niri','--config',str(ROOT/'spikes/niri.kdl')]
        else:
            env['AQ_DRM_DEVICES']='/dev/null'
            command=['Hyprland','--config',str(ROOT/'spikes/hyprland.lua')]
        compositor_launch_env = env.copy()
        compositor = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
        client = None
        try:
            deadline = time.monotonic() + 20
            runtime = pathlib.Path(env['XDG_RUNTIME_DIR'])
            while time.monotonic() < deadline:
                sockets = [f for f in runtime.glob('wayland-*') if stat.S_ISSOCK(f.stat().st_mode)]
                if sockets:
                    env['WAYLAND_DISPLAY'] = str(sockets[0])
                    break
                if compositor.poll() is not None:
                    raise RuntimeError('Nested niri exited; inspect ignored spikes/results/raw/niri.log')
                time.sleep(0.1)
            else:
                raise RuntimeError('Nested niri did not create a Wayland socket')
            if args.virtual120:
                if args.compositor!='hyprland':raise RuntimeError('virtual120 needs isolated Hyprland')
                signatures=[]
                for _ in range(100):
                    signatures=list(runtime.glob('hypr/*/.socket.sock'))
                    if signatures:break
                    if compositor.poll() is not None:break
                    time.sleep(.05)
                if not signatures:raise RuntimeError('No private Hyprland IPC socket; inspect private compositor log')
                env['HYPRLAND_INSTANCE_SIGNATURE']=signatures[0].parent.name
                subprocess.run(['hyprctl','output','create','headless','bead-bench'],env=env,check=True,capture_output=True,timeout=5)
                configured=subprocess.run(['hyprctl','eval','hl.monitor({output="bead-bench",mode="2880x1800@120",position="0x0",scale=1.5})'],env=env,check=True,capture_output=True,text=True,timeout=5)
                print('Private virtual monitor configuration:',configured.stdout.strip(),flush=True)
                time.sleep(1)
                monitors=json.loads(subprocess.check_output(['hyprctl','-j','monitors'],env=env,text=True,timeout=5))
                print('Private outputs:',[{k:m.get(k) for k in ['name','width','height','refreshRate','scale']}for m in monitors],flush=True)
            info=subprocess.run(['wayland-info'],env=env,capture_output=True,text=True,timeout=10,check=True)
            (raw/'wayland-info.log').write_text(info.stdout)
            result['globals']=[{'interface':name,'version':int(version)} for name,version in re.findall(r"interface: '([^']+)',\s*version:\s*(\d+)",info.stdout)]
            sockets=list(runtime.glob('niri.*.sock')) if args.compositor=='niri' else []
            if sockets:
                env['NIRI_SOCKET']=str(sockets[0])
                outputs=subprocess.run(['niri','msg','--json','outputs'],env=env,capture_output=True,text=True,timeout=10,check=True)
                result['output_properties']=json.loads(outputs.stdout)
            if args.compositor=='hyprland':
                sockets=list(runtime.glob('hypr/*/.socket.sock'))
                if sockets: env['HYPRLAND_INSTANCE_SIGNATURE']=sockets[0].parent.name
            if os.environ.get('BEAD_BAR_MATRIX') and args.compositor=='niri':
                name=next(iter(result['output_properties']))
                subprocess.run(['niri','msg','output',name,'scale','1.5'],env=env,capture_output=True,check=True,timeout=5)
                time.sleep(1)
                outputs=json.loads(subprocess.check_output(['niri','msg','-j','outputs'],env=env,text=True,timeout=5))
                print('BAR_MATRIX_OUTPUTS '+json.dumps(outputs),flush=True)
                output=next(iter(outputs.values()))
                mode=output['modes'][output['current_mode']]
                assert (mode['width'],mode['height'])==(2880,1800), outputs
                assert output['logical']['width']==1920,outputs
            if args.bar_ui:
                import importlib.util
                spec=importlib.util.spec_from_file_location("bar_ui",ROOT/"spikes/bar-ui.py")
                module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
                module.sample(env,raw,args.compositor,stop)
                return 0
            if args.bar_reference:
                import importlib.util
                spec=importlib.util.spec_from_file_location('bar_reference',ROOT/'spikes/bar-reference.py')
                reference=importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)
                reference.sample(env,raw,args.compositor,stop)
                return 0
            if args.demo_test:
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                env['PATH']=str(target)+os.pathsep+env['PATH']
                env.pop('NIRI_SOCKET',None);env.pop('HYPRLAND_INSTANCE_SIGNATURE',None)
                with (raw/('demo-'+args.compositor+'.log')).open('w') as demo_log:
                    demo=subprocess.Popen([sys.executable,str(ROOT/'scripts/demo.py'),args.compositor],env=env,cwd=ROOT,stdout=demo_log,stderr=demo_log,start_new_session=True)
                    try:
                        time.sleep(27)
                        if demo.poll() is not None:raise RuntimeError('Demo exited early')
                    finally:stop(demo)
                text=(raw/('demo-'+args.compositor+'.log')).read_text()
                for marker in ['DEMO: valid','DEMO: broken','DEMO: recovered','UI_COMMIT','UI_ERROR']:
                    if marker not in text:raise RuntimeError('Demo missing '+marker+'; inspect ignored raw log')
                print('DEMO_PROOF nested='+args.compositor+' valid/broken/recovered=true',flush=True)
                return 0
            if args.qml_reload_test:
                sample_qml_reload(env,raw);return 0
            if args.reload_test and os.environ.get('BEAD_BACKEND_TEST'):
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                sample_backend(env,raw,target,args.compositor)
                import importlib.util
                spec=importlib.util.spec_from_file_location('backend_restart',ROOT/'spikes/backend-restart.py')
                restart=importlib.util.module_from_spec(spec);spec.loader.exec_module(restart)
                compositor=restart.sample(env,compositor_launch_env,command,compositor,target,raw,stop)
                return 0
            if args.reload_test and os.environ.get('BEAD_INPUT_TEST'):
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                sample_input(env,raw,target);return 0
            if args.reload_test and os.environ.get('BEAD_GATE_B'):
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                sample_gate_b(env,raw,target,args.renderer);return 0
            if args.reload_test:
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                sample_reload(env,raw,target,args.renderer);return 0
            if args.retained:
                target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                for run in range(args.runs):
                    for layout in args.layouts:
                        for mode in args.modes:
                            case_env=env.copy();cache=private/'renderer-cache'/(mode+'-'+layout);cache.mkdir(parents=True,exist_ok=True);case_env['XDG_CACHE_HOME']=str(cache)
                            sample_retained(case_env,raw,target,mode,layout,run,args.seconds,args.virtual120)
                return 0
            for run in range(args.runs):
                for mode in args.modes:
                    target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
                    fixture='panel.qml' if args.panel else ('blank-outputs.qml' if 'bead' in args.modes else 'blank.qml')
                    command = ['quickshell', '--log-rules', 'qml.debug=true', '--path', str(ROOT/'spikes'/fixture)] if mode == 'qml' else ([str(target/'nacre-shell')] if mode=='bead' else [str(target/'bead-spike-blank-layer'), mode])
                    if args.panel:
                        palette_path=(args.palette or ROOT/'spikes/palette-stress.json').resolve()
                        env.update({'BEAD_IDLE':'0' if args.animate else '1','BEAD_PALETTE_PATH':str(palette_path),'BEAD_SHELL_DIR':str(ROOT/'shell'),'BEAD_ICON_PATH':'/usr/share/icons/Adwaita/scalable/places/folder.svg'})
                        if mode=='bead':
                            command+=['--animate' if args.animate else '--panel','--palette',str(palette_path),'--renderer',args.renderer]
                            if args.virtual120:command+=['--output','bead-bench']
                            if args.content_size:command+=['--content-size']
                    env['WAYLAND_DEBUG']='1' if 'bead' in args.modes else '0'
                    started = time.monotonic()
                    client = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True, bufsize=0)
                    data, ready_at = b'', None
                    frames=set()
                    trace_cursor=0
                    deadline = started + 60
                    while time.monotonic() < deadline:
                        if select.select([client.stdout], [], [], 0.1)[0]:
                            chunk = os.read(client.stdout.fileno(), 65536)
                            data += chunk
                            if 'bead' in args.modes:
                                ready=False
                                # Callback IDs can be reused after a registry sync.
                                # Only a done event AFTER its frame request counts.
                                for line in data[trace_cursor:].splitlines(keepends=True):
                                    if not line.endswith(b'\n'): break
                                    trace_cursor+=len(line)
                                    request=re.search(rb'wl_surface[@#]\d+\.frame\((?:new id )?wl_callback[@#](\d+)',line)
                                    done=re.search(rb'wl_callback[@#](\d+)\.done(?:,\s*)?\(',line)
                                    if request: frames.add(request[1])
                                    if done and done[1] in frames: ready=True
                            else:
                                ready=b'READY' in data
                            if ready:
                                ready_at = time.monotonic()
                                break
                            if not chunk:
                                break
                    (raw/f'{mode}-{run}.log').write_bytes(data)
                    if ready_at is None:
                        result['runs'].append({'renderer':mode, 'run':run, 'error':'No READY marker; see ignored log'})
                        print(f'{mode} run {run}: FAILED', flush=True)
                        stop(client)
                        continue
                    time.sleep(5)
                    before_mem, before_ticks, before = memory(client.pid), ticks(client.pid), time.monotonic()
                    (raw/f'{mode}-{run}-before.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
                    # The parent waits; progress lines identify the active interval.
                    print(f'{mode} run {run}: ready, sampling idle CPU for {args.seconds:g}s', flush=True)
                    deadline = before + args.seconds
                    while time.monotonic() < deadline:
                        if client.poll() is not None:
                            data += client.stdout.read()
                            (raw/f'{mode}-{run}.log').write_bytes(data)
                            result['runs'].append({'renderer':mode, 'run':run, 'error':f'Exited during idle interval: {client.returncode}', 'compositor_exit':compositor.poll()})
                            raise RuntimeError(f'{mode} exited during idle interval: {client.returncode}; compositor status: {compositor.poll()}')
                        time.sleep(min(1, deadline-time.monotonic()))
                    elapsed = time.monotonic() - before
                    after_ticks, after_mem = ticks(client.pid), memory(client.pid)
                    (raw/f'{mode}-{run}-after.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
                    drm=[]
                    for fd in pathlib.Path(f'/proc/{client.pid}/fdinfo').glob('*'):
                        try:
                            lines=[line for line in fd.read_text().splitlines() if line.startswith('drm-')]
                            if lines:drm.append(lines)
                        except OSError:
                            pass
                    row_gpu={'fdinfo':drm,'note':'driver accounting where available; PSS does not include all GPU allocations'}
                    row = {'renderer':mode, 'run':run, 'ready_wall_ms':(ready_at-started)*1000, 'before':before_mem, 'after':after_mem,
                           'idle_seconds':elapsed, 'cpu_ticks':after_ticks-before_ticks,
                           'idle_cpu_one_core_percent':100*(after_ticks-before_ticks)/os.sysconf('SC_CLK_TCK')/elapsed,
                           'ready_kind':'wl_surface frame callback; not presentation timestamp' if 'bead' in args.modes else ('QML component completion' if mode=='qml' else 'renderer ready')}
                    row['gpu']=row_gpu
                    result['runs'].append(row)
                    if save_baseline: (ROOT/'spikes/results/baseline.json').write_text(json.dumps(result,indent=2)+'\n')
                    print(f"{mode} run {run}: PSS {after_mem['pss_mib']:.2f} MiB / USS {after_mem['uss_mib']:.2f} MiB / CPU {row['idle_cpu_one_core_percent']:.3f}% / SwapPss {after_mem['swap_pss_mib']:.2f} MiB", flush=True)
                    if mode=='bead' and args.lifecycle:
                        if args.compositor=='hyprland':
                            for action in ['create','remove','create','remove']:
                                command=['hyprctl','output',action]+(['headless'] if action=='create' else [])+['bead-test']
                                changed=subprocess.run(command,env=env,capture_output=True,text=True,timeout=5,check=True)
                                print('Nested output '+action+': '+changed.stdout.strip(),flush=True)
                                time.sleep(0.5)
                        else:
                            name=next(iter(result['output_properties']))
                            for scale in ['2','1']:
                                subprocess.run(['niri','msg','output',name,'scale',scale],env=env,capture_output=True,text=True,timeout=5,check=True)
                                time.sleep(0.5)
                    try:
                        subprocess.run(['grim', '-t', 'png', str(raw/f'{mode}-{run}.png')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False,timeout=5)
                    except subprocess.TimeoutExpired:
                        print(f'{args.compositor} {mode}: screenshot timed out; measurements retained',flush=True)
                    stop(client)
                    print(f'{args.compositor} {mode}: startup/frame callback {row["ready_wall_ms"]:.2f} ms; exit={client.returncode}',flush=True)
                    data+=client.stdout.read()
                    (raw/f'{mode}-{run}.log').write_bytes(data)
                    if mode=='bead':
                        assert client.returncode==0, 'Bead did not shut down cleanly'
                        assert b'STOP surfaces=0' in data, 'Missing resource cleanup marker'
                        if args.lifecycle and args.compositor=='hyprland':
                            assert data.count(b'READY output=')>=3 and data.count(b'OUTPUT_REMOVED')>=2, 'Hotplug lifecycle did not map/release every output'
                        if args.lifecycle and args.compositor=='niri':
                            assert b'set_buffer_scale(2)' in data and b'set_buffer_scale(1)' in data, 'Scale change did not redraw'
                        if args.lifecycle: print(args.compositor+' lifecycle: PASS',flush=True)
                    client = None
            return 0
        finally:
            for detail in pathlib.Path(env['XDG_RUNTIME_DIR']).glob('hypr/*/hyprland.log'):
                shutil.copyfile(detail,raw/'hyprland-detail.log')
            if client is not None:
                stop(client)
            stop(compositor)
            if parent is not None: stop(parent)
            log.close()
            parent_log.close()
            if save_baseline: (ROOT/'spikes/results/baseline.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__ == '__main__':
    raise SystemExit(main())
