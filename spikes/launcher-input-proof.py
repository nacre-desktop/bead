#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Core launcher view-state keyboard proof; exact fixture replies, nested only."""
import importlib.util,json,os,pathlib,re,shutil,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
matrix=load('matrix',ROOT/'spikes/bar-matrix.py')
fixtures=load('menu_fixtures',ROOT/'spikes/menu-fixtures.py')

def sample(env,raw,compositor,stop):
    binary=os.environ.get('BEAD_HARDENING_BINARY',str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell'))
    env={**env,'BEAD_PRIVATE_INPUT_PROOF':'1','BEAD_PROFILE_INPUT':'clocks','BEAD_MATCHED_ICONS':'1',
         'BEAD_ICON_PATH':str(ROOT/'shell/sample.svg'),'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],
         'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    client=pointer=keeper=None;log=raw/'launcher-input.log';states=[]
    def presented_state(text,query,selection=None):
        matches=list(re.finditer(r'^UI_STATE (.*)$',text,re.M))
        for match in reversed(matches):
            state=json.loads(match[1])
            if state.get('launcher.search.text')!=query:continue
            if selection is not None and state.get('launcher.selection')!=selection:continue
            commit=re.search(r'UI_COMMIT generation=(\d+).*update=(\d+).*source_error=false',text[match.end():])
            if not commit:continue
            generation,update=commit.groups()
            if not re.search(r'UI_PRESENTED generation='+generation+r' .*update='+update+r' ',text[match.end()+commit.end():]):continue
            chosen=[r['id']for r in state.get('launcher.apps',[])if r.get('selected')]
            assert chosen==([state['launcher.selection']]if state.get('launcher.selection')else[]),'selection/model diverged'
            return state
        return None
    def key(args,query,selection=None):
        offset=len(matrix.log_text(log))
        subprocess.run(['wtype','-s','20',*args],env=env,capture_output=True,check=True,timeout=6)
        matrix.wait_log(log,lambda t:presented_state(t[offset:],query,selection)is not None,timeout=8)
        state=presented_state(matrix.log_text(log)[offset:],query,selection)
        states.append({'query':query,'selection':state['launcher.selection'],'scroll':state['launcher.scroll.y']})
        return state
    try:
        with tempfile.TemporaryDirectory(prefix='bead-launcher-input-')as directory:
            root=pathlib.Path(directory);shutil.copytree(ROOT/'shell',root/'shell')
            rows=[{'id':f'app-{i:04}','name':f'Application {i:04}','kind':'app',
                   'description':'Fixture','icon':'fixture','disabled':i==0}for i in range(320)]
            fixture=root/'menu.json';fixture.write_text(json.dumps({'queries':{q:fixtures.reply(q,rows if not q else rows[:160])for q in['','a','ab']}}))
            keeper=subprocess.Popen(['wtype','-s','600000',''],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
            command=[binary,'--renderer','cpu','--namespace','bead-launcher-input','--ui','ui/main.luau',
                     '--menu-fixture',str(fixture),'--backend','none','--test-keyboard','--trace-state','--seconds','90']
            if compositor=='hyprland':command+=['--output','bead-bench']
            with log.open('w')as stream:
                client=subprocess.Popen(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                matrix.wait_log(log,lambda t:presented_state(t,'','')is not None,timeout=30)
                time.sleep(.3)
                if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,capture_output=True,check=True,timeout=5)
                pointer=matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],env,raw/'pointer.log')
                pointer.wait('POINTER_READY');pointer.command('move 1000 300','POINTER_DONE');time.sleep(.2)
                key(['a'],'a','app-0001')
                subprocess.run(['grim',str(raw/'launcher-search.png')],env=env,capture_output=True,check=True,timeout=8)
                # The first row is deliberately disabled: a missed star cannot
                # activate anything, even in this fixture-only proof.
                for enabled in (True,False):
                    offset=len(matrix.log_text(log))
                    pointer.command('move 1462 140','POINTER_DONE')
                    pointer.command('click','POINTER_DONE')
                    def favorite_presented(text):
                        state=presented_state(text[offset:],'a','app-0001')
                        return state and any(row['id']=='app-0000' and row.get('favorite')is enabled
                                             for row in state.get('launcher.apps',[]))
                    matrix.wait_log(log,favorite_presented,timeout=8)
                    if enabled:
                        subprocess.run(['grim',str(raw/'launcher-favorite.png')],env=env,capture_output=True,check=True,timeout=8)
                key(['-k','Down'],'a','app-0002')
                key(['-k','Up'],'a','app-0001')
                wrapped=key(['-k','Up'],'a','app-0159');assert wrapped['launcher.scroll.y']>0
                reset=key(['b'],'ab','app-0001');assert reset['launcher.scroll.y']==0
                cleared=key(['-M','ctrl','-k','a','-m','ctrl','-k','BackSpace'],'','');assert cleared['launcher.scroll.y']==0
                subprocess.run(['grim',str(raw/'launcher-catalogue.png')],env=env,capture_output=True,check=True,timeout=8)
                key(['-k','Down'],'','app-0001')
                key(['-k','Right'],'','app-0002')
                down=key(['-k','Down'],'');assert int(down['launcher.selection'].split('-')[-1])>3,'grid down did not use column geometry'
                key(['-k','Up'],'','app-0002')
                key(['-k','Left'],'','app-0001')
                offset=len(matrix.log_text(log))
                subprocess.run(['wtype','-s','20','-k','Up'],env=env,capture_output=True,check=True,timeout=6)
                matrix.wait_log(log,lambda t:'MENU_NAV_DONE changed=false'in t[offset:],timeout=8)
                assert 'UI_ERROR'not in matrix.log_text(log)[offset:]
                states.append({'query':'','selection':'app-0001','scroll':0,'noop_ack':True})
                denied=key(['-k','Return'],'','app-0001')
                assert denied['launcher.error'] and client.poll()is None,'activation failure was not visible/recoverable'
                assert 'UI_ERROR'not in matrix.log_text(log)
                assert 'PRESENTATION clock_id=1'in matrix.log_text(log)
                memory=matrix.measure.memory(client.pid)
                assert memory['swap_pss_mib']==0,'nonzero UI SwapPss during launcher input smoke'
                if pointer:pointer.close();pointer=None
                stop(client);client=None
                stop(keeper);keeper=None
        result={'compositor':compositor,'catalog_size':320,'engine':False,'states':states,'favorite_pointer_pin_unpin':True,'memory':memory,'valid':True}
        (raw/'launcher-input-proof.json').write_text(json.dumps(result,indent=2)+'\n')
        print('LAUNCHER_INPUT_PROOF states='+str(len(states))+' valid=true',flush=True)
    finally:
        if pointer:pointer.close()
        if client:stop(client)
        if keeper:stop(keeper)
