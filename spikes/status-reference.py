#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Matched read-only unavailable status fixture, not a full Nacre service panel."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)

def sample(env,raw,compositor,stop):
    runtime=Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime!=Path(f'/run/user/{os.getuid()}'),'live runtime refused'
    assert Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime),'live display refused'
    kind=os.environ['BEAD_STATUS_REFERENCE_KIND']
    assert kind in ['network','bluetooth','battery']
    title={'network':'Network','bluetooth':'Bluetooth','battery':'Battery'}[kind]
    icon={'network':'wifi','bluetooth':'bluetooth','battery':'battery_full'}[kind]
    values=dict(title=title,icon=icon,primary=title+' unavailable',secondary='Status data unavailable',detail='')
    private=dict(env,DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],PIPEWIRE_REMOTE='bead-private-unavailable')
    with tempfile.TemporaryDirectory(prefix='bead-status-reference-')as directory:
        root=Path(directory);reference=matrix.reference.fixture(root)
        compositor_fixture=root/'services/NacreCompositor.qml'
        text=compositor_fixture.read_text()
        compositor_fixture.write_text(text[:text.rfind('}')]+ ' property var monitors:({values:[]}); property var workspaces:({values:[]}) }\n')
        path=root/'shell.qml';text=path.read_text().replace('import "modules/topbar"','import "modules/topbar"\nimport "widgets"')
        content="""
        PanelWindow {
            anchors {top:true;left:true}
            margins.top:70
            implicitWidth:374;implicitHeight:286;exclusiveZone:-1;color:"transparent"
            WlrLayershell.namespace:"bead-reference-status"
            NacreSurface {
                id:statusFixture
                anchors.fill:parent;anchors.margins:32
                radius:NacreTokens.radii.medium;color:NacreTokens.body
                property var values:VALUES
                readonly property real inset:NacreTokens.space.md
                readonly property real iconPixels:NacreTokens.iconSize.xlarge*4/3
                Column {
                    x:statusFixture.inset;y:statusFixture.inset
                    width:310-statusFixture.inset*2;spacing:NacreTokens.space.sm
                    Text {text:statusFixture.values.title;width:parent.width;height:24;font.family:NacreTokens.textFamily;font.pixelSize:NacreTokens.textSize.title*4/3;color:NacreTokens.ink}
                    Item {
                        width:parent.width;height:88
                        Text {x:0;y:23;width:statusFixture.iconPixels;height:42;text:statusFixture.values.icon;font.family:"Material Symbols Rounded";font.pixelSize:statusFixture.iconPixels;color:NacreTokens.mutedInk}
                        Column {
                            x:statusFixture.iconPixels+NacreTokens.space.md;y:2
                            width:parent.width-x;height:84;spacing:2
                            Text {width:parent.width;height:60;text:statusFixture.values.primary;font.family:NacreTokens.textFamily;font.pixelSize:NacreTokens.textSize.body*4/3;color:NacreTokens.ink;wrapMode:Text.WrapAtWordBoundaryOrAnywhere}
                            Text {width:parent.width;height:22;text:statusFixture.values.secondary;font.family:NacreTokens.textFamily;font.pixelSize:NacreTokens.textSize.caption*4/3;color:NacreTokens.mutedInk}
                        }
                    }
                    Text {width:parent.width;height:60;text:statusFixture.values.detail;font.family:NacreTokens.textFamily;font.pixelSize:NacreTokens.textSize.caption*4/3;color:NacreTokens.mutedInk;wrapMode:Text.WrapAtWordBoundaryOrAnywhere}
                }
            }
        }
        """.replace('VALUES',json.dumps(values))
        path.write_text(text[:text.rfind('}')]+content+'}\n')
        log=raw/'qml-status.log'
        with log.open('w')as output:
            client=subprocess.Popen(['quickshell','--path',str(path)],env=private,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                time.sleep(4)
                assert client.poll()is None,'QML status exited; inspect private log'
                text=log.read_text()
                assert not any(error in text for error in ['TypeError:','ReferenceError:','is not a type']),'QML fixture has a runtime/type error'
                subprocess.run(['grim',str(raw/'qml-status.png')],env=private,capture_output=True,check=True,timeout=8)
                result=matrix.idle(client.pid,float(os.environ.get('BEAD_REFERENCE_SECONDS','60')))
                result.update(compositor=compositor,kind=kind,fixture_state='unavailable',source_revision=os.environ['NACRE_REFERENCE_REVISION'],scope='matched read-only status fixture + actual NacreHeader/NacreSurface; same shared tokens/geometry/text; not full production network/Bluetooth/battery parity or native service CPU')
                (raw/'qml-status.json').write_text(json.dumps(result,indent=2)+'\n')
                print('QML_STATUS_REFERENCE',compositor,kind,result['pss_plus_drm_mib'],flush=True)
            finally:stop(client)
