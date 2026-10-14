#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Native bar screenshot proof; uses measure.py's existing private session."""
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def sample(env, raw, compositor, stop):
    if os.environ.get('BEAD_GL_UNAVAILABLE_PROOF'):
        target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
        path=raw/(compositor+'-no-gl.log')
        private={**env,'LIBGL_ALWAYS_SOFTWARE':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
        client=subprocess.Popen([str(target),'--bar','--bar-panel','--renderer','auto','--seconds','3'],cwd=ROOT,env=private,stdout=path.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        try:client.wait(timeout=15)
        finally:stop(client)
        text=path.read_text()
        assert client.returncode==1 and 'software GL adapter' in text and 'STOP surfaces=0' in text,text
        print('COMPANION_GL_UNAVAILABLE clean bounded error; no recursive fallback',flush=True)
        return
    if os.environ.get('BEAD_BAR_MATRIX'):
        import importlib.util
        spec=importlib.util.spec_from_file_location('bar_matrix',ROOT/'spikes/bar-matrix.py')
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        return module.sample(env,raw,compositor,stop)
    if os.environ.get('BEAD_SERVICES_LIFECYCLE'):
        import importlib.util
        spec=importlib.util.spec_from_file_location('services_lifecycle',ROOT/'spikes/services-lifecycle.py')
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        return module.sample(env,raw,compositor,stop)
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'], 'PIPEWIRE_REMOTE':'bead-private-unavailable','BEAD_PRIVATE_SERVICES_TEST':'1'}
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR']) / 'release/nacre-shell'
    windows = subprocess.Popen(['quickshell','--path',str(ROOT/'spikes/backend-windows.qml')],env=env,stdout=(raw/(compositor+'-bar-windows.log')).open('w'),stderr=subprocess.STDOUT,start_new_session=True)
    try:
        for renderer in ['cpu','femtovg-gl']:
            log=(raw/(compositor+'-bar-'+renderer+'.log')).open('w')
            client=subprocess.Popen([str(target),'--bar','--renderer',renderer,'--memory-usage','--namespace','bead-feature6-bar'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            tray=None
            try:
                time.sleep(1)
                tray=subprocess.Popen([str(target.parent/"examples/tray-fixture")],env=env,stdout=(raw/(compositor+"-tray-"+renderer+".log")).open("w"),stderr=subprocess.STDOUT,start_new_session=True)
                time.sleep(3)
                if client.poll() is not None: raise RuntimeError('Native bar exited; inspect private bar log')
                if compositor=='niri':
                    outputs=json.loads(subprocess.check_output(['niri','msg','-j','outputs'],env=env,text=True,timeout=5))
                    width=next(iter(outputs.values()))['logical']['width']
                else:
                    outputs=json.loads(subprocess.check_output(['hyprctl','-j','monitors'],env=env,text=True,timeout=5))
                    width=int(outputs[0]['width']/outputs[0]['scale'])
                subprocess.run(['grim','-g',f'0,0 {width}x50',str(raw/(compositor+'-bar-'+renderer+'.png'))],env=env,capture_output=True,timeout=8,check=True)
                print('NATIVE_BAR_CAPTURE '+compositor+' renderer='+renderer+' width='+str(width)+' height=50',flush=True)
            finally:
                if tray is not None: stop(tray)
                stop(client);log.close()
            text=(raw/(compositor+"-bar-"+renderer+".log")).read_text()
            assert "UI_COMMIT" in text and "UI_ERROR" not in text and "STOP surfaces=0" in text, "bar must load, draw and release successfully"
    finally:
        stop(windows)
