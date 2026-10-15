#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Real sound panel proof, using measure.py's existing private containment."""
import importlib.util
import json
import os
import pathlib
import re
import subprocess
from PIL import Image
import socket as unix_socket
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('matrix', ROOT/'spikes/bar-matrix.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


def sample(env, raw, compositor, stop):
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS': env['DBUS_SESSION_BUS_ADDRESS'],
           'PIPEWIRE_REMOTE': 'bead-private-pw', 'BEAD_PRIVATE_SERVICES_TEST': '1',
           'BEAD_PRIVATE_INPUT_PROOF': '1'}
    target = pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release'
    socket = pathlib.Path(env['XDG_RUNTIME_DIR'])/'panel.sock'
    log = raw/'panel.log'
    pw = matrix.services.pipewire(env, raw)
    client = pointer = None
    command = [str(target/'nacre-shell'), '--bar', '--audio-panel', '--command-socket',
               str(socket), '--namespace', 'bead-gate-c', '--trace-state']
    if os.environ.get('BEAD_BACKGROUND_BLUR'):command+=['--background-blur']
    if os.environ.get('BEAD_PANEL_NO_DAMAGE'):command+=['--no-damage']
    if os.environ.get('BEAD_PANEL_RENDERER'):
        command += ['--panel-renderer', os.environ['BEAD_PANEL_RENDERER']]
    if compositor == 'hyprland':
        command += ['--output', 'bead-bench']
    seconds = float(os.environ.get('BEAD_PANEL_IDLE_SECONDS', '0'))

    def run(*args):
        return subprocess.run(args, env=env, check=True, capture_output=True, text=True, timeout=8)

    def call(method, *args, target_name='bead.panel', ok=True):
        r = subprocess.run([str(target/'nacre-shell'), 'ipc', '--socket', str(socket),
                            '--json', target_name, method, *args], cwd=ROOT, env=env,
                           capture_output=True, text=True, timeout=5)
        if (r.returncode == 0) != ok:
            raise RuntimeError((method, r.stderr))
        return json.loads(r.stdout)

    def memory():
        label = 'memory-'+str(time.monotonic_ns())
        (raw/(label+'.smaps')).write_text(pathlib.Path(f'/proc/{client.pid}/smaps').read_text())
        p = matrix.measure.memory(client.pid)
        d = matrix.measure.drm_memory(client.pid) or {}
        return {'pss_mib': p['pss_mib'], 'drm_mib': sum(d.values()), 'swap_pss_mib':p['swap_pss_mib'],
                'total_mib': p['pss_mib']+sum(d.values()),
                'fds': len(list(pathlib.Path(f'/proc/{client.pid}/fd').iterdir())),
                'threads': len(list(pathlib.Path(f'/proc/{client.pid}/task').iterdir()))}

    def wait_closed():
        deadline = time.monotonic()+15
        while call('state')['result']['resident']:
            if time.monotonic() > deadline:
                raise RuntimeError('panel did not release')
            time.sleep(.1)

    def audio(predicate):
        return matrix.wait_log(log, lambda t: any('panel.open' in json.loads(l[9:]) and predicate(json.loads(l[9:]))
                              for l in t.splitlines()[::-1] if l.startswith('UI_STATE ')))

    def pointer_to(region, width):
        x, y, w, h = region
        # Panel uses the selected output's right edge and a 70 logical-pixel top margin.
        pointer.command(f'move {round(1920-width+32+x+w/2)} {round(70+32+y+h/2)}', 'POINTER_DONE')
        time.sleep(.08)

    result = {'compositor': compositor, 'renderer': os.environ.get('BEAD_PANEL_RENDERER', 'default'),
              'idle_seconds': seconds, 'cycles': []}
    protocols=run('wayland-info').stdout
    result['clipboard_transport']='ext-data-control-v1' if 'ext_data_control_manager_v1' in protocols and os.environ.get('BEAD_PRIVATE_CLIPBOARD_CORE')!='1' else 'wl_data_device'
    result['private_pipewire_fixture']=matrix.measure.memory(pw.pid)
    try:
        with log.open('w') as f:
            client = subprocess.Popen(command, cwd=ROOT, env=env, stdout=f,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            matrix.wait_log(log, lambda t: 'UI_PRESENTED generation=1' in t)
            if result['clipboard_transport']=='ext-data-control-v1':
                call('clipboard-set','private before keyboard',target_name='bead')
                time.sleep(.1)
                assert call('clipboard-get',target_name='bead')['result']=='private before keyboard'
                result['clipboard_without_keyboard']=True
            time.sleep(2)
            if seconds:
                print('PANEL_IDLE_SAMPLE bar', flush=True)
                result['bar_idle'] = matrix.idle(client.pid, seconds)
            baseline = memory()
            result['bar'] = baseline
            for i in range(int(os.environ.get('BEAD_PANEL_CYCLES', '10'))):
                offset = len(matrix.log_text(log))
                started = time.monotonic_ns()
                call('open', 'sound')
                matrix.wait_log(log, lambda t: 'UI_PRESENTED generation=1' in t[offset:]
                                and 'companion=true' in t[offset:], timeout=20)
                ready = next(int(m[1]) for m in re.finditer(
                    r'UI_PRESENTED generation=1 monotonic_ns=(\d+).*companion=true',
                    matrix.log_text(log)[offset:]))
                time.sleep(.4)
                legacy=json.loads(call('state',target_name='nacre')['result'])
                assert legacy['edgeMenu']=='sound' and legacy['bead']['panelOpen']
                warm = memory()
                segment = matrix.log_text(log)[offset:]
                timestamps = [int(n) for n in re.findall(r'PANEL_PRESENTED monotonic_ns=(\d+)', segment)]
                intervals = [(b-a)/1e6 for a,b in zip(timestamps,timestamps[1:])]
                row = {'warm': warm, 'command_to_ready_present_ms': (ready-started)/1e6,
                       'opening_present_intervals_ms': intervals,
                       'animation_window_ms': 220,
                       'opening_animation_intervals_ms': [(b-a)/1e6 for a,b in zip(timestamps,timestamps[1:]) if b<=started+220_000_000],
                       'opening_draw_ms': [float(n) for n in re.findall(r'PANEL_FRAME processing_ms=([\d.]+)', segment)]}
                capture=raw/('sound.png' if i==0 else f'sound-{i}.png')
                run('grim',str(capture))
                picture=Image.open(capture).convert('RGB');scale=picture.width/1920
                area=picture.crop((round((1920-464+32+20)*scale),round((70+32+160)*scale),round((1920-464+32+372)*scale),round((70+32+190)*scale)))
                bright=sum(r>140 and g>140 and b>90 for r,g,b in list(area.get_flattened_data()))
                row['slider_bright_pixels']=bright
                assert bright>1000*scale*scale, 'presented panel has no visible volume track'

                if i == 0:
                    if compositor == 'hyprland':
                        run('hyprctl', 'dismissnotify', '-1')
                        run('hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))')
                    if seconds:
                        print('PANEL_IDLE_SAMPLE warm', flush=True)
                        result['warm_idle'] = matrix.idle(client.pid, seconds)
                        result['warm_after_idle'] = memory()
                    keyboard=subprocess.Popen(['wtype','-s','200','-k','Tab','-s','2500'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                    matrix.wait_log(log,lambda t:'INPUT_KEY sym=XK_Tab' in t)
                    time.sleep(.1)
                    # Synthetic selection, inside the private compositor only.
                    call('clipboard-set', 'private clipboard fixture Ω', target_name='bead')
                    time.sleep(.2)
                    assert call('clipboard-get', target_name='bead')['result'] == 'private clipboard fixture Ω'
                    assert run('wl-paste','--no-newline').stdout=='private clipboard fixture Ω'
                    peer=subprocess.Popen(['wl-copy','--foreground','--type','text/plain'],env=env,stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
                    try:
                        peer.stdin.write(b'plain text peer');peer.stdin.close()
                        deadline=time.monotonic()+3
                        while call('clipboard-get',target_name='bead')['result']!='plain text peer':
                            if time.monotonic()>deadline:raise RuntimeError('plain MIME peer transfer failed')
                            time.sleep(.05)
                    finally:stop(peer)
                    result['clipboard_peer_utf8_and_plain'] = True
                    result['clipboard'] = True
                    keyboard.wait(timeout=5)
                    pointer = matrix.services.Probe([str(target/'examples/private-pointer')], env, raw/'pointer.log')
                    pointer.wait('POINTER_READY')
                    state = call('state')['result']
                    assert len(state['regions']) == 23, state
                    regions, width = state['regions'], state['size'][0]
                    pointer_to(regions[2+10], width)
                    pointer.command('press', 'POINTER_DONE')
                    audio(lambda s: abs(s.get('bead.desktop.audio.percent', -1)-50) < .1)
                    pointer_to(regions[2+15], width)
                    audio(lambda s: abs(s.get('bead.desktop.audio.percent', -1)-75) < .1)
                    pointer.command('release', 'POINTER_DONE')
                    run('wtype', '-s', '200', '-k', 'Left', '-s', '200')
                    audio(lambda s: abs(s.get('bead.desktop.audio.percent', -1)-70) < .1)
                    pointer_to(regions[1], width)
                    pointer.command('click', 'POINTER_DONE')
                    audio(lambda s: s.get('bead.desktop.audio') == 'volume_off')
                    result['controls'] = {'slider_click': 50, 'slider_drag': 75, 'slider_key': 70, 'mute': True}
                    run('wtype', '-s', '200', '-k', 'Escape')
                    matrix.wait_log(log, lambda _: not call('state')['result']['target_open'])
                    result['escape'] = True
                elif i == 1:
                    pointer.command('move 1000 600', 'POINTER_DONE')
                    pointer.command('click', 'POINTER_DONE')
                    matrix.wait_log(log, lambda _: not call('state')['result']['target_open'])
                    result['outside_dismiss'] = True
                elif i == 2:
                    state = call('state')['result']
                    pointer_to(state['regions'][0], state['size'][0])
                    pointer.command('click', 'POINTER_DONE')
                    matrix.wait_log(log, lambda _: not call('state')['result']['target_open'])
                    result['close_button'] = True
                else:
                    call('close')
                wait_closed()
                legacy=json.loads(call('state',target_name='nacre')['result'])
                assert legacy['edgeMenu']=='' and not legacy['bead']['panelOpen']
                time.sleep(.3)
                idle = memory()
                row.update(idle=idle, delta_from_bar_mib=idle['total_mib']-baseline['total_mib'])
                result['cycles'].append(row)
                print('PANEL_CYCLE '+json.dumps({'cycle':i,'warm_mib':warm['total_mib'],
                      'idle_mib':idle['total_mib'],'delta_mib':row['delta_from_bar_mib']}),flush=True)
            if seconds:
                print('PANEL_IDLE_SAMPLE closed', flush=True)
                result['closed_idle'] = matrix.idle(client.pid, seconds)
            if compositor == 'hyprland':
                run('hyprctl', 'output', 'create', 'headless', 'bead-panel-other')
                time.sleep(.3)
                call('open', 'sound', 'bead-panel-other')
                time.sleep(.4)
                assert call('state')['result']['surfaces'] == 1
                call('open', 'sound', 'bead-bench')
                time.sleep(.3)
                assert call('state')['result']['surfaces'] == 1
                call('open', 'sound', 'bead-panel-other')
                time.sleep(.3)
                run('hyprctl', 'output', 'remove', 'bead-panel-other')
                matrix.wait_log(log, lambda t: 'OUTPUT_REMOVED' in t)
                assert not call('state')['result']['resident']
                assert call('state', target_name='bead')['result']['surfaces'] == 1
                result['move_and_output_removal'] = True
            if compositor == 'niri' and os.environ.get('BEAD_PANEL_OUTPUT_OFF'):
                name=next(iter(json.loads(run('niri','msg','-j','outputs').stdout).values()))['name']
                call('open','sound');time.sleep(.4)
                try:
                    response=run('niri','msg','output',name,'off')
                    time.sleep(1)
                    result['nested_output_off']={'response':response.stdout.strip(),'released':not call('state')['result']['resident']}
                finally:run('niri','msg','output',name,'on')
            if os.environ.get('BEAD_PANEL_RAPID'):
                def fast(method):
                    with unix_socket.socket(unix_socket.AF_UNIX,unix_socket.SOCK_STREAM) as stream:
                        stream.settimeout(3);stream.connect(str(socket))
                        stream.sendall((json.dumps({'version':1,'id':1,'target':'bead.panel','method':method,'args':['sound'] if method=='open' else []})+'\n').encode())
                        reply=json.loads(stream.makefile('rb').readline(65537))
                        assert reply.get('error') is None,reply
                fast('open');time.sleep(.3)
                for _ in range(10):fast('close');fast('open');time.sleep(.03)
                time.sleep(.5)
                result['rapid_reversal']=call('state')['result']['target_open']
                assert result['rapid_reversal'], 'a stale closed candidate overrode the latest open intent'
                run('wtype','-s','200','-k','Tab','-s','100','-k','Return','-s','200')
                assert not call('state')['result']['target_open'], 'focused close activation failed'
                result['keyboard_close']=True
                wait_closed()
            time.sleep(.3)
            result['lifecycle_idle']=memory()
            result['lifecycle_delta_mib']=result['lifecycle_idle']['total_mib']-baseline['total_mib']
            text = matrix.log_text(log)
            assert 'PRESENTATION clock_id=1' in text
            assert 'UI_ERROR' not in text and 'PANIC ' not in text
            result['memory_acceptance'] = all(r['delta_from_bar_mib'] <= 5 for r in result['cycles']) and result['lifecycle_delta_mib']<=5 and (not seconds or result['closed_idle']['pss_plus_drm_mib']-baseline['total_mib']<=5)
            (raw/'panel-proof.json').write_text(json.dumps(result, indent=2)+'\n')
            print('PANEL_PROOF '+json.dumps({k:v for k,v in result.items() if k!='cycles'}), flush=True)
            assert result['memory_acceptance'], 'closed-idle exceeds bar + 5MiB'
    finally:
        if pointer:
            stop(pointer.process)
            pointer.reader.join(timeout=2)
        if client:
            stop(client)
        stop(pw)
    assert 'STOP surfaces=0' in log.read_text()
