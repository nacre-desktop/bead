#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Real native private notification server and both view families, nested only."""
import importlib.util,json,os,pathlib,re,signal,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release'
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(target/'nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    path=raw/'notifications.log';client=None;pointer=None;interactive=None;owned_daemons=set()
    with tempfile.TemporaryDirectory(prefix='bn-') as directory:
        private=pathlib.Path(directory);socket=private/'bus';history=private/'history.json'
        command=[binary,'--notification-bus',str(socket),'--notification-history',str(history),'--command-socket',str(private/'cmd'),'--backend','none','--trace-state','--namespace','bead-notifications']
        if compositor=='hyprland':command+=['--output','bead-bench']
        def ambient_owner():
            return subprocess.run(['gdbus','call','--session','--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.NameHasOwner','org.freedesktop.Notifications'],env=env,check=True,capture_output=True,text=True,timeout=5).stdout
        before_owner=ambient_owner()
        if compositor=='hyprland':
            # Only this harness's private instance; keep startup banners out of pixels.
            outputs=json.loads(subprocess.run(['hyprctl','-j','monitors'],env=env,capture_output=True,text=True,check=True,timeout=5).stdout)
            assert all(o['name']=='bead-bench' or o['name'].startswith('HEADLESS') for o in outputs)
            for command_name,args in [('dismissnotify',['-1']),('seterror',['disable'])]:
                subprocess.run(['hyprctl',command_name,*args],env=env,capture_output=True,check=True,timeout=5)
        try:
            with path.open('w') as log:
                client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                text=matrix.wait_log(path,lambda t:'NOTIFICATIONS_PRIVATE address=' in t and 'UI_PRESENTED' in t,timeout=30)
                text=matrix.log_text(path)
                address=re.search(r'NOTIFICATIONS_PRIVATE address=(\S+)',text)[1]
                daemon_reply=subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.GetConnectionUnixProcessID','org.freedesktop.DBus'],env=env,check=True,capture_output=True,text=True,timeout=5).stdout
                daemon_pid=int(re.search(r'uint32 (\d+)',daemon_reply)[1]);owned_daemons.add(daemon_pid)
                time.sleep(.3)
                idle=matrix.idle(client.pid,float(os.environ.get("BEAD_NOTIFICATION_IDLE_SECONDS","2")))
                fixture=subprocess.run([str(target/'examples/notification-client'),address],env=env,capture_output=True,text=True,timeout=20)
                (raw/'notification-client.log').write_text(fixture.stdout+fixture.stderr)
                assert fixture.returncode==0,(fixture.stdout+fixture.stderr)[-1600:]
                assert 'NOTIFICATION_PROTOCOL_PASS' in fixture.stdout
                text=matrix.wait_log(path,lambda t:'Visible notification' in t and '"osd.label":"Caps lock"' in t,timeout=10)
                text=matrix.log_text(path)
                assert 'UI_ERROR' not in text
                assert 'INPUT_ENTER surface=' not in text,'passive notice/OSD stole keyboard focus'
                last_state={};channels={};presented=set()
                for line in text.splitlines():
                    if line.startswith('UI_STATE '):last_state=json.loads(line[9:])
                    commit=re.search(r'UI_COMMIT .*update=(\d+) companion=true source_error=false',line)
                    if commit:channels.setdefault(last_state.get('osd.label',''),[]).append(int(commit[1]))
                    present=re.search(r'UI_PRESENTED .*companion=true update=(\d+)',line)
                    if present:presented.add(int(present[1]))
                for label in ['Volume','Display brightness','Microphone','Caps lock']:
                    assert any(update in presented for update in channels.get(label,[])),('missing exact OSD presentation',label,channels)
                time.sleep(.4)
                subprocess.run(['grim',str(raw/'notifications.png')],env=env,capture_output=True,check=True,timeout=8)
                warm=matrix.measure.memory(client.pid);drm=matrix.measure.drm_memory(client.pid)
                assert not drm or sum(drm.values())==0,'small notice/OSD must use CPU'
                bus_memory=matrix.measure.memory(daemon_pid)
                matrix.wait_log(path,lambda t:"PANEL_IDLE_RELEASED workers=0 rasters=0 shm=0" in t,timeout=10)
                time.sleep(1)
                assert history.stat().st_mode&0o777==0o600
                saved=json.loads(history.read_text());assert saved["version"]==1 and len(saved["entries"])>=3
                def ipc(target_name,method,*args):
                    reply=subprocess.run([binary,'ipc','--socket',str(private/'cmd'),'--json',target_name,method,*map(str,args)],env=env,capture_output=True,text=True,check=True,timeout=5)
                    return json.loads(reply.stdout)['result']
                counts=json.loads(ipc('notifs','counts'));assert counts['historyReady'] and counts['popups']==1
                if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,check=True,capture_output=True,timeout=5)
                interactive=matrix.services.Probe([str(target/'examples/notification-client'),address,'interactive'],env,raw/'notification-interactive.log')
                notice_id=interactive.wait('UI_NOTICE')['id']
                prefix='notices.k'+str(notice_id).encode().hex()+'.'
                def region(suffix):
                    end=time.monotonic()+2
                    while time.monotonic()<end:
                        state=ipc('bead','regions')
                        found=next((r for r in state['regions'] if prefix in r['id'] and r['id'].endswith('.'+suffix)),None)
                        if found:return state,found['rect']
                        time.sleep(.02)
                    raise AssertionError(('missing UI region',prefix+suffix,state))
                def center(state,rect):
                    x,y,w,h=rect
                    return round(1920-state['size'][0]+state['offset'][0]+x+w/2),round(state['offset'][1]+y+h/2)
                pointer=matrix.services.Probe([str(target/'examples/private-pointer')],env,raw/'notification-pointer.log');pointer.wait('POINTER_READY');time.sleep(.15)
                state,rect=region('notice');x,y=center(state,rect);pointer.command(f'move {x} {y}','POINTER_DONE')
                time.sleep(2)
                assert interactive.process.poll() is None,'hover did not pause expiry'
                # Replace through the standard API while pointer stays inside the notice.
                replaced=subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.Notifications','--object-path','/org/freedesktop/Notifications','--method','org.freedesktop.Notifications.Notify','Example',str(notice_id),'dialog-information','Replaced while hovered','Hover keeps expiry paused',"['open', 'Open']",'{}','300'],env=env,capture_output=True,text=True,check=True,timeout=5)
                assert int(re.search(r'uint32 (\d+)',replaced.stdout)[1])==notice_id
                time.sleep(.5)
                assert interactive.process.poll() is None,'replacement lost hover pause'
                # Details are activated through the card before invoking the visible action.
                pointer.command('click','POINTER_DONE');time.sleep(.2)
                state,rect=region('action-1');assert rect[2]>=48 and rect[3]>=28,('compact action hitbox must be visible',rect)
                (raw/'notification-action-region.json').write_text(json.dumps({'state':state,'rect':rect},indent=2))
                subprocess.run(['grim',str(raw/'notification-expanded.png')],env=env,capture_output=True,check=True,timeout=8)
                x,y=center(state,rect);pointer.command(f'move {x} {y}','POINTER_DONE');pointer.command('click','POINTER_DONE')
                interactive.wait('UI_ACTION_PASS');interactive.process.wait(timeout=3);interactive.reader.join(timeout=2);assert interactive.process.returncode==0
                interactive=None;pointer.close();pointer=None
                assert json.loads(ipc('notifs','counts'))['retained']>=4
                ipc('notifs','clear');time.sleep(.4);assert json.loads(ipc('notifs','counts'))['popups']==0
                (raw/'closed-regions.json').write_text(json.dumps(ipc("bead","regions"),indent=2))
                closed_idle=matrix.idle(client.pid,float(os.environ.get('BEAD_NOTIFICATION_CLOSED_SECONDS','2')))
                subprocess.run(['grim',str(raw/'notifications-closed.png')],env=env,capture_output=True,check=True,timeout=8)
                subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.Notifications','--object-path','/org/freedesktop/Notifications','--method','org.nacre.Bead.Notifications.SetDoNotDisturb','true'],env=env,capture_output=True,check=True,timeout=5)
                time.sleep(.1)
                stop(client);client=None
                assert not socket.exists(),'owned private bus socket left behind'
                assert not pathlib.Path(f'/proc/{daemon_pid}').exists(),'owned bus daemon did not exit'
                assert ambient_owner()==before_owner,'ambient notification name changed'
                # Restart preserves history without replaying expired popups.
                with (raw/'notifications-restart.log').open('w') as restart:
                    client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=restart,stderr=subprocess.STDOUT,start_new_session=True)
                    restarted=matrix.wait_log(raw/'notifications-restart.log',lambda t:'UI_PRESENTED' in t,timeout=20)
                    restarted=matrix.log_text(raw/'notifications-restart.log')
                    assert 'UI_ERROR' not in restarted
                    counts=json.loads(ipc('notifs','counts'));assert counts['popups']==0 and counts['retained']>=4
                    address=re.search(r'NOTIFICATIONS_PRIVATE address=(\S+)',restarted)[1]
                    prop=subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.Notifications','--object-path','/org/freedesktop/Notifications','--method','org.freedesktop.DBus.Properties.Get','org.nacre.Bead.Notifications','Inhibited'],env=env,capture_output=True,text=True,check=True,timeout=5)
                    assert 'true' in prop.stdout,'DND was not restored'
                    child=subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.GetConnectionUnixProcessID','org.freedesktop.DBus'],env=env,capture_output=True,text=True,check=True,timeout=5)
                    child_pid=int(re.search(r'uint32 (\d+)',child.stdout)[1]);owned_daemons.add(child_pid)
                    os.kill(client.pid,signal.SIGKILL);client.wait(timeout=5);client=None
                    deadline=time.monotonic()+3
                    while time.monotonic()<deadline and pathlib.Path(f'/proc/{child_pid}').exists():time.sleep(.03)
                    assert not pathlib.Path(f'/proc/{child_pid}').exists(),'private daemon survived parent SIGKILL'
                    with (raw/'notifications-crash-restart.log').open('w') as final:
                        client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=final,stderr=subprocess.STDOUT,start_new_session=True)
                        matrix.wait_log(raw/'notifications-crash-restart.log',lambda t:'UI_PRESENTED' in t,timeout=20)
                        assert json.loads(ipc('notifs','counts'))['popups']==0
                        stop(client);client=None
                result={'compositor':compositor,'protocol':True,'ui_action':True,'hover_replacement':True,'legacy_counts_clear':True,'osd_channels':4,'history_restart':True,'dnd_restart':True,'bus_parent_death':True,'ambient_owner_unchanged':True,'idle':idle,'warm_memory':warm,'drm_mib':drm,'bus_memory':bus_memory,'closed_idle':closed_idle}
                (raw/'notification-proof.json').write_text(json.dumps(result,indent=2)+'\n')
                print('NOTIFICATION_PROOF '+json.dumps(result),flush=True)
        finally:
            if pointer:pointer.close()
            if interactive:
                stop(interactive.process);interactive.reader.join(timeout=2)
            if client:stop(client)
            for owned_pid in owned_daemons:
                try:
                    args=pathlib.Path(f'/proc/{owned_pid}/cmdline').read_bytes().split(b'\0')
                    if ('--address=unix:path='+str(socket)).encode() in args:os.kill(owned_pid,signal.SIGKILL)
                except ProcessLookupError:pass
                except FileNotFoundError:pass
    return result
