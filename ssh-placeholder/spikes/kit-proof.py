#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Shared kit capture proof; containment is owned by measure.py."""
import importlib.util,os,pathlib,shutil,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    with tempfile.TemporaryDirectory(prefix='bead-kit-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        source=bundle/'shell/kit/main.luau';original=source.read_text()
        for edge in ['left','right','top']:
            source.write_text(original.replace('edge="left"',f'edge="{edge}"'))
            command=[str(binary),'--renderer','femtovg-gl','--namespace','bead-kit-proof']
            command+=['--bar'] if edge=='top' else ['--ui','kit/main.luau','--backend','none']
            if compositor=='hyprland':command+=['--output','bead-bench']
            path=raw/(edge+'.log')
            with path.open('w') as log:
                client=subprocess.Popen(command,cwd=bundle,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                try:
                    matrix.wait_log(path,lambda t:'UI_PRESENTED generation=1' in t)
                    time.sleep(.4)
                    subprocess.run(['grim',str(raw/(edge+'.png'))],env=env,check=True,capture_output=True,timeout=8)
                finally:stop(client)
            text=path.read_text()
            assert 'UI_ERROR' not in text and 'STOP surfaces=0' in text and client.returncode==0
            print('KIT_PROOF compositor='+compositor+' edge='+edge+' presented=true stopped=true',flush=True)
