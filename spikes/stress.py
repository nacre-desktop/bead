#!/usr/bin/env python3
"""Matched Feature 2 clients; raw evidence stays ignored, no generated ledger.

SPDX-License-Identifier: MIT OR Apache-2.0
Default preview is scripts/dev.sh. This measurement needs --live because nested
refresh does not establish real-monitor performance. Stops only its own clients.
"""
import argparse
import math
import os
import pathlib
import re
import subprocess
import time
import threading
from measure import memory, ticks, stop

ROOT = pathlib.Path(__file__).resolve().parent.parent

def percentile(values, p):
    if not values:
        return None
    return sorted(values)[math.ceil((len(values)-1)*p)]

def timestamp(line):
    match=re.search(r'\[\s*(\d+):(\d+):(\d+\.\d+)\]',line)
    if match:return (int(match[1])*3600+int(match[2])*60+float(match[3]))*1000
    match=re.search(r'\[\s*([0-9.]+)\]',line)
    return float(match[1]) if match else None

def pacing(data):
    pending = set()
    times = []
    first = None
    for line in data.splitlines():
        request = re.search(r'wl_surface[@#]\d+\.frame\((?:new id )?wl_callback[@#](\d+)', line)
        done = re.search(r'wl_callback[@#](\d+)\.done(?:,\s*)?\(', line)
        stamp=timestamp(line)
        if request:
            pending.add(request[1])
        if done and done[1] in pending and stamp is not None:
            pending.remove(done[1])
            value=stamp
            if first is None:
                first = value
            if value > first + 2000:
                times.append(value)
    intervals = [(b-a) % 4294967296 for a,b in zip(times,times[1:])]
    return {'callback_samples': len(intervals), 'callback_p50_ms': percentile(intervals,.5), 'callback_p99_ms': percentile(intervals,.99)}

def sample(mode, seconds, traced, idle):
    raw = ROOT/'spikes/results/raw/feature2'
    raw.mkdir(parents=True,exist_ok=True)
    label = f'{mode}-{"idle" if idle else "animated"}-{"trace" if traced else "cpu"}'
    env=os.environ.copy()
    env['WAYLAND_DEBUG']='1' if traced else '0'
    env['BEAD_IDLE']='1' if idle else '0'
    env['BEAD_PALETTE_PATH']=str(ROOT/'spikes/palette-stress.json')
    env['BEAD_SHELL_DIR']=str(ROOT/'shell')
    # Select the same installed SVG icon for both clients without copying its asset.
    icon=pathlib.Path('/usr/share/icons/Adwaita/scalable/places/folder.svg')
    if icon.exists():
        env['BEAD_ICON_PATH']=str(icon)
    target=pathlib.Path(env.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
    if mode=='qml':
        command=['quickshell','--path',str(ROOT/'spikes/panel.qml')]
        if traced:
            env['QSG_RENDER_TIMING']='1'
    else:
        command=[str(target/'nacre-shell'),'--panel' if idle else '--animate','--namespace','bead-dev-f2','--palette',str(ROOT/'spikes/palette-stress.json')]
        if mode=='bead-full':
            command.append('--no-damage')
    started=time.monotonic()
    with (raw/f'{label}.log').open('w') as log:
        client=subprocess.Popen(command,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True,text=True,bufsize=1)
        ready=threading.Event();first_frame=[]
        def drain():
            pending=set()
            for line in client.stdout:
                log.write(line)
                request=re.search(r'wl_surface[@#]\d+\.frame\((?:new id )?wl_callback[@#](\d+)',line)
                done=re.search(r'wl_callback[@#](\d+)\.done(?:,\s*)?\(',line)
                if request:pending.add(request[1])
                if done and done[1] in pending:
                    pending.remove(done[1])
                    if not first_frame:first_frame.append((time.monotonic()-started)*1000)
                    ready.set()
                elif not traced and 'READY' in line:ready.set()
        reader=threading.Thread(target=drain);reader.start()
        print(f'{label}: own PID {client.pid}; emergency stop: kill -TERM {client.pid}',flush=True)
        try:
            if not ready.wait(timeout=20):
                raise RuntimeError(f'{label} has no ready/frame marker; client status={client.poll()}')
            time.sleep(3)
            before=ticks(client.pid);begin=time.monotonic()
            before_mem=memory(client.pid)
            (raw/f'{label}-before.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
            print(f'{label}: sampling {seconds:g}s, trace={traced}',flush=True)
            while time.monotonic()<begin+seconds:
                if client.poll() is not None:
                    raise RuntimeError(f'{label} exited during sample')
                time.sleep(min(1,begin+seconds-time.monotonic()))
            elapsed=time.monotonic()-begin
            cpu=(ticks(client.pid)-before)/os.sysconf('SC_CLK_TCK')/elapsed*100
            after_mem=memory(client.pid)
            maps=pathlib.Path(f'/proc/{client.pid}/maps').read_text()
            emoji_mapped='NotoColorEmoji' in maps or 'NotoColorEmoji' in maps.replace('-','')
            print(f'{label}: color emoji font mapped={emoji_mapped}',flush=True)
            (raw/f'{label}-after.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
        finally:
            stop(client)
            reader.join(timeout=5)
            if reader.is_alive():raise RuntimeError("client log reader did not stop")
    text=(raw/f'{label}.log').read_text(errors='replace')
    metrics=[line for line in text.splitlines() if line.startswith(('BUFFER','RESOURCES','METRICS','STOP'))]
    print(label,{'cpu_one_core_pct':round(cpu,3),'before':before_mem,'after':after_mem,**(pacing(text) if traced else {})},flush=True)
    for line in metrics:
        print(line,flush=True)
    if first_frame:print('launch_to_first_frame_callback_ms',round(first_frame[0],3))
    print(f'{label}: stopped, exit={client.returncode}',flush=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live',action='store_true')
    parser.add_argument('--seconds',type=float,default=60)
    parser.add_argument('--trace',action='store_true')
    parser.add_argument('--idle',action='store_true')
    parser.add_argument('--modes',nargs='+',choices=['bead-damage','bead-full','qml'],default=['bead-damage','bead-full','qml'])
    args=parser.parse_args()
    if not args.live:parser.error('Use --live only for the separate bead-dev-f2 measurement; preview with scripts/dev.sh')
    for mode in args.modes:sample(mode,args.seconds,args.trace,args.idle)
if __name__=='__main__':main()
