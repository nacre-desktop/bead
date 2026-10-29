#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Real private tray/PipeWire proof callback for the existing measure.py harness."""
import json
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import pathlib
import subprocess
import threading
import time
import importlib.util

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('measure', ROOT/'spikes/measure.py')
measure = importlib.util.module_from_spec(spec); spec.loader.exec_module(measure)

class Probe:
    def __init__(self, command, env, path):
        self.lines=[]; self.condition=threading.Condition()
        self.process=subprocess.Popen(command,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,bufsize=1,start_new_session=True)
        def drain():
            with path.open('w') as log:
                for line in self.process.stdout:
                    log.write(line);log.flush()
                    with self.condition:self.lines.append(line.rstrip());self.condition.notify_all()
        self.reader=threading.Thread(target=drain);self.reader.start()
    def wait(self, prefix, predicate=lambda v:True, offset=0, timeout=12):
        deadline=time.monotonic()+timeout
        with self.condition:
            while True:
                for line in self.lines[offset:]:
                    if line.startswith(prefix+' '):
                        value=json.loads(line.split(' ',1)[1])
                        if predicate(value):return value
                if self.process.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Proof timeout '+prefix+' '+str(self.lines[-3:]))
                self.condition.wait(.02)
    def command(self, value, prefix='RESULT'):
        offset=len(self.lines);self.process.stdin.write(json.dumps(value)+'\n');self.process.stdin.flush()
        return self.wait(prefix,offset=offset)
    def current(self, predicate, timeout=12):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            value=self.command({'action':'snapshot'})['snapshot']
            if predicate(value):return value
            time.sleep(.02)
        raise RuntimeError('Current snapshot timeout '+str(value))
    def close(self):
        if self.process.poll() is None:
            self.process.stdin.write('stop\n');self.process.stdin.flush()
            self.process.wait(timeout=10)
        self.reader.join(timeout=2)
        assert self.process.returncode == 0

def resources(pid):
    return {**measure.memory(pid),'fds':len(list(pathlib.Path(f'/proc/{pid}/fd').iterdir())), 'threads':len(list(pathlib.Path(f'/proc/{pid}/task').iterdir()))}

def pipewire(env, raw):
    config=raw/'pipewire.conf'
    config.write_text('''context.properties = { core.daemon = true core.name = bead-private-pw }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-metadata }
 { name = libpipewire-module-spa-node-factory }
 { name = libpipewire-module-client-node }
 { name = libpipewire-module-adapter }
 { name = libpipewire-module-access args = { access.force = unrestricted } }
]
context.objects = [
 { factory = adapter args = { factory.name = support.null-audio-sink node.name = bead-null media.class = Audio/Sink audio.position = [ FL FR ] } }
 { factory = metadata args = { metadata.name = default } }
]
''')
    daemon=subprocess.Popen(['pipewire','-c',str(config)],env=env,stdout=(raw/'pipewire.log').open('a'),stderr=subprocess.STDOUT,start_new_session=True)
    for _ in range(100):
        if (pathlib.Path(env['XDG_RUNTIME_DIR'])/'bead-private-pw').exists():break
        if daemon.poll() is not None:raise RuntimeError('Private PipeWire exited')
        time.sleep(.05)
    subprocess.run(['pw-metadata','-n','default','0','default.audio.sink','{"name":"bead-null"}','Spa:String:JSON'],env=env,capture_output=True,check=True,timeout=5)
    return daemon

def sample(env, raw, compositor, stop):
    # Reuse private XDG/compositor containment; a replaceable bus tests EOF/reconnect.
    address='unix:path='+str(pathlib.Path(env['XDG_RUNTIME_DIR'])/'services-bus')
    env={**env,'DBUS_SESSION_BUS_ADDRESS':address,'DBUS_SYSTEM_BUS_ADDRESS':address,'PIPEWIRE_REMOTE':'bead-private-pw','BEAD_PRIVATE_SERVICES_TEST':'1'}
    target=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples'
    def bus():
        p=subprocess.Popen(['dbus-daemon','--session','--nofork','--address='+address],env=env,stdout=(raw/'services-bus.log').open('a'),stderr=subprocess.STDOUT,start_new_session=True)
        time.sleep(.15);return p
    daemon=bus();pw=None;probe=None;tray=None
    try:
        pw=pipewire(env,raw)
        probe=Probe([str(target/'services-probe')],env,raw/'services-probe.log')
        probe.current(lambda s:s['audio']['volume'] is not None)
        for volume in [.25,.75,.5]:
            assert probe.command({'action':'volume','value':volume})['ok']
            probe.current(lambda s: s['audio']['volume'] is not None and abs(s['audio']['volume']-volume)<.001)
        for mute in [True,False]:
            assert probe.command({'action':'mute','value':mute})['ok']
            probe.current(lambda s:s['audio']['muted']==mute)
        for restart in range(3):
            offset=len(probe.lines);stop(pw);pw=None
            probe.current(lambda s:not s['audio']['connected'])
            assert not probe.command({'action':'mute','value':True})['ok']
            pw=pipewire(env,raw)
            probe.current(lambda s:s['audio']['volume'] is not None)
        samples=[]
        for cycle in range(100):
            log=raw/f'tray-{cycle}.log';offset=len(probe.lines)
            tray=subprocess.Popen([str(target/'tray-fixture')],env={**env,'BEAD_FIXTURE_UPDATE':'1'},stdout=log.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
            s=probe.wait('SNAPSHOT',lambda s:len(s['tray'])==1 and bool(s['tray'][0]['menu']),offset)
            item=s['tray'][0]['id']
            if cycle in [0,50,99]:
                for action in ['activate','prepare','menu']:
                    assert probe.command({'action':action,'item':item})['ok']
                probe.wait('SNAPSHOT',lambda s:len(s['tray'])==1 and s['tray'][0]['title'].endswith('1'),offset)
                text=log.read_text()
                assert all(marker in text for marker in ['TRAY_ACTIVATE 7 9','TRAY_PREPARE 0','TRAY_MENU 1 clicked','TRAY_UPDATED']),text
            offset=len(probe.lines);stop(tray);tray=None
            probe.wait('SNAPSHOT',lambda s:not s['tray'],offset)
            assert not probe.command({'action':'activate','item':item})['ok']
            result=probe.command({'action':'subscriptions'});assert result['subscriptions']==1
            if cycle in [9,49,99]:
                time.sleep(.2);samples.append({'cycle':cycle+1,'bus_tasks':result['bus_tasks'],'retained_tasks':result['retained_tasks'],**resources(probe.process.pid)})
                print('SERVICE_CHURN '+json.dumps(samples[-1]),flush=True)
        # A separate real bar runtime exercises its actual hit testing and action routes.
        probe.close();probe=None
        for item_menu in [False,True]:
            ui_env={**env,**({'BEAD_FIXTURE_ITEM_IS_MENU':'1'} if item_menu else {})}
            ui_log=raw/f'bar-service-routes-{item_menu}.log'
            ui=subprocess.Popen([str(target.parent/'nacre-shell'),'--services-ui-probe'],cwd=ROOT,env=ui_env,stdout=ui_log.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
            time.sleep(.5)
            tray_log=raw/f'bar-service-tray-{item_menu}.log'
            tray=subprocess.Popen([str(target/'tray-fixture')],env=ui_env,stdout=tray_log.open('w'),stderr=subprocess.STDOUT,start_new_session=True)
            try:
                ui.wait(timeout=20)
                assert ui.returncode==0,ui_log.read_text()
                text=tray_log.read_text()
                assert 'TRAY_PREPARE' in text and 'TRAY_MENU 1 clicked' in text,text
                assert ('TRAY_ACTIVATE' in text) != item_menu,text
            finally:stop(ui);stop(tray);tray=None
        probe=Probe([str(target/'services-probe')],env,raw/'services-reconnect-probe.log')
        probe.current(lambda s:s['audio']['volume'] is not None)
        reconnect_baseline=resources(probe.process.pid)
        reconnect_tasks=probe.command({'action':'subscriptions'})['bus_tasks']
        # Bus replacement must clear snapshots and re-register the watcher.
        for restart in range(3):
            offset=len(probe.lines);stop(daemon);daemon=bus()
            for _ in range(100):
                tray=subprocess.Popen([str(target/'tray-fixture')],env=env,stdout=(raw/f'bus-tray-{restart}.log').open('a'),stderr=subprocess.STDOUT,start_new_session=True)
                time.sleep(.1)
                if tray.poll() is None:break
                stop(tray);tray=None
            assert tray is not None
            probe.wait('SNAPSHOT',lambda s:len(s['tray'])==1,offset,timeout=15)
            offset=len(probe.lines)
            time.sleep(.2);stop(tray);tray=None
            probe.wait('SNAPSHOT',lambda s:not s['tray'],offset)
        time.sleep(2)
        result=probe.command({'action':'subscriptions'})
        assert result['retained_tasks']<=2,result
        final={'bus_tasks':result['bus_tasks'],'retained_tasks':result['retained_tasks'],**resources(probe.process.pid)}
        assert final['bus_tasks']<=max(samples[0]['bus_tasks'],reconnect_tasks),(samples,final)
        assert final['fds']<=samples[0]['fds']+2,(samples,final)
        assert final['threads']<=samples[0]['threads']+1,(samples,final)
        assert final['pss_mib']-samples[0]['pss_mib']<2,(samples,final)
        begun=time.monotonic();before=measure.ticks(probe.process.pid)
        print('SERVICE_IDLE sampling 60 seconds',flush=True)
        time.sleep(60)
        idle=measure.ticks(probe.process.pid)-before
        result={'tray_cycles':100,'pipewire_restarts':3,'bus_restarts':3,'samples':samples,'final':resources(probe.process.pid),'bus_tasks_after_restarts':final['bus_tasks'],'reconnect_baseline':reconnect_baseline,'idle_ticks':idle,'idle_seconds':time.monotonic()-begun,'scope':'private real null sink, SNI/DBusMenu, replacement bus; no hardware'}
        assert idle<=2,result
        probe.close();probe=None
        (raw/'services-lifecycle.json').write_text(json.dumps(result,indent=2)+'\n')
        print('SERVICES_LIFECYCLE PASS '+json.dumps(result),flush=True)
    finally:
        if tray:stop(tray)
        if probe:stop(probe.process);probe.reader.join(timeout=2)
        if pw:stop(pw)
        stop(daemon)
