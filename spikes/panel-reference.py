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
import time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)

def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    with tempfile.TemporaryDirectory(prefix='bead-sound-reference-') as directory:
        root=pathlib.Path(directory)
        reference=matrix.reference.fixture(root)
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
                    NacreText {text:"Sound output";font.pointSize:18;width:parent.width-80}
                    ActionButton {text:"Close";compact:true}
                }
                NacreText {text:"bead-null";height:26}
                Row {
                    height:40;spacing:12
                    ActionButton {text:"Mute";compact:true}
                    NacreText {text:"100%";font.pointSize:18}
                }
                NacreQuickSlider {width:352;value:1;label:"Output volume"}
                NacreText {text:"Drag or use arrow keys to adjust volume";font.pointSize:11}
            }
        }
    }
}
'''
        path.write_text(text)
        log=raw/'qml-panel.log'
        with log.open('w') as f:
            client=subprocess.Popen(['quickshell','--path',str(path)],env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                time.sleep(4)
                if client.poll() is not None:raise RuntimeError('QML reference exited; inspect log')
                if compositor=='hyprland':subprocess.run(['hyprctl','dismissnotify','-1'],env=env,capture_output=True,timeout=5)
                subprocess.run(['grim',str(raw/'qml-sound.png')],env=env,check=True,capture_output=True,timeout=8)
                result=matrix.idle(client.pid,float(os.environ.get('BEAD_PANEL_IDLE_SECONDS','60')))
                result['source_revision']=os.environ.get('NACRE_REFERENCE_REVISION') or subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()
                result['scope']='unchanged NacreHeader/widgets/NacreQuickSlider; output-controls subset, deterministic transport shims; no microphone/device list'
                (raw/'qml-panel-proof.json').write_text(json.dumps(result,indent=2)+'\n')
                print('QML_PANEL_PROOF '+json.dumps(result),flush=True)
            finally:stop(client)
