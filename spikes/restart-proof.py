#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Kill/restart an owned client while preserving only its private panel state."""
import importlib.util,json,os,pathlib,shutil,signal,subprocess,tempfile
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    panic=subprocess.run([str(binary),'--panic-probe'],env=env,cwd=ROOT,capture_output=True,text=True,timeout=8)
    assert panic.returncode!=0 and 'PANIC thread=' in panic.stderr and 'private panic hook proof' in panic.stderr
    (raw/'panic.log').write_text(panic.stderr)
    with tempfile.TemporaryDirectory(prefix='bead-restart-') as directory:
        bundle=pathlib.Path(directory);shutil.copytree(ROOT/'shell',bundle/'shell')
        ui=bundle/'shell/restart';ui.mkdir()
        (ui/'behavior.luau').write_text('return {stores={["panel.open"]=true,["panel.page"]="home"},actions={close=function(s) s["panel.open"]=false;s["panel.page"]="sound" end},computed={}}')
        (ui/'main.luau').write_text('local ui=require("bead.ui");local kit=require("bead.kit");return kit.Panel{id="panel",edge="left",width=320,height=200,open=ui.bind("panel.open"),kit.Button{id="close",text="Close",on_activate=ui.action("close")}}')
        command=[str(binary),'--ui','restart/main.luau','--renderer','cpu','--backend','none','--test-keyboard','--trace-state','--namespace','bead-restart-proof']
        if compositor=='hyprland':command+=['--output','bead-bench']
        first=raw/'before.log'
        with first.open('w') as log:
            client=subprocess.Popen(command,cwd=bundle,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                matrix.wait_log(first,lambda t:'UI_PRESENTED generation=1' in t)
                if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,check=True,capture_output=True,timeout=5)
                subprocess.run(['wtype','-s','500','-k','Tab','-s','200','-k','Return'],env=env,check=True,timeout=8)
                matrix.wait_log(first,lambda t:'"panel.open":false' in t and 'PANEL_INPUT visible=false' in t)
                state=pathlib.Path(env['XDG_STATE_HOME'])/'bead/panels.json'
                saved=json.loads(state.read_text())
                assert saved['values']=={'panel.open':False,'panel.page':'sound'}
                os.killpg(client.pid,signal.SIGKILL);client.wait(timeout=5)
                assert client.returncode==-signal.SIGKILL
            finally:stop(client)
        after=raw/'after.log'
        with after.open('w') as log:
            client=subprocess.Popen(command,cwd=bundle,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                matrix.wait_log(after,lambda t:'UI_COMMIT generation=1' in t and '"panel.open":false' in t and '"panel.page":"sound"' in t and 'PANEL_INPUT visible=false' in t)
            finally:stop(client)
        assert client.returncode==0 and 'STOP surfaces=0' in after.read_text()
        print('RESTART_PROOF compositor='+compositor+' abrupt_kill=true panel_open_page_restored=true hidden_input_disabled=true panic_hook=true',flush=True)
