#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Private nested proof: first-frame bundle, startup fallback, watchdog, safe mode."""
import importlib.util,json,os,pathlib,re,shutil,signal,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1'}
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    with tempfile.TemporaryDirectory(prefix='bead-recovery-') as directory:
        root=pathlib.Path(directory);shutil.copytree(ROOT/'shell',root/'shell')
        ui=root/'shell/recovery';ui.mkdir()
        (ui/'behavior.luau').write_text('return {stores={},actions={},computed={}}')
        original='local ui=require("bead.ui");return ui.Panel{id="recovery",width=320,height=100,ui.Text{text="Recovery ready"}}'
        fixed=original.replace('Recovery ready','Original fixed')
        (ui/'main.luau').write_text(original)
        palette=(root/'shell/palette.json').read_bytes()
        command=[str(binary),'--ui','recovery/main.luau','--renderer','cpu','--backend','none','--namespace','bead-recovery-proof']
        if compositor=='hyprland':command+=['--output','bead-bench']
        def start(name,extra=None):
            log=raw/(name+'.log');stream=log.open('w')
            client=subprocess.Popen(['nice','-n','10',*command],cwd=root,env={**env,**(extra or {})},stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            stream.close();return client,log
        def presented(log):matrix.wait_log(log,lambda t:bool(re.search(r'UI_PRESENTED generation=[1-9]',t)))
        state=pathlib.Path(env['XDG_STATE_HOME'])/'bead/ui'
        client,log=start('first')
        try:
            presented(log)
            matrix.wait_log(log,lambda t:'RECOVERY_BUNDLE_RECORDED generation=' in t)
            candidates=list(state.glob('ui-*.json'));assert len(candidates)==1
            saved=candidates[0];data=json.loads(saved.read_text())
            assert bytes(data['files']['recovery/main.luau']).decode()==original
        finally:stop(client)
        (ui/'main.luau').write_text('broken UI !!!!');(root/'shell/palette.json').write_text('broken palette')
        client,log=start('fallback')
        try:
            matrix.wait_log(log,lambda t:'RECOVERY code=startup_bundle' in t)
            presented(log)
            assert 'source_error=true' not in log.read_text() and 'UI_ERROR' not in log.read_text()
            (root/'shell/palette.json').write_bytes(palette);(ui/'main.luau').write_text(fixed)
            matrix.wait_log(log,lambda t:bool(re.search(r'UI_PRESENTED generation=[3-9]',t)))
            deadline=time.monotonic()+5
            while bytes(json.loads(saved.read_text())['files']['recovery/main.luau']).decode()!=fixed:
                assert time.monotonic()<deadline,'fixed original source not saved';time.sleep(.03)
        finally:stop(client)
        client,log=start('watchdog',{'BEAD_PRIVATE_MAIN_LOOP_STALL_MS':'8000'})
        try:
            presented(log);client.wait(timeout=12)
            assert client.returncode==-signal.SIGABRT
            text=log.read_text();begin=int(re.search(r'WATCHDOG_PROBE main_stall_begin monotonic_ns=(\d+)',text)[1])
            end=int(re.search(r'WATCHDOG code=main_loop_timeout stalled_ms=\d+ monotonic_ns=(\d+)',text)[1])
            stall_ms=(end-begin)/1e6;assert 5000<=stall_ms<7500,(stall_ms,'watchdog outside busy+dispatch grace')
            report=next(l for l in text.splitlines() if l.startswith('WATCHDOG code='));assert '/' not in report and 'Recovery ready' not in report
        finally:stop(client)
        # Two further unclean exits make three; the fourth start must be safe.
        for index in range(2):
            client,log=start('crash-'+str(index))
            try:presented(log);os.killpg(client.pid,signal.SIGKILL);client.wait(timeout=5)
            finally:stop(client)
        client,log=start('safe-mode')
        try:
            matrix.wait_log(log,lambda t:'RECOVERY code=safe_mode' in t and 'UI_PRESENTED generation=0' in t)
            text=log.read_text();assert 'UI_COMMIT' not in text and 'SERVICE_BUS' not in text and 'NOTIFICATIONS_PRIVATE' not in text
        finally:stop(client)
        result={'compositor':compositor,'first_frame_bundle':True,'startup_bad_source_palette':True,'fixed_original_reload':True,'watchdog_stall_ms':stall_ms,'crash_loop_safe_mode':True,'live_actions':False}
        (raw/'recovery-proof.json').write_text(json.dumps(result,indent=2)+'\n')
        print('RECOVERY_PROOF '+json.dumps(result),flush=True)
