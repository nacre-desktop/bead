#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""UI-only Source fixture matching notification-reference.py, no bus ownership."""
import importlib.util
import json
import os
import pathlib
import subprocess
import tempfile
import time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)

def sample(env,raw,compositor,stop):
    if compositor=='hyprland':
        for command,args in [('dismissnotify',['-1']),('seterror',['disable'])]:
            subprocess.run(['hyprctl',command,*args],env=env,capture_output=True,check=True,timeout=5)
    mode=os.environ.get('BEAD_NOTICE_REFERENCE','notification')
    if mode not in ['notification','osd']:
        raise ValueError('notification reference mode')
    snapshot={'rows':[],'popup_count':0,'history_count':0,'history_ready':True,'dnd':False,'inhibited':False,'error':'','timers':[],'osd':None}
    if mode=='notification':
        snapshot.update(rows=[{'id':'1','app':'Example','summary':'Foundation ready','body':'Private fixture','icon':'dialog-information','time_str':'now','expanded':True,'urgency':1,'actions':[{'id':'1|open','label':'Open'}]}],popup_count=1,history_count=1)
    else:
        snapshot['osd']={'channel':'volume','value':.75,'muted':False}
    path=raw/'fixture.json';path.write_text(json.dumps(snapshot));path.chmod(0o600)
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_MATCHED_ICONS':'1','BEAD_ICON_PATH':str(ROOT/'shell/tests/icon.svg'),'BEAD_PRIVATE_NOTIFICATION_FIXTURE':str(path),'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    client=None
    with tempfile.TemporaryDirectory(prefix='bead-notice-ui-')as directory:
        root=pathlib.Path(directory)
        log=raw/(mode+'-bead.log')
        try:
            with log.open('w')as stream:
                client=subprocess.Popen([str(target),'--notification-bus',str(root/'unused-bus'),'--notification-history',str(root/'unused-history'),'--namespace','bead-notice-reference'],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                matrix.wait_log(log,lambda t:'NOTIFICATIONS_FIXTURE source=true'in t and 'UI_PRESENTED generation=1'in t and (mode!='osd'or'companion=true'in t))
                time.sleep(3)
                seconds=float(os.environ.get('BEAD_REFERENCE_SECONDS','60'))
                result=matrix.idle(client.pid,seconds)
                subprocess.run(['grim',str(raw/(mode+'-bead.png'))],env=env,capture_output=True,check=True,timeout=8)
                text=log.read_text()
                assert 'UI_ERROR'not in text and 'PANIC 'not in text
                assert not (root/'unused-bus').exists() and not (root/'unused-history').exists()
                result.update(compositor=compositor,mode=mode,fixture='bounded fake Source; identical visual data; no notification server')
                (raw/(mode+'-bead.json')).write_text(json.dumps(result,indent=2)+'\n')
                print('BEAD_NOTICE_REFERENCE '+mode+' pss_plus_drm_mib='+str(result['pss_plus_drm_mib'])+' seconds='+str(seconds),flush=True)
        finally:
            if client:stop(client)
    assert 'STOP surfaces=0'in log.read_text()
