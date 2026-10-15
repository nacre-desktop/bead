#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Matched output-controls subset using unchanged Nacre QML widgets."""
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import re
import time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)

def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    seconds=float(os.environ.get('BEAD_PANEL_IDLE_SECONDS','60'))
    probe=os.environ.get('BEAD_QML_PROBE')
    if probe:env.update(LD_PRELOAD=probe,BEAD_QML_TRACE_PRESENT='1',BEAD_QML_QUIET_IDLE='1')
    with tempfile.TemporaryDirectory(prefix='bead-sound-reference-') as directory:
        root=pathlib.Path(directory)
        reference=matrix.reference.fixture(root)
        # Match the private native bar: one workspace, no focused title, audio100,
        # unavailable network/Bluetooth. Keep visual widgets unchanged.
        for name,replacements in {
            'NacreProfile': [('workspaceCount:5','workspaceCount:1')],
            'NacreCompositor': [('title:"Bead fixture · reference"','title:""'),('property var windows:[activeWindow]','property var windows:[]')],
            'NacreAudio': [('volume:0.5','volume:1')],
            'NacreNetwork': [('active:({strength:80})','active:null'),('wifiAvailable:true','wifiAvailable:false')],
            'NacreBluetooth': [('available:true','available:false'),('powered:true','powered:false')],
        }.items():
            shim=root/'services'/(name+'.qml');body=shim.read_text()
            for old,new in replacements:body=body.replace(old,new)
            shim.write_text(body)
        popup=root/'modules/bar/popouts';popup.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(reference/'modules/bar/popouts/NacreQuickSlider.qml',popup/'NacreQuickSlider.qml')
        path=root/'shell.qml'
        text=path.read_text().replace('import "modules/topbar"','import "modules/topbar"\nimport "modules/bar/popouts"\nimport "widgets"')
        end=text.rfind('}')
        text=text[:end]+'''
    PanelWindow {
        id: panel
        anchors {top:true;right:true}
        margins.top:70
        implicitWidth:464;implicitHeight:350;exclusiveZone:-1;color:"transparent"
        WlrLayershell.namespace:"bead-reference-sound-output"
        NacreSurface {
            anchors.fill:parent;anchors.margins:32;radius:16;color:NacreTokens.body
            Column {
                anchors.fill:parent;anchors.margins:20;spacing:14
                Row {
                    width:parent.width;height:32;spacing:10
                    NacreText {text:"Sound output";font.pointSize:14;width:parent.width-80}
                    ActionButton {text:"Close";compact:true}
                }
                NacreText {text:"bead-null";height:26}
                Row {
                    height:40;spacing:12
                    ActionButton {text:"Mute";compact:true}
                    NacreText {text:"100%";font.pointSize:18}
                }
                NacreQuickSlider {width:352;value:1;label:"Output volume"}
                NacreText {text:"Drag or use arrow keys to adjust volume";font.pointSize:10}
            }
        }
    }
}
'''
        if probe:
            text=text.replace('import Quickshell.Wayland','import Quickshell.Wayland\nimport BeadProbe 1.0')
            text=text.replace('id: panel','id: panel\n        visible:expanded||progress>0.0001\n        property bool expanded:true\n        property real progress:expanded?1:0\n        Behavior on progress {NumberAnimation {duration:200;easing.type:Easing.OutCubic}}')
            text=text.replace('anchors.fill:parent;anchors.margins:32;radius:16;color:NacreTokens.body','anchors.fill:parent;anchors.margins:32;radius:16;color:NacreTokens.body;opacity:panel.progress\n            transform:Translate {x:(1-panel.progress)*464}')
            end=text.rfind('}')
            text=text[:end]+'''Timer {
                interval:START_AFTER;running:true
                onTriggered:motion.start()
            }
            Timer {
                id:motion;interval:500;repeat:true;property int phase:0
                onTriggered:{TimingProbe.commit(++phase);panel.expanded=!panel.expanded;if(phase>=20)stop()}
            }
            '''.replace('START_AFTER',str(round((seconds+6)*1000)))+text[end:]
        path.write_text(text)
        log=raw/'qml-panel.log'
        with log.open('w') as f:
            client=subprocess.Popen(['quickshell','--path',str(path)],env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                time.sleep(4)
                if client.poll() is not None:raise RuntimeError('QML reference exited; inspect log')
                if compositor=='hyprland':subprocess.run(['hyprctl','dismissnotify','-1'],env=env,capture_output=True,timeout=5)
                subprocess.run(['grim',str(raw/'qml-sound.png')],env=env,check=True,capture_output=True,timeout=8)
                result=matrix.idle(client.pid,seconds)
                result['source_revision']=os.environ.get('NACRE_REFERENCE_REVISION') or subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()
                result['scope']='unchanged NacreHeader/widgets/NacreQuickSlider; output-controls subset, deterministic transport shims; no microphone/device list'
                if probe:
                    matrix.wait_log(log,lambda t:'QML_FRAME generation=20 ' in t,timeout=25)
                    time.sleep(.4)
                    groups={}
                    for generation,stamp in re.findall(r'QML_FRAME generation=(\d+) monotonic_ns=(\d+) panel=true',matrix.log_text(log)):
                        if int(generation)>0:groups.setdefault(int(generation),[]).append(int(stamp))
                    assert len(groups)==20, 'missing real QML animation phase presentation'
                    result['animation_intervals_ms']={phase:[(b-a)/1e6 for g,v in groups.items() if g%2==parity for a,b in zip(v,v[1:])] for phase,parity in [('open',0),('close',1)]}
                    result['animation_phases']=len(groups)
                    assert 'QML_PRESENTATION clock_id=1' in matrix.log_text(log)
                (raw/'qml-panel-proof.json').write_text(json.dumps(result,indent=2)+'\n')
                print('QML_PANEL_PROOF '+json.dumps({k:v for k,v in result.items() if k!='animation_intervals_ms'}),flush=True)
            finally:stop(client)
