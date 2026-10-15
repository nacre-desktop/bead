#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Unfocused-title churn against a passive bar + real panel; existing isolation."""
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)


def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],
         'PIPEWIRE_REMOTE':'bead-private-pw','BEAD_PRIVATE_SERVICES_TEST':'1',
         'BEAD_BACKGROUND_TITLE_CHURN':'1'}
    binary=os.environ['BEAD_NATIVE_BINARY'];sock=pathlib.Path(env['XDG_RUNTIME_DIR'])/'models.sock'
    log=raw/'models.log';pw=windows=probe=client=None
    def call(method):
        r=subprocess.run([binary,'ipc','--socket',str(sock),'--json','bead.panel',method]+(['sound']if method=='open'else[]),env=env,cwd=ROOT,check=True,capture_output=True,text=True,timeout=5)
        return json.loads(r.stdout)['result']
    def presented(text,panel):
        for m in re.finditer(r'UI_PRESENTED generation=1 .*companion='+str(panel).lower()+r' update=(\d+)(?:\s|$)',text):
            if re.search(r'UI_COMMIT generation=1 .*update='+m[1]+r' companion='+str(panel).lower()+r' source_error=false(?:\s|$)',text[:m.start()]):return True
        return False
    try:
        pw=matrix.services.pipewire(env,raw)
        windows=subprocess.Popen(['quickshell','--path',str(ROOT/'spikes/backend-windows.qml')],env=env,stdout=(raw/'windows.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        probe=matrix.services.Probe([os.environ['BEAD_MODEL_PROBE_BINARY'],'--backend-probe'],env,raw/'probe.log')
        snapshot=probe.wait('BACKEND_SNAPSHOT',lambda s:len([w for w in s['windows']if w['title'].startswith('Bead fixture')])==2)
        first=next(w for w in snapshot['windows']if w['title']=='Bead fixture A')
        second=next(w for w in snapshot['windows']if w['title'].startswith('Bead fixture B'))
        result=probe.command({'action':'focus-window','window':first['id']},'BACKEND_RESULT')
        assert result['ok'] and result['observed'],result
        command=[binary,'--bar','--audio-panel','--command-socket',str(sock),'--test-passive','--trace-state','--namespace','bead-model-proof']
        if compositor=='hyprland':command+=['--output','bead-bench']
        with log.open('w')as f:
            client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
            matrix.wait_log(log,lambda t:presented(t,False))
            call('open');matrix.wait_log(log,lambda t:presented(t,True));time.sleep(2)
            assert any(json.loads(l[9:]).get('bead.focused_title')=='Bead fixture A'for l in matrix.log_text(log).splitlines()if l.startswith('UI_STATE '))
            offset=len(matrix.log_text(log));sample=matrix.idle(client.pid,float(os.environ.get('BEAD_MODEL_SECONDS','20')))
            tail=matrix.log_text(log)[offset:];updates=tail.count('BACKEND_UPDATE ')
            assert updates>=30,'background workload did not reach the client'
            commits=len(re.findall(r'UI_COMMIT .*companion=false',tail))
            offset=len(matrix.log_text(log))
            result=probe.command({'action':'focus-window','window':second['id']},'BACKEND_RESULT')
            assert result['ok'] and result['observed'],result
            def focus_presented(text):
                tail=text[offset:]
                for match in re.finditer(r'UI_STATE (.*)\n',tail):
                    if not json.loads(match[1]).get('bead.focused_title','').startswith('Bead fixture B'):continue
                    commit=re.search(r'UI_COMMIT generation=(\d+).*update=(\d+) companion=false source_error=false',tail[match.end():])
                    if commit and re.search(r'UI_PRESENTED generation='+commit[1]+r'.*companion=false update='+commit[2]+r'(?:\s|$)',tail[match.end()+commit.end():]):return True
                return False
            matrix.wait_log(log,focus_presented)
            assert 'UI_ERROR'not in matrix.log_text(log)
            result={'compositor':compositor,'churn':sample,'backend_updates':updates,'bar_commits':commits,'genuine_focus_change':True,'genuine_focus_presented':True}
            (raw/'model-proof.json').write_text(json.dumps(result,indent=2)+'\n')
            print('MODEL_PROOF '+json.dumps(result),flush=True)
    finally:
        if client:stop(client)
        if probe:probe.close()
        if windows:stop(windows)
        if pw:stop(pw)
