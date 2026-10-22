# SPDX-License-Identifier: GPL-3.0-only
"""Private nested proof for per-output bar sessions and their settled memory."""
import hashlib
import importlib.util
import json
import math
import os
import pathlib
import re
import socket as unix_socket
import statistics
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
_measure_spec = importlib.util.spec_from_file_location('measure', ROOT/'spikes/measure.py')
measure = importlib.util.module_from_spec(_measure_spec)
_measure_spec.loader.exec_module(measure)


def record_pool_bounds(result, log):
    if result['role'] != 'candidate':
        return
    rows = [dict((key, int(value)) for key, value in re.findall(
        r'\b(pool_bytes|buffer_bytes)=(\d+)', line))
        for line in log.splitlines()
        if line.startswith('BUFFER output=') and ' allocation_count=' in line]
    if not rows or any('pool_bytes' not in row or 'buffer_bytes' not in row for row in rows):
        raise RuntimeError('candidate must report shared-memory pool and frame byte counts')
    for row in rows:
        if row['pool_bytes'] != 3*((row['buffer_bytes']+63)//64)*64:
            raise RuntimeError(f'geometry pool retained more than three current frames: {row}')
    result['bounded_geometry_pools'] = rows


def finish_memory_only(result, client, stop, text_log, pswpin_pages):
    if 'UI_ERROR ' in text_log() or 'OUTPUT_SESSION_ERROR ' in text_log():
        raise RuntimeError('per-output proof logged a UI or session error')
    record_pool_bounds(result, text_log())
    stop(client)
    if client.returncode != 0:
        raise RuntimeError(f'memory-only shell exited {client.returncode}')
    result['clean_stop'] = 'STOP surfaces=0' in text_log()
    if not result['clean_stop']:
        raise RuntimeError('per-output shell did not report STOP surfaces=0')
    result['host_pswpin_after'] = pswpin_pages()
    delta_pages = result['host_pswpin_after']-result['host_pswpin_before']
    result['host_swap_in_delta_mib'] = delta_pages*os.sysconf('SC_PAGE_SIZE')/1024**2
    if result['host_swap_in_delta_mib'] >= 1:
        raise RuntimeError(f'host swap-in reached stop line: {result["host_swap_in_delta_mib"]:.3f} MiB')
    result['status'] = 'memory-only-done'
    print('PER_OUTPUT_PROOF '+json.dumps({k:v for k,v in result.items() if k != 'phases'}, sort_keys=True), flush=True)
    return result


def sample(env, raw, compositor, stop):
    env = {**env, 'DBUS_SYSTEM_BUS_ADDRESS': env['DBUS_SESSION_BUS_ADDRESS'],
           'PIPEWIRE_REMOTE': 'bead-private-unavailable', 'BEAD_PRIVATE_SERVICES_TEST': '1',
           'BEAD_PRIVATE_INPUT_PROOF': '1'}
    role = os.environ.get('BEAD_PER_OUTPUT_ROLE', 'candidate')
    if role not in {'control', 'candidate'}:
        raise ValueError('BEAD_PER_OUTPUT_ROLE must be control or candidate')
    binary = pathlib.Path(os.environ.get(
        'BEAD_NATIVE_BINARY', str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/nacre-shell')))
    socket = pathlib.Path(env['XDG_RUNTIME_DIR'])/'per-output.sock'
    log_path = raw/(compositor+'-per-output-'+role+'.log')
    log_path.write_text('')
    client = None
    pointer_probe = None
    result = {'compositor': compositor, 'role': role, 'binary': str(binary),
              'phases': [], 'multi_output_supported': compositor == 'hyprland',
              'host_pswpin_before': _pswpin_pages()}

    def text_log():
        return log_path.read_text(errors='replace')

    def outputs():
        if compositor == 'hyprland':
            return json.loads(subprocess.check_output(['hyprctl', '-j', 'monitors'], env=env,
                                                       text=True, timeout=5))
        return json.loads(subprocess.check_output(['niri', 'msg', '-j', 'outputs'], env=env,
                                                   text=True, timeout=5))

    def wait(predicate, label, timeout=15):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            value = predicate(text_log())
            if value:
                return value
            if client is not None and client.poll() is not None:
                raise RuntimeError(f'per-output client exited while waiting for {label}: {client.returncode}')
            time.sleep(.05)
        raise RuntimeError(f'per-output proof timed out waiting for {label}')

    def wait_outputs(names):
        deadline = time.monotonic()+10
        while time.monotonic() < deadline:
            current = outputs()
            current_names = ({m['name'] for m in current} if compositor == 'hyprland'
                             else set(current))
            if current_names == set(names):
                return current
            time.sleep(.05)
        raise RuntimeError(f'private outputs mismatch: expected {sorted(names)}, got {sorted(current_names)}')

    def command(*args):
        return subprocess.run(args, env=env, capture_output=True, text=True, check=True, timeout=8)

    def mem_sample():
        memory = measure.memory(client.pid)
        if memory['swap_pss_mib'] != 0:
            raise RuntimeError(f"UI SwapPss must remain zero, got {memory['swap_pss_mib']:.3f} MiB")
        drm = measure.drm_memory(client.pid)
        drm_known = drm is not None
        drm_mib = sum(drm.values()) if drm_known else None
        accounted = memory['pss_mib'] + memory['swap_pss_mib'] + (drm_mib or 0)
        row = {**memory, 'monotonic_ns': time.monotonic_ns(),
               'drm_known': drm_known, 'drm_regions_mib': drm,
               'drm_mib': drm_mib, 'pss_swap_known_drm_mib': accounted}
        # Preserve the complete kernel rollup with every sample; desktop gate is
        # a same-host, same-output-count control comparison, not an absolute cap.
        row['smaps_rollup'] = pathlib.Path(f'/proc/{client.pid}/smaps_rollup').read_text()
        return row

    def settled(label, forbidden_commit_output=None, forbidden_commit_offset=0):
        # Warm up after each topology/scale change, then retain five one-second samples.
        warmup_end = time.monotonic()+10
        while time.monotonic() < warmup_end:
            if forbidden_commit_output:
                stale = [line for line in text_log().splitlines()[forbidden_commit_offset:]
                         if line.startswith('UI_COMMIT ')
                         and f'output={forbidden_commit_output}' in line]
                if stale:
                    raise RuntimeError(f'removed output received a stale commit: {stale[-1]}')
            time.sleep(.05)
        rows = []
        for _ in range(5):
            rows.append(mem_sample())
            if forbidden_commit_output:
                stale = [line for line in text_log().splitlines()[forbidden_commit_offset:]
                         if line.startswith('UI_COMMIT ')
                         and f'output={forbidden_commit_output}' in line]
                if stale:
                    raise RuntimeError(f'removed output received a stale commit: {stale[-1]}')
            time.sleep(1)
        result = {'label': label, 'samples': rows}
        result['geometry'] = outputs()
        for key in ('pss_mib', 'swap_pss_mib', 'pss_anon_mib', 'pss_file_mib', 'pss_shmem_mib'):
            result[key+'_median'] = statistics.median(row[key] for row in rows)
        known = [row['drm_mib'] for row in rows if row['drm_known']]
        result['drm_known_median_mib'] = statistics.median(known) if len(known) == len(rows) else None
        result['drm_unknown_samples'] = sum(not row['drm_known'] for row in rows)
        result['pss_swap_known_drm_median_mib'] = statistics.median(
            row['pss_swap_known_drm_mib'] for row in rows)
        diagnostics = ipc('state')
        result['cache_bytes'] = diagnostics.get('cache_bytes')
        result['heap_bytes'] = diagnostics.get('heap_bytes')
        resources = {}
        live_outputs = ({output['name'] for output in result['geometry']}
                        if compositor == 'hyprland' else set(result['geometry']))
        for line in text_log().splitlines():
            if line.startswith('UI_RESOURCES '):
                output = re.search(r'\boutput=(\S+)', line)
                if output and output.group(1) not in live_outputs:
                    continue
                resources[output.group(1) if output else 'shared'] = {
                    key: int(value) for key, value in re.findall(r'\b(\w+)=(\d+)', line)}
        result['resources_by_output'] = resources
        print('PER_OUTPUT_MEMORY '+json.dumps({k:v for k,v in result.items() if k != 'samples'}, sort_keys=True), flush=True)
        result['role'] = role
        return result

    def session_commits(name, count=1, after=0):
        def found(data):
            rows = [line for line in data.splitlines()[after:]
                    if line.startswith('UI_COMMIT ') and re.search(rf'\boutput={re.escape(name)}\b', line)]
            return rows if len(rows) >= count else None
        return wait(found, f'{count} output-specific commit(s) for {name}')

    def monitor_setup(name, scale=1.5):
        x = 0 if name == 'bead-bench' else 1920
        command('hyprctl', 'eval',
                f'hl.monitor({{output="{name}",mode="2880x1800@120",position="{x}x0",scale={scale}}})')

    def create_output(name):
        command('hyprctl', 'output', 'create', 'headless', name)
        monitor_setup(name)
        wait_outputs({'bead-bench', name})

    def remove_output(name):
        command('hyprctl', 'output', 'remove', name)
        wait_outputs({'bead-bench'} if name != 'bead-bench' else set())

    def ipc(method, *args, target='bead.panel'):
        response = subprocess.run([str(binary), 'ipc', '--socket', str(socket), '--json',
                                   target, method, *args], cwd=ROOT, env=env,
                                  capture_output=True, text=True, check=True, timeout=5)
        return json.loads(response.stdout)['result']

    def addressed_regions(output):
        with unix_socket.socket(unix_socket.AF_UNIX, unix_socket.SOCK_STREAM) as stream:
            stream.settimeout(5)
            stream.connect(str(socket))
            stream.sendall((json.dumps({'version': 1, 'id': 1, 'target': 'bead',
                                       'method': 'regions', 'args': [], 'output': output})+'\n').encode())
            return json.loads(stream.makefile('rb').readline(65537))

    def wait_surfaces(count, label):
        deadline = time.monotonic()+12
        state = None
        while time.monotonic() < deadline:
            state = ipc('state', target='bead')
            if state.get('surfaces') == count and state.get('panel') is False:
                return state
            time.sleep(.05)
        raise RuntimeError(f'bead state IPC did not report {count} bar surfaces for {label}: {state!r}')

    def wait_bar_buffers(count, label, scale='1.5', after=0, fresh=False):
        deadline = time.monotonic()+12
        scale_value = float(scale)
        logical_width = round(2880/scale_value)
        physical_height = math.ceil(50*scale_value)
        expected = f'logical={logical_width}x50 physical=2880x{physical_height} scale={scale}'
        while time.monotonic() < deadline:
            all_lines = text_log().splitlines()
            lines = all_lines[after:]
            rows = [line for line in (lines if fresh else all_lines) if line.startswith('BUFFER output=')
                    and expected in line]
            ids = {re.search(r'\boutput=([^ ]+)', line).group(1) for line in rows}
            commits = [line for line in lines if line.startswith('UI_COMMIT ')]
            if len(ids) >= count and commits:
                return {'outputs': sorted(ids), 'buffer_rows': rows,
                        'commit_count': len(commits)}
            if client is not None and client.poll() is not None:
                raise RuntimeError(f'per-output client exited while waiting for {label}: {client.returncode}')
            time.sleep(.05)
        raise RuntimeError(f'bar buffer/commit proof timed out for {label}: expected {count} output(s) with {expected!r}')

    def wait_panel(predicate, label, timeout=12):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            state = ipc('state')
            if predicate(state):
                return state
            time.sleep(.05)
        raise RuntimeError(f'panel state timed out waiting for {label}')

    def pointer_command(command_text):
        offset = len(pointer_probe.lines)
        pointer_probe.process.stdin.write(command_text+'\n')
        pointer_probe.process.stdin.flush()
        return pointer_probe.wait('POINTER_DONE', offset=offset)

    try:
        result['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        if compositor == 'hyprland':
            current = outputs()
            names = {m['name'] for m in current}
            if 'bead-bench' not in names:
                raise RuntimeError(f'expected measure.py virtual120 bead-bench output, got {sorted(names)}')
            # Hyprland may have a default active output besides bead-bench. Disable it
            # through the private compositor API; it is not necessarily removable headless output.
            for name in sorted(names-{'bead-bench'}):
                command('hyprctl', 'eval', f'hl.monitor({{output="{name}",disabled=true}})')
            wait_outputs({'bead-bench'})
            base = next(m for m in outputs() if m['name'] == 'bead-bench')
            if (base.get('width'), base.get('height'), base.get('scale')) != (2880, 1800, 1.5):
                raise RuntimeError(f'bead-bench geometry is not 2880x1800@1.5: {base!r}')
            initial_name = 'bead-bench'
        elif compositor == 'niri':
            current = outputs()
            if len(current) != 1:
                raise RuntimeError(f'niri scale smoke requires one private output, got {list(current)}')
            result['multi_output_status'] = 'unsupported: no supported private niri output hotplug API in this harness'
            initial_name = next(iter(current))
        else:
            raise ValueError(f'unsupported compositor: {compositor}')

        command_args = [str(binary), '--bar', '--core-views', '--renderer', 'cpu',
                        '--namespace', 'bead-per-output-proof', '--command-socket', str(socket),
                        '--trace-state']
        with log_path.open('w') as stream:
            result['client_start_monotonic_ns'] = time.monotonic_ns()
            client = subprocess.Popen(command_args, cwd=ROOT, env=env, stdout=stream,
                                      stderr=subprocess.STDOUT, start_new_session=True)
            if role == 'candidate':
                wait(lambda data: any(f'{marker} output={initial_name} ' in data for marker in ('UI_SESSION_CREATED', 'UI_SESSION_NAMED')),
                     f'initial {initial_name} output session')
                session_commits(initial_name)
                if text_log().count('UI_SESSION_CREATED ') != 1:
                    raise RuntimeError('one output must own exactly one session, including late naming')
            else:
                # Main exposes generic commits and the native compositor state
                # IPC, but lacks per-output UI_SESSION/UI_COMMIT annotations.
                wait(lambda data: 'UI_COMMIT ' in data, 'initial control bar commit')
            wait_surfaces(1, 'initial output')
            if compositor == 'hyprland':
                wait_outputs({'bead-bench'})
                wait_bar_buffers(1, 'one-output')
                initial_offset = len(text_log().splitlines())
                one = settled('one-output')
                result['phases'].append(one)
                create_output('bead-extra')
                if role == 'candidate':
                    wait(lambda data: any(f'{marker} output=bead-extra ' in data for marker in ('UI_SESSION_CREATED', 'UI_SESSION_NAMED')),
                         'second output session creation')
                    session_commits('bead-extra', after=initial_offset)
                    if text_log().count('UI_SESSION_CREATED ') != 2:
                        raise RuntimeError('adding a second output must create exactly one new session')
                    session_commits('bead-bench')
                else:
                    wait_outputs({'bead-bench', 'bead-extra'})
                wait_surfaces(2, 'two-output')
                wait_bar_buffers(2, 'two-output', after=initial_offset)
                two = settled('two-output')
                result['phases'].append(two)
                result['additional_output_pss_delta_mib'] = two['pss_mib_median']-one['pss_mib_median']
                result['additional_output_accounted_delta_mib'] = (
                    two['pss_swap_known_drm_median_mib']-one['pss_swap_known_drm_median_mib'])
                result['additional_output_categories_delta_mib'] = {
                    key: two[key+'_median']-one[key+'_median']
                    for key in ('pss_anon_mib', 'pss_file_mib', 'pss_shmem_mib', 'swap_pss_mib')}
                if os.environ.get('BEAD_PER_OUTPUT_MEMORY_ONLY') == '1':
                    return finish_memory_only(result, client, stop, text_log, _pswpin_pages)
                print(f"PER_OUTPUT_DELTA one_to_two_PSS={result['additional_output_pss_delta_mib']:.3f}MiB "
                      f"PSS+SwapPss+known-DRM={result['additional_output_accounted_delta_mib']:.3f}MiB target=<3MiB", flush=True)
                if role == 'candidate' and result['additional_output_pss_delta_mib'] >= 3:
                    raise RuntimeError(f"additional output PSS exceeded 3 MiB target: {result['additional_output_pss_delta_mib']:.3f} MiB")
                resource_rows = [{key: int(value) for key, value in
                                  re.findall(r'(\w+)=(\d+)', line)}
                                 for line in text_log().splitlines()
                                 if line.startswith('UI_RESOURCES ')]
                result['resource_counts'] = {
                    key: sorted({row[key] for row in resource_rows if key in row})
                    for key in ('stores', 'nodes', 'watchers', 'subscriptions',
                                'bindings', 'sources', 'luau_bytes', 'layers',
                                'gpu_images', 'surfaces')}
                # Input proof comes after the memory comparison so its panel
                # allocations cannot bias the one-output/two-output delta.
                pointer_module_spec = importlib.util.spec_from_file_location(
                    'services_lifecycle', ROOT/'spikes/services-lifecycle.py')
                pointer_module = importlib.util.module_from_spec(pointer_module_spec)
                pointer_module_spec.loader.exec_module(pointer_module)
                pointer_env = {**env, 'BEAD_PRIVATE_INPUT_PROOF': '1'}
                pointer_probe = pointer_module.Probe(
                    [str(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/examples/private-pointer')],
                    pointer_env, raw/'per-output-pointer.log')
                pointer_probe.wait('POINTER_READY')
                prepared = ipc('state').get('prepare_views', [])
                calendar = next((row for row in prepared if row.get('view') == 'calendar'), None)
                if calendar is None:
                    raise RuntimeError(f'calendar input region missing from panel state: {prepared!r}')
                x, y, width, height = calendar['rect']
                click_rows = []
                for output_name, offset_x in (('bead-bench', 0), ('bead-extra', 1920)):
                    click_offset = len(text_log().splitlines())
                    px = round(offset_x+x+width/2)
                    py = round(y+height/2)
                    pointer_command(f'move {px} {py} 3840 1200')
                    pointer_command('click')
                    opened = wait_panel(lambda state, expected=output_name: state.get('target_open')
                                        and state.get('prepared') and state.get('surfaces') == 1
                                        and state.get('output') == expected
                                        and state.get('view') == 'calendar',
                                        f'calendar click routed to {output_name}')
                    wait(lambda data, start=click_offset: any(
                        line.startswith('UI_PRESENTED ') and 'companion=true' in line
                        for line in data.splitlines()[start:]), f'calendar frame on {output_name}')
                    time.sleep(.3)
                    click_rows.append({'clicked_output': output_name, 'pointer_xy': [px, py],
                                       'target_open': opened['target_open'],
                                       'output': opened['output'], 'view': opened['view']})
                    ipc('close')
                    wait_panel(lambda state: not state.get('target_open')
                               and not state.get('resident'), f'{output_name} panel idle release')
                    time.sleep(2)
                    closed = mem_sample()
                    closed_total = closed['pss_swap_known_drm_mib']
                    delta_from_bar = closed_total-two['pss_swap_known_drm_median_mib']
                    if delta_from_bar > 5:
                        raise RuntimeError(f"closed panel exceeded bar+5MiB: {delta_from_bar:.3f} MiB")
                    click_rows[-1]['closed_after_2s'] = closed
                    click_rows[-1]['closed_delta_from_two_output_bar_mib'] = delta_from_bar
                pointer_probe.close()
                pointer_probe = None
                result['input_routing'] = click_rows
                before_scale = len(text_log().splitlines())
                monitor_setup('bead-extra', scale=1.25)
                deadline = time.monotonic()+10
                while time.monotonic() < deadline:
                    output = next((m for m in outputs() if m['name'] == 'bead-extra'), None)
                    commits = [line for line in text_log().splitlines()[before_scale:]
                               if line.startswith('UI_COMMIT ') and 'output=bead-extra' in line]
                    if output and output.get('scale') == 1.25 and commits:
                        break
                    time.sleep(.05)
                else:
                    raise RuntimeError('scale change did not update geometry and commit bead-extra session')
                wait_surfaces(2, 'two-output-scale-1.25')
                wait_bar_buffers(1, 'two-output-scale-1.25', scale='1.25', after=before_scale, fresh=True)
                wait_bar_buffers(1, 'unchanged-output-scale-1.5', scale='1.5', after=before_scale)
                result['phases'].append(settled('two-output-scale-1.25'))
                addressed = {name: addressed_regions(name) for name in ('bead-bench', 'bead-extra')}
                for name, width in (('bead-bench', 1920), ('bead-extra', 2304)):
                    reply = addressed[name]
                    if reply.get('error') or reply['result']['size'][0] != width:
                        raise RuntimeError(f'addressed input regions selected the wrong output: {name}: {reply!r}')
                unknown = addressed_regions('missing-output')
                if (unknown.get('error') or {}).get('code') != 'output':
                    raise RuntimeError(f'unknown output request did not fail closed: {unknown!r}')
                result['addressed_input_widths'] = {name: reply['result']['size'][0] for name, reply in addressed.items()}
                result['unknown_output_rejected'] = True
                remove_output('bead-extra')
                if role == 'candidate':
                    wait(lambda data: 'UI_SESSION_REMOVED output=bead-extra' in data,
                         'second output session teardown')
                removed_at = len(text_log().splitlines())
                wait_surfaces(1, 'one-output-after-remove')
                result['removed_output_stale_commits'] = []
                result['phases'].append(settled('one-output-after-remove', 'bead-extra', removed_at))
                readd_offset = len(text_log().splitlines())
                create_output('bead-extra')
                if role == 'candidate':
                    wait(lambda data: sum(data.count(f'{marker} output=bead-extra ') for marker in ('UI_SESSION_CREATED', 'UI_SESSION_NAMED')) >= 2,
                         'second output session recreation')
                    session_commits('bead-extra', count=1, after=readd_offset)
                if role == 'candidate' and text_log().count('UI_SESSION_CREATED ') != 3:
                    raise RuntimeError('readding an output must create one fresh session')
                wait_surfaces(2, 'two-output-readded')
                wait_bar_buffers(2, 'two-output-readded', after=readd_offset)
                result['phases'].append(settled('two-output-readded'))
                remove_output('bead-extra')
                if role == 'candidate':
                    wait(lambda data: data.count('UI_SESSION_REMOVED output=bead-extra') >= 2,
                         're-added output session teardown')
            else:
                output_name = next(iter(outputs()))
                result['niri_output'] = output_name
                wait_surfaces(1, 'niri-one-output')
                wait_bar_buffers(1, 'niri-one-output')
                result['phases'].append(settled('niri-one-output'))
                for scale in ('1.25', '1.5'):
                    before = len(text_log().splitlines())
                    command('niri', 'msg', 'output', output_name, 'scale', scale)
                    wait(lambda data, offset=before: any(
                        line.startswith('UI_COMMIT ') for line in data.splitlines()[offset:]),
                         f'niri scale {scale} redraw')
                    wait_surfaces(1, f'niri scale {scale}')
                    wait_bar_buffers(1, f'niri scale {scale}', scale=scale, after=before, fresh=True)
                    result['phases'].append(settled('niri-scale-'+scale))
                if os.environ.get('BEAD_PER_OUTPUT_MEMORY_ONLY') == '1':
                    return finish_memory_only(result, client, stop, text_log, _pswpin_pages)
        final_log = text_log()
        if 'UI_ERROR ' in final_log or 'OUTPUT_SESSION_ERROR ' in final_log:
            raise RuntimeError('per-output proof logged a UI or session error')
        record_pool_bounds(result, final_log)
        commit_names = set(re.findall(r'\bUI_COMMIT .*?\boutput=(\S+)', final_log))
        created_names = set(re.findall(r'\bUI_SESSION_(?:CREATED|NAMED) output=(\S+)', final_log))
        result['created_outputs'] = sorted(created_names)
        result['committed_outputs'] = sorted(commit_names)
        if role == 'candidate' and compositor == 'hyprland' and (not {'bead-bench', 'bead-extra'} <= created_names
                                         or not {'bead-bench', 'bead-extra'} <= commit_names):
            raise RuntimeError(f'expected both output sessions to create and commit: {result}')
        stop(client)
        client = None
        final_log = text_log()
        result['clean_stop'] = 'STOP surfaces=0' in final_log
        if not result['clean_stop']:
            raise RuntimeError('per-output shell did not report STOP surfaces=0')
        result['host_pswpin_after'] = _pswpin_pages()
        delta_pages = result['host_pswpin_after']-result['host_pswpin_before']
        result['host_swap_in_delta_mib'] = delta_pages*os.sysconf('SC_PAGE_SIZE')/1024**2
        if result['host_swap_in_delta_mib'] >= 1:
            raise RuntimeError(f'host swap-in reached stop line: {result["host_swap_in_delta_mib"]:.3f} MiB')
        result['status'] = 'done'
        print('PER_OUTPUT_PROOF '+json.dumps({k:v for k,v in result.items() if k != 'phases'}, sort_keys=True), flush=True)
    except Exception as error:
        result['status'] = 'failed'
        result['reason'] = str(error)
        raise
    finally:
        if pointer_probe is not None:
            try:
                measure.stop(pointer_probe.process)
            except (OSError, subprocess.TimeoutExpired) as error:
                result['pointer_cleanup_error'] = str(error)
            pointer_probe.reader.join(timeout=2)
        if client is not None:
            try:
                stop(client)
            except (OSError, subprocess.TimeoutExpired) as error:
                result['cleanup_error'] = str(error)
                if result.get('status') == 'done':
                    result['status'] = 'failed'
                    result['reason'] = f'client cleanup failed: {error}'
        (raw/(compositor+'-per-output-'+role+'-result.json')).write_text(
            json.dumps(result, indent=2, sort_keys=True)+'\n')


def _pswpin_pages():
    for line in pathlib.Path('/proc/vmstat').read_text().splitlines():
        if line.startswith('pswpin '):
            return int(line.split()[1])
    raise RuntimeError('/proc/vmstat does not expose pswpin')
