#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Capture unchanged Nacre QML widgets with deterministic private service shims.

Run through measure.py --bar-reference; never against the daily display.
NACRE_REFERENCE is read-only. The fixture replaces transports, not visual widgets.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time


def fixture(root):
    reference = pathlib.Path(os.environ['NACRE_REFERENCE']) / 'nacre/shell'
    for name in ['widgets', 'config', 'modules/topbar', 'modules/bar/components', 'utils', 'branding']:
        shutil.copytree(reference / name, root / name)
    services = root / 'services'
    services.mkdir()
    palette = json.loads((pathlib.Path(__file__).resolve().parent.parent / 'shell/palette.json').read_text())
    palette = {'m3'+key[0].lower()+key[1:]:"#"+value.lstrip("#") for key,value in palette['colours'].items()}
    palette.setdefault('m3frame',palette['m3surface'])
    shims = {
        'NacreColours': 'property var palette: ' + json.dumps(palette) + '; property bool light: false; property bool publishing: false',
        'DesktopSettings': 'property var data: ({animations:false, reduceMotion:true, workspaceWindowHints:false, interfaceFont:"IBM Plex Sans", clickEdgeMenus:true})',
        'NacreProfile': 'property var data: ({workspaceCount:5,workspaceNames:[],workspaceIcons:[]})',
        'NacrePresentation': 'property var active: ({workspaceColors:[]})',
        'NacrePanelState': 'property var screens: ({}); property var panels: ({}); property bool hidden:false; function getForActive() {return null}',
        'NacreHoverIntent': 'property var headers: ({}); property var positions: ({}); property var blocked: ({}); property var popupHovered: ({}); property int dashboardDepth:0; function register() {} function release() {} function observe() {} function setHeaderHover() {} function rearm() {} function fullscreenFor() {return false}',
        'NacreCompositor': 'property int activeWsId:1; property var activeWindow: ({windowId:"fixture",title:"Bead fixture · reference",wmClass:"terminal",pid:0, workspace:{id:1}}); property var windows:[activeWindow]; property var capabilities:({perOutputWorkspaces:false}); property var focusedMonitor:({name:"fixture"}); property var activeWorkspace:({id:1}); property string backendName:"fixture"; property var overviewHoverWindow:null; property var overviewSelectedWindow:null; function workspaceUrgent() {return false} function windowsForWorkspace() {return []}',
        'NacreBrightness': 'function getMonitorForScreen() {return null}',
        'NacreI18n': 'function tr(key,fallback,args) {return typeof fallback === "string" ? fallback : fallback.other} function windowCaption(title) {return title}',
        'ChatWindowTitle': 'property var titles: ({})',
        'NacreAudio': 'property bool muted:false; property real volume:0.5',
        'NacreNetwork': 'property var active:({strength:80}); property bool wifiAvailable:true',
        'NacreBluetooth': 'property bool available:true; property bool powered:true',
        'NacreNotifs': 'property var retained:[]',
        'NacreTime': 'function format(pattern) {return "10:14"}',
        'Updates': 'property string message:""; property int count:0',
        'AppLaunch': 'function run(args) {}',
    }
    for name, body in shims.items():
        (services / (name + '.qml')).write_text('pragma Singleton\nimport QtQuick\nQtObject { ' + body + ' }\n')
    (root / 'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
import "modules/topbar"
ShellRoot {
    PanelWindow {
        id: window
        anchors {top:true; left:true; right:true}
        implicitHeight:50; exclusiveZone:50; color:"transparent"
        WlrLayershell.namespace:"bead-reference-header"
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
        NacreHeader {anchors.fill:parent; screen:window.screen}
    }
}
''')
    return reference


def sample(env, raw, compositor, stop):
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'], 'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    with tempfile.TemporaryDirectory(prefix='bead-header-') as directory:
        root = pathlib.Path(directory)
        reference = fixture(root)
        log = (raw / (compositor + '-qml-header.log')).open('w')
        client = subprocess.Popen(['quickshell', '--path', str(root / 'shell.qml')], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            time.sleep(3)
            if client.poll() is not None:
                raise RuntimeError('QML header exited; inspect private header log')
            if compositor == 'niri':
                outputs=json.loads(subprocess.check_output(['niri','msg','-j','outputs'],env=env,text=True,timeout=5))
                width=next(iter(outputs.values()))['logical']['width']
            else:
                outputs=json.loads(subprocess.check_output(['hyprctl','-j','monitors'],env=env,text=True,timeout=5))
                width=int(outputs[0]['width']/outputs[0]['scale'])
            subprocess.run(['grim', '-g', f'0,0 {width}x50', str(raw / (compositor + '-qml-header.png'))], env=env, capture_output=True, check=True, timeout=8)
            (raw / (compositor + '-qml-source.json')).write_text(json.dumps({'revision':subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip(), 'component':'modules/topbar/NacreHeader.qml','fixture':'unchanged visual widgets; deterministic transport shims','height_logical':50},indent=2)+'\n')
            print('QML_HEADER_CAPTURE ' + compositor + ' height=50', flush=True)
        finally:
            stop(client)
            log.close()
