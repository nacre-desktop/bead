#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""CPU-first and idle release proof inside measure.py containment."""
import importlib.util,json,os,pathlib,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
def sample(env,raw,compositor,stop):
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'
    rows=[]
    for mode in ['bar','panel']:
        path=raw/(mode+'.log')
        command=[str(target),'--bar','--renderer','auto','--namespace','bead-policy']
        if mode=='panel':command+=['--bar-panel','--debug-policy-cycle','--seconds','9']
        if compositor=='hyprland':command+=['--output','bead-bench']
        with path.open('w') as log:
            client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                matrix.wait_log(path,lambda t:'UI_PRESENTED generation=1' in t)
                if mode=='panel':matrix.wait_log(path,lambda t:'transition=cpu-to-gl' in t)
                time.sleep(.4)
                def memory():
                    pss=matrix.measure.memory(client.pid)['pss_mib'];drm=sum((matrix.measure.drm_memory(client.pid) or {}).values())
                    return {'pss_mib':pss,'drm_mib':drm,'total_mib':pss+drm}
                warm=memory()
                if mode=='panel':matrix.wait_log(path,lambda t:'transition=gl-to-cpu' in t)
                time.sleep(.4);cold=memory()
                if mode=='panel':
                    matrix.wait_log(path,lambda t:t.count('transition=cpu-to-gl')==2,timeout=15)
                    matrix.wait_log(path,lambda t:t.count('transition=gl-to-cpu')==2,timeout=15)
            finally:stop(client)
        text=path.read_text()
        assert client.returncode==0 and 'STOP surfaces=0' in text
        assert 'UI_ERROR' not in text
        if mode=='panel':
            assert text.index('first_frame=cpu')<text.index('transition=cpu-to-gl')<text.index('transition=gl-to-cpu')
            assert text.count('transition=cpu-to-gl')==2
            assert text.count('transition=gl-to-cpu')==2
            assert 'shared_context_released=true' in text
        else:assert 'transport=GL' not in text and 'GPU_SHARED' not in text
        rows.append({'mode':mode,'warm':warm,'idle':cold})
    (raw/'policy-proof.json').write_text(json.dumps({'compositor':compositor,'rows':rows},indent=2)+'\n')
    print('POLICY_PROOF '+json.dumps(rows),flush=True)
