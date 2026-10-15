#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Matched notification/OSD visuals using unchanged Nacre QML components."""
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time

ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('reference',ROOT/'spikes/bar-reference.py')
reference=importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)

def sample(env,raw,compositor,stop):
    if compositor=='hyprland':
        for command,args in [('dismissnotify',['-1']),('seterror',['disable'])]:
            subprocess.run(['hyprctl',command,*args],env=env,capture_output=True,check=True,timeout=5)
    mode=os.environ.get('BEAD_NOTICE_REFERENCE','notification')
    if mode not in ['notification','osd']:
        raise ValueError('notification reference mode')
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    seconds=float(os.environ.get('BEAD_REFERENCE_SECONDS','60'))
    with tempfile.TemporaryDirectory(prefix='bead-notice-reference-') as directory:
        root=pathlib.Path(directory)
        original=reference.fixture(root)
        for component in ['notifications','osd']:
            shutil.copytree(original/'modules'/component,root/'modules'/component)
        replacements={
            'NacreNotifs':'property var retained:[]; property var popups:[]; function dismiss(entry) {}',
            'NacreAudio':'property bool muted:false; property real volume:.75; property bool micMuted:false; property real micVolume:.75',
            'NacreKeyboardLight':'property real brightness:.75',
            'NacreI18n':'function tr(key,fallback,args) {return typeof fallback === "string" ? fallback : fallback.other} function number(value) {return String(value)} function windowCaption(title) {return title}',
        }
        for name,body in replacements.items():
            (root/'services'/(name+'.qml')).write_text('pragma Singleton\nimport QtQuick\nQtObject {'+body+'}\n')
        if mode=='notification':
            content='''PanelWindow {
                anchors {top:true;right:true}
                implicitWidth:464;implicitHeight:350;exclusiveZone:-1;color:"transparent"
                WlrLayershell.namespace:"bead-reference-notice"
                NacreNotice {
                    x:32;y:32;width:400;expanded:true
                    modelData: ({appName:"Example",appIcon:Quickshell.iconPath("dialog-information"),summary:"Foundation ready",body:"Private fixture",time:new Date(2026,0,1,10,14),timeStr:"now",urgency:1,image:"",hovered:false,notification:true,actions:[{identifier:"open",text:"Open",invoke:function(){}}]})
                }
            }'''
        else:
            content='''NacreLevelNotice {output:Quickshell.screens[0];shown:true;channel:"volume"}'''
        content=content.replace('Quickshell.iconPath("dialog-information")',json.dumps((ROOT/'shell/tests/icon.svg').as_uri()))
        (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
import "modules/notifications"
import "modules/osd"
ShellRoot {'''+content+'}\n')
        log=raw/(mode+'-qml.log')
        client=None
        try:
            with log.open('w')as stream:
                client=subprocess.Popen(['quickshell','--path',str(root/'shell.qml')],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                time.sleep(3)
                if client.poll() is not None:
                    raise RuntimeError('QML reference exited; inspect private log')
                result=matrix.idle(client.pid,seconds)
                subprocess.run(['grim',str(raw/(mode+'-qml.png'))],env=env,capture_output=True,check=True,timeout=8)
                result.update(compositor=compositor,component='NacreNotice'if mode=='notification'else'NacreLevelNotice',mode=mode,fixture='unchanged visual QML, deterministic transport shims, UI-only cost')
                (raw/(mode+'-qml.json')).write_text(json.dumps(result,indent=2)+'\n')
                print('QML_NOTICE_REFERENCE '+mode+' pss_plus_drm_mib='+str(result['pss_plus_drm_mib'])+' seconds='+str(seconds),flush=True)
        finally:
            if client:stop(client)
