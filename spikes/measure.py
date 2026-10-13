#!/usr/bin/env python3
"""Measure task-owned clients in nested niri and stop every process on exit.

SPDX-License-Identifier: MIT OR Apache-2.0
"""
import argparse
import json
import os
import pathlib
import re
import select
import signal
import stat
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def ticks(pid):
    fields = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return int(fields[11]) + int(fields[12])


def memory(pid):
    fields = {}
    for line in pathlib.Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines():
        if ':' in line:
            key, value = line.split(':', 1)
            if value.strip().endswith('kB'):
                fields[key] = int(value.split()[0])
    return {'pss_mib': fields['Pss'] / 1024, 'uss_mib': sum(fields.get(k, 0) for k in ['Private_Clean', 'Private_Dirty', 'Private_Hugetlb']) / 1024,
            'swap_pss_mib': fields.get('SwapPss',0)/1024, 'swap_mib':fields.get('Swap',0)/1024,
            'pss_anon_mib':fields.get('Pss_Anon',0)/1024,'pss_file_mib':fields.get('Pss_File',0)/1024,'pss_shmem_mib':fields.get('Pss_Shmem',0)/1024}


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seconds', type=float, default=60)
    p.add_argument('--runs', type=int, default=3)
    p.add_argument('--modes', nargs='+', default=['tiny-skia','vello','femtovg','qml'])
    p.add_argument('--in-private-bus', action='store_true')
    args = p.parse_args()
    if not args.in_private_bus:
        # D-Bus activation inherits its daemon's environment: isolate XDG BEFORE
        # starting the bus, not just before launching its clients.
        with tempfile.TemporaryDirectory(prefix='bead-bus-') as directory:
            env=os.environ.copy()
            for key, name in [('XDG_RUNTIME_DIR','runtime'),('XDG_CONFIG_HOME','config'),('XDG_CACHE_HOME','cache'),('XDG_STATE_HOME','state'),('XDG_DATA_HOME','data')]:
                path=pathlib.Path(directory)/name
                path.mkdir(mode=0o700)
                env[key]=str(path)
            env['XDG_DATA_DIRS']='/usr/local/share:/usr/share'
            return subprocess.call(['dbus-run-session', '--', sys.executable, __file__, '--in-private-bus', '--seconds', str(args.seconds), '--runs', str(args.runs), '--modes', *args.modes],env=env)
    raw = ROOT / 'spikes/results/raw'
    raw.mkdir(parents=True, exist_ok=True)
    result = {'date': '2026-10-13', 'surface': {'width': 512, 'height': 32, 'scale': 1, 'outputs': 1, 'namespace': 'bead-dev-spike'},
              'method': 'client /proc smaps_rollup; CPU ticks over monotonic interval; READY is frame callback for Rust, QML component completion (not comparable first-frame); separate nested niri/private D-Bus/XDG', 'runs': []}
    with tempfile.TemporaryDirectory(prefix='bead-phase0-') as directory:
        private = pathlib.Path(directory)
        env = os.environ.copy()
        for key, name in [('XDG_RUNTIME_DIR','runtime'), ('XDG_CONFIG_HOME','config'), ('XDG_CACHE_HOME','cache'), ('XDG_STATE_HOME','state'), ('XDG_DATA_HOME','data')]:
            path = private / name
            path.mkdir(mode=0o700)
            env[key] = str(path)
        env['XDG_DATA_DIRS'] = '/usr/local/share:/usr/share'
        for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY']:
            env.pop(key, None)
        env['WGPU_BACKEND'] = 'vulkan'
        env['WGPU_POWER_PREF'] = 'low'
        env['QSG_INFO']='1'
        # An offscreen headless parent avoids visible preview-window closure
        # interrupting unattended measurements. niri remains the client-facing
        # compositor; the labwc process is outside measured client PSS/CPU.
        parent_runtime = private / 'parent-runtime'
        parent_runtime.mkdir(mode=0o700)
        parent_config = private / 'labwc'
        parent_config.mkdir()
        (parent_config/'autostart').write_text('')
        parent_env = env.copy()
        parent_env.update({'XDG_RUNTIME_DIR':str(parent_runtime), 'WLR_BACKENDS':'headless', 'WLR_HEADLESS_OUTPUTS':'1', 'WLR_RENDERER':'gles2',
                           'LD_LIBRARY_PATH':str(ROOT/'.tools/native/usr/lib')})
        parent_env.pop('WAYLAND_DISPLAY', None)
        parent_log = (raw/'labwc.log').open('w')
        parent = subprocess.Popen([str(ROOT/'.tools/native/usr/bin/labwc'), '--config-dir', str(parent_config)],env=parent_env,stdout=parent_log,stderr=parent_log,start_new_session=True)
        deadline = time.monotonic()+20
        while time.monotonic()<deadline:
            sockets = [f for f in parent_runtime.glob('wayland-*') if stat.S_ISSOCK(f.stat().st_mode)]
            if sockets:
                env['WAYLAND_DISPLAY']=str(sockets[0])
                break
            if parent.poll() is not None:
                raise RuntimeError('Offscreen parent exited; inspect ignored labwc.log')
            time.sleep(0.1)
        else:
            stop(parent)
            raise RuntimeError('Offscreen parent did not start')
        log = (raw / 'niri.log').open('w')
        compositor = subprocess.Popen(['niri', '--config', str(ROOT/'spikes/niri.kdl')], env=env, stdout=log, stderr=log, start_new_session=True)
        client = None
        try:
            deadline = time.monotonic() + 20
            runtime = pathlib.Path(env['XDG_RUNTIME_DIR'])
            while time.monotonic() < deadline:
                sockets = [f for f in runtime.glob('wayland-*') if stat.S_ISSOCK(f.stat().st_mode)]
                if sockets:
                    env['WAYLAND_DISPLAY'] = str(sockets[0])
                    break
                if compositor.poll() is not None:
                    raise RuntimeError('Nested niri exited; inspect ignored spikes/results/raw/niri.log')
                time.sleep(0.1)
            else:
                raise RuntimeError('Nested niri did not create a Wayland socket')
            info=subprocess.run(['wayland-info'],env=env,capture_output=True,text=True,timeout=10,check=True)
            (raw/'wayland-info.log').write_text(info.stdout)
            result['globals']=[{'interface':name,'version':int(version)} for name,version in re.findall(r"interface: '([^']+)',\s*version:\s*(\d+)",info.stdout)]
            sockets=list(runtime.glob('niri.*.sock'))
            if sockets:
                env['NIRI_SOCKET']=str(sockets[0])
                outputs=subprocess.run(['niri','msg','--json','outputs'],env=env,capture_output=True,text=True,timeout=10,check=True)
                result['output_properties']=json.loads(outputs.stdout)
            for run in range(args.runs):
                for mode in args.modes:
                    command = ['quickshell', '--log-rules', 'qml.debug=true', '--path', str(ROOT/'spikes/blank.qml')] if mode == 'qml' else [str(ROOT/'target/release/bead-spike-blank-layer'), mode]
                    started = time.monotonic()
                    client = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True, bufsize=0)
                    data, ready_at = b'', None
                    deadline = started + 60
                    while time.monotonic() < deadline:
                        if select.select([client.stdout], [], [], 0.1)[0]:
                            chunk = os.read(client.stdout.fileno(), 65536)
                            data += chunk
                            if b'READY' in data:
                                ready_at = time.monotonic()
                                break
                            if not chunk:
                                break
                    (raw/f'{mode}-{run}.log').write_bytes(data)
                    if ready_at is None:
                        result['runs'].append({'renderer':mode, 'run':run, 'error':'No READY marker; see ignored log'})
                        print(f'{mode} run {run}: FAILED', flush=True)
                        stop(client)
                        continue
                    time.sleep(5)
                    before_mem, before_ticks, before = memory(client.pid), ticks(client.pid), time.monotonic()
                    (raw/f'{mode}-{run}-before.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
                    # The parent waits; progress lines identify the active interval.
                    print(f'{mode} run {run}: ready, sampling idle CPU for {args.seconds:g}s', flush=True)
                    deadline = before + args.seconds
                    while time.monotonic() < deadline:
                        if client.poll() is not None:
                            data += client.stdout.read()
                            (raw/f'{mode}-{run}.log').write_bytes(data)
                            result['runs'].append({'renderer':mode, 'run':run, 'error':f'Exited during idle interval: {client.returncode}', 'compositor_exit':compositor.poll()})
                            raise RuntimeError(f'{mode} exited during idle interval: {client.returncode}; compositor status: {compositor.poll()}')
                        time.sleep(min(1, deadline-time.monotonic()))
                    elapsed = time.monotonic() - before
                    after_ticks, after_mem = ticks(client.pid), memory(client.pid)
                    (raw/f'{mode}-{run}-after.smaps').write_text(pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text())
                    drm=[]
                    for fd in pathlib.Path(f'/proc/{client.pid}/fdinfo').glob('*'):
                        try:
                            lines=[line for line in fd.read_text().splitlines() if line.startswith('drm-')]
                            if lines:drm.append(lines)
                        except OSError:
                            pass
                    row_gpu={'fdinfo':drm,'note':'driver accounting where available; PSS does not include all GPU allocations'}
                    row = {'renderer':mode, 'run':run, 'ready_wall_ms':(ready_at-started)*1000, 'before':before_mem, 'after':after_mem,
                           'idle_seconds':elapsed, 'cpu_ticks':after_ticks-before_ticks,
                           'idle_cpu_one_core_percent':100*(after_ticks-before_ticks)/os.sysconf('SC_CLK_TCK')/elapsed,
                           'ready_kind':'QML component completion' if mode=='qml' else 'wl_surface frame callback; not presentation timestamp'}
                    row['gpu']=row_gpu
                    result['runs'].append(row)
                    (ROOT/'spikes/results/baseline.json').write_text(json.dumps(result,indent=2)+'\n')
                    print(f"{mode} run {run}: PSS {after_mem['pss_mib']:.2f} MiB / USS {after_mem['uss_mib']:.2f} MiB / CPU {row['idle_cpu_one_core_percent']:.3f}% / SwapPss {after_mem['swap_pss_mib']:.2f} MiB", flush=True)
                    subprocess.run(['grim', '-t', 'png', str(raw/f'{mode}-{run}.png')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False)
                    stop(client)
                    data+=client.stdout.read()
                    (raw/f'{mode}-{run}.log').write_bytes(data)
                    client = None
            return 0
        finally:
            if client is not None:
                stop(client)
            stop(compositor)
            stop(parent)
            log.close()
            parent_log.close()
            (ROOT/'spikes/results/baseline.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__ == '__main__':
    raise SystemExit(main())
