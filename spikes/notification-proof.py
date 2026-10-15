#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Real native private notification server and both view families, nested only."""
import importlib.util,json,os,pathlib,re,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release'
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(target/'nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    path=raw/'notifications.log';client=None
    with tempfile.TemporaryDirectory(prefix='bn-') as directory:
        private=pathlib.Path(directory);socket=private/'bus';history=private/'history.json'
        command=[binary,'--notification-bus',str(socket),'--notification-history',str(history),'--command-socket',str(private/'cmd'),'--backend','none','--trace-state','--namespace','bead-notifications']
        if compositor=='hyprland':command+=['--output','bead-bench']
        def ambient_owner():
            return subprocess.run(['gdbus','call','--session','--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.NameHasOwner','org.freedesktop.Notifications'],env=env,check=True,capture_output=True,text=True,timeout=5).stdout
        before_owner=ambient_owner()
        try:
            with path.open('w') as log:
                client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                text=matrix.wait_log(path,lambda t:'NOTIFICATIONS_PRIVATE address=' in t and 'UI_PRESENTED' in t,timeout=30)
                text=matrix.log_text(path)
                address=re.search(r'NOTIFICATIONS_PRIVATE address=(\S+)',text)[1]
                daemon_reply=subprocess.run(['gdbus','call','--address',address,'--dest','org.freedesktop.DBus','--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.GetConnectionUnixProcessID','org.freedesktop.DBus'],env=env,check=True,capture_output=True,text=True,timeout=5).stdout
                daemon_pid=int(re.search(r'uint32 (\d+)',daemon_reply)[1])
                time.sleep(.3)
                idle=matrix.idle(client.pid,float(os.environ.get("BEAD_NOTIFICATION_IDLE_SECONDS","2")))
                fixture=subprocess.run([str(target/'examples/notification-client'),address],env=env,capture_output=True,text=True,timeout=20)
                (raw/'notification-client.log').write_text(fixture.stdout+fixture.stderr)
                assert fixture.returncode==0,(fixture.stdout+fixture.stderr)[-1600:]
                assert 'NOTIFICATION_PROTOCOL_PASS' in fixture.stdout
                text=matrix.wait_log(path,lambda t:'Visible notification' in t and '"osd.label":"Caps lock"' in t,timeout=10)
                text=matrix.log_text(path)
                assert 'UI_ERROR' not in text
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
                saved=json.loads(history.read_text());assert len(saved)>=3
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
                    stop(client);client=None
                result={'compositor':compositor,'protocol':True,'osd_channels':4,'history_restart':True,'ambient_owner_unchanged':True,'idle':idle,'warm_memory':warm,'drm_mib':drm,'bus_memory':bus_memory}
                (raw/'notification-proof.json').write_text(json.dumps(result,indent=2)+'\n')
                print('NOTIFICATION_PROOF '+json.dumps(result),flush=True)
        finally:
            if client:stop(client)
    return result
