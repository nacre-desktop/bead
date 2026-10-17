#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Real menu.query typing: private daemon/catalog, exact reply-to-present proof."""
import hashlib, importlib.util, json, os, pathlib, re, shutil, signal, subprocess, sys, tempfile, time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


matrix = load('matrix', ROOT / 'spikes/bar-matrix.py')
clocks = load('owned_clocks', ROOT / 'spikes/process-clock.py')


def presented_catalog(text, count):
    ready = re.search(r'MENU_RESULT ok=true catalog_rev=[1-9]\d* count=' + str(count) + r'\b', text)
    if not ready:
        return False
    tail = text[ready.end():]
    commit = re.search(r'UI_COMMIT generation=(\d+).*update=(\d+).*source_error=false', tail)
    return bool(commit and re.search(r'UI_PRESENTED generation=' + commit[1] + r' .*update=' + commit[2] + r' ', tail[commit.end():]))


def sample(env, raw, compositor, stop):
    helper = pathlib.Path(os.environ['BEAD_MENU_ENGINE_BINARY']).resolve()
    binary = pathlib.Path(os.environ['BEAD_HARDENING_BINARY']).resolve()
    assert os.getpriority(os.PRIO_PROCESS, 0) == 10, 'private capture must run at nice10'
    assert helper.is_file() and binary.is_file()
    client = daemon = pointer = keeper = None
    workers = {}
    rows = []
    diagnostic = None
    profile_mode = os.environ.get('BEAD_TYPING_PROFILE', 'clocks')
    assert profile_mode in ['clocks', '1']
    log = raw / 'engine-typing.log'

    def stop_engine():
        clocks.stop_owned_tree(daemon, stop)

    try:
        with tempfile.TemporaryDirectory(prefix='bead-menu-engine-') as directory:
            root = pathlib.Path(directory)
            shutil.copytree(pathlib.Path(os.environ.get('BEAD_MENU_ENGINE_BUNDLE', str(ROOT / 'shell'))), root / 'shell')
            for name in ['home', 'config', 'state', 'cache', 'data/applications', 'empty', 'bin', 'runtime']:
                (root / name).mkdir(parents=True, exist_ok=True)
            (root / 'runtime').chmod(0o700)
            endpoint = root / 'runtime/nacre/daemon.sock'
            (root / 'bin/nacrectl').symlink_to(helper)
            for index in range(320):
                name = 'Alpha' if index < 160 else 'Application'
                (root / f'data/applications/fixture-{index:04}.desktop').write_text(
                    f'[Desktop Entry]\nType=Application\nName={name} {index:04}\n'
                    'Comment=Synthetic application\nExec=bead-fixture-never-activate\nIcon=fixture\nCategories=Utility;\n')
            daemon_env = dict(env, HOME=str(root / 'home'), XDG_CONFIG_HOME=str(root / 'config'),
                              XDG_STATE_HOME=str(root / 'state'), XDG_CACHE_HOME=str(root / 'cache'),
                              XDG_DATA_HOME=str(root / 'data'), XDG_DATA_DIRS=str(root / 'empty'),
                              NACRE_DAEMON_SOCKET=str(endpoint), NACRE_NIRI_HOVER_STREAM='0',
                              NACRE_COMPOSITOR=compositor,
                              DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],
                              PIPEWIRE_REMOTE='bead-private-unavailable')
            # The outer harness removes live compositor endpoints. Explicitly
            # choose its nested backend instead of inheriting desktop identity.
            runtime = pathlib.Path(env['XDG_RUNTIME_DIR']).resolve()
            assert runtime != pathlib.Path(f'/run/user/{os.getuid()}'), 'live runtime rejected'
            if compositor == 'niri':
                assert pathlib.Path(env['NIRI_SOCKET']).resolve().is_relative_to(runtime)
                assert 'HYPRLAND_INSTANCE_SIGNATURE' not in env
            else:
                assert env['HYPRLAND_INSTANCE_SIGNATURE'] and 'NIRI_SOCKET' not in env
                assert (runtime / 'hypr' / env['HYPRLAND_INSTANCE_SIGNATURE'] / '.socket.sock').exists()
            # Startup-only native CLI probes, never a per-key process or transport.
            def call(method, params):
                result = subprocess.run([str(root / 'bin/nacrectl'), 'call', '--timeout', '2000',
                                         method, json.dumps(params)], env=daemon_env,
                                        capture_output=True, text=True, timeout=4)
                with (raw / 'engine-cli-probes.jsonl').open('a') as evidence:
                    evidence.write(json.dumps({'method': method, 'params': params,
                        'returncode': result.returncode, 'stdout': result.stdout,
                        'stderr': result.stderr}) + '\n')
                return json.loads(result.stdout) if result.returncode == 0 else None

            with (raw / 'engine-daemon.log').open('w') as errors:
                daemon = subprocess.Popen([str(helper), 'daemon'], cwd=root, env=daemon_env,
                                          stdout=errors, stderr=subprocess.STDOUT, start_new_session=True)
                deadline = time.monotonic() + 30
                initial = None
                while time.monotonic() < deadline:
                    assert daemon.poll() is None, 'private engine exited before readiness'
                    if endpoint.exists():
                        status = call('daemon.status', {})
                        if status and status.get('services', {}).get('menu', {}).get('status') == 'running':
                            initial = call('menu.query', {'query': '', 'mode': 'apps', 'route': 'apps'})
                            if initial and initial.get('catalog_rev', 0) > 0 and len(initial.get('order', [])) == 320:
                                assert set(initial['order']) == set(initial['items'])
                                break
                    time.sleep(.05)
                else:
                    raise RuntimeError('real catalog did not become ready')
                (raw / 'engine-ready.json').write_text(json.dumps({
                    'catalog_rev': initial['catalog_rev'], 'count': len(initial['order']),
                    'methods_running': True, 'helper_sha256': hashlib.sha256(helper.read_bytes()).hexdigest()}, indent=2) + '\n')
                assert os.getpriority(os.PRIO_PROCESS, daemon.pid) == 10
                settle = float(os.environ.get('BEAD_ENGINE_SETTLE_SECONDS', '0'))
                assert 0 <= settle <= 60
                if settle:
                    time.sleep(settle)
                if os.environ.get('BEAD_ENGINE_LIFECYCLE_ONLY') == '1':
                    def observe():
                        # Workers expire after30s idle. Discard a partial read
                        # and retry the WHOLE group if one exits mid-sample.
                        # Never count a vanished/unmeasured process as zero.
                        def live_members():
                            members = set()
                            exited = []
                            for pid in {daemon.pid, *clocks.descendants(daemon)}:
                                # A zombie has exited and has no userspace memory,
                                # but can remain in the child list until reaped.
                                state = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                                if state == 'Z':
                                    assert pid != daemon.pid, 'private daemon exited during observation'
                                    exited.append({'pid': pid, 'state': state})
                                else:
                                    members.add(pid)
                            return members, exited
                        for attempt in range(5):
                            assert daemon.poll() is None, 'private daemon exited during observation'
                            processes = []
                            try:
                                members, exited = live_members()
                            except (FileNotFoundError, ProcessLookupError):
                                continue
                            processes = []
                            try:
                                for pid in sorted(members):
                                    memory = matrix.measure.memory(pid)
                                    drm = matrix.measure.drm_memory(pid)
                                    devices = 0
                                    for fd in pathlib.Path(f'/proc/{pid}/fd').iterdir():
                                        try:
                                            devices += os.readlink(fd).startswith('/dev/dri/')
                                        except FileNotFoundError:
                                            pass
                                    assert drm is not None or devices == 0, 'missing DRM measurement for an open DRM device'
                                    processes.append({'pid': pid, 'memory': memory, 'drm_mib': drm,
                                                      'drm_fds': devices, 'pss_plus_drm_swap_mib': memory['pss_mib'] + (drm or 0) + memory['swap_pss_mib']})
                            except (FileNotFoundError, ProcessLookupError):
                                continue
                            try:
                                after, exited_after = live_members()
                            except (FileNotFoundError, ProcessLookupError):
                                continue
                            if members != after:
                                continue
                            return {'processes': processes, 'pss_plus_drm_swap_mib': sum(p['pss_plus_drm_swap_mib'] for p in processes),
                                    'membership_stable': True, 'observation_retries': attempt, 'exited_children': exited + exited_after}
                        raise RuntimeError('no stable daemon worker group for memory observation')
                    lifecycle = load('engine_launcher_lifecycle', ROOT / 'spikes/launcher-proof.py')
                    result = lifecycle.sample(dict(env, BEAD_LAUNCHER_ENGINE_SOCKET=str(endpoint)), raw, compositor, stop, observe, bundle_root=root)
                    stop_engine(); daemon = None
                    (raw / 'engine-lifecycle-proof.json').write_text(json.dumps({
                        'compositor': compositor, 'valid': result['valid'], 'owned_workers_cleaned': True,
                        'helper_sha256': hashlib.sha256(helper.read_bytes()).hexdigest()}, indent=2) + '\n')
                    assert result['valid'], 'control budget observation failed; result is NOT a passing proof'
                    return
                daemon_clock = clocks.OwnedCpuClock(daemon)

                def worker_samples():
                    live = clocks.descendants(daemon)
                    assert set(workers).issubset(live), 'worker exited during CPU capture; do not lose its accounting'
                    for pid in live:
                        if pid not in workers:
                            assert os.getpriority(os.PRIO_PROCESS, pid) == 10, 'provider worker must use nice10'
                            workers[pid] = clocks.OwnedCpuClock(clocks.OwnedDescendant(daemon, pid))
                    return {pid: clock.sample_ns() for pid, clock in workers.items()}

                if os.environ.get('BEAD_ENGINE_COMPONENT_ONLY') == '1':
                    component = load('menu_component', ROOT / 'spikes/menu-component.py')
                    component.sample(endpoint, initial['catalog_rev'], daemon_clock, worker_samples,
                                     raw / 'menu-component.json', compositor, helper)
                    stop_engine(); daemon = None
                    return
                if os.environ.get('BEAD_ENGINE_DIAGNOSTIC') == '1':
                    diagnostic = {'idle': matrix.idle(daemon.pid, 2), 'api_rows': []}
                    # Component diagnostic only: sequential native CLI calls
                    # before input, never a substitute UI or a per-key client.
                    for index in range(12):
                        before_workers = worker_samples(); before_daemon = daemon_clock.sample_ns()
                        query = 'a' if index % 2 == 0 else 'ap'
                        reply = call('menu.query', {'query': query, 'mode': 'apps', 'route': 'apps'})
                        after_daemon = daemon_clock.sample_ns(); after_workers = worker_samples()
                        assert reply and reply['query'] == query and reply['catalog_rev'] == initial['catalog_rev'] and reply['order']
                        diagnostic['api_rows'].append({'index': index, 'count': len(reply['order']),
                            'daemon_cpu_ms': (after_daemon - before_daemon) / 1e6,
                            'worker_cpu_ms': sum(value - before_workers.get(pid, 0) for pid, value in after_workers.items()) / 1e6})
                client_env = dict(env, BEAD_PRIVATE_INPUT_PROOF='1', BEAD_PROFILE_INPUT=profile_mode,
                                  BEAD_MATCHED_ICONS='1', BEAD_ICON_PATH=str(ROOT / 'shell/sample.svg'),
                                  DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],
                                  PIPEWIRE_REMOTE='bead-private-unavailable')
                keeper = subprocess.Popen(['wtype', '-s', '600000', ''], env=client_env,
                                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                command = [str(binary), '--renderer', 'cpu', '--namespace', 'bead-engine-typing',
                           '--ui', 'ui/main.luau', '--daemon-socket', str(endpoint), '--backend', 'none', '--test-keyboard']
                if compositor == 'hyprland':
                    command += ['--output', 'bead-bench']
                with log.open('w') as output:
                    client = subprocess.Popen(command, cwd=root, env=client_env, stdout=output,
                                              stderr=subprocess.STDOUT, start_new_session=True)
                    matrix.wait_log(log, lambda t: re.search(r'MENU_RPC_RETURN id=\d+ ok=true catalog_rev=\d+ count=320', t)
                                    and presented_catalog(t, 320), timeout=30)
                    time.sleep(1)
                    if compositor == 'hyprland':
                        subprocess.run(['hyprctl', 'eval', 'hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],
                                       env=env, capture_output=True, check=True, timeout=5)
                    pointer = matrix.services.Probe([str(pathlib.Path(os.environ['CARGO_TARGET_DIR']) / 'release/examples/private-pointer')],
                                                    client_env, raw / 'pointer.log')
                    pointer.wait('POINTER_READY')
                    pointer.command('move 1000 300', 'POINTER_DONE')
                    time.sleep(.3)
                    thread_before = clocks.thread_snapshot(client) if os.environ.get('BEAD_ENGINE_THREAD_PROFILE') == '1' else None
                    for index in range(int(os.environ.get('BEAD_TYPING_KEYS', '200'))):
                        offset = len(matrix.log_text(log))
                        before_workers = worker_samples()
                        before_daemon = daemon_clock.sample_ns()
                        key = ['a'] if index == 0 else ['p'] if index % 2 else ['-k', 'BackSpace']
                        subprocess.run(['wtype', '-s', '20', *key], env=env, check=True, timeout=5)
                        matrix.wait_log(log, lambda t: re.search(r'UI_PRESENTED .*update=\d+ cpu_ns=\d+', t[offset:]), timeout=10)
                        segment = matrix.log_text(log)[offset:]
                        after_daemon = daemon_clock.sample_ns()
                        after_workers = worker_samples()
                        assert 'UI_ERROR' not in segment and 'MENU_RESULT ok=false' not in segment
                        received = re.findall(r'INPUT_RECEIVED monotonic_ns=(\d+) cpu_ns=(\d+)', segment)
                        sent = re.findall(r'MENU_RPC_SENT id=(\d+)', segment)
                        returned = re.findall(r'MENU_RPC_RETURN id=(\d+) ok=true catalog_rev=(\d+) count=(\d+)', segment)
                        busy = re.findall(r'MENU_RPC_RETURN id=(\d+) ok=false code=busy retry=true', segment)
                        assert len(received) == len(returned) == 1 and 1 <= len(sent) <= 11, 'missing/extraneous completed menu RPC'
                        assert len(set(sent)) == len(sent) and busy == sent[:-1], 'only bounded retryable busy replies may precede completion'
                        rpc_id, revision, count = map(int, returned[0])
                        assert rpc_id == int(sent[-1]) and revision == initial['catalog_rev'] and count > 0
                        typed = re.search(r'MENU_RESULT ok=true catalog_rev=' + str(revision) + r' count=' + str(count) + r'\b', segment)
                        assert typed and presented_catalog(segment, count)
                        commits = re.findall(r'UI_COMMIT generation=(\d+).*update=(\d+).*source_error=false', segment[typed.end():])
                        presented = re.findall(r'UI_PRESENTED generation=(\d+) monotonic_ns=(\d+) refresh_ns=(\d+) companion=(\w+) update=(\d+) cpu_ns=(\d+)', segment)
                        start, cpu = map(int, received[0])
                        found = [p for p in presented if (p[0], p[4]) in commits and int(p[1]) > start]
                        assert len(found) == 1, 'completed result did not match its exact committed/presented update'
                        gen, ns, refresh, companion, update, end_cpu = found[0]
                        assert int(end_cpu) > cpu > 0 and int(ns) > start > 0
                        profiles = [{'stage': stage, 'wall_ms': float(wall), 'cpu_ms': float(cpu)}
                                    for stage, wall, cpu in re.findall(r'INPUT_PROFILE stage=(\w+) wall_ms=([\d.]+) cpu_ms=([\d.]+)', segment)]
                        if profile_mode == 'clocks': assert not profiles, 'clock-only capture emitted stage traces'
                        rows.append(dict(index=index, rpc_id=rpc_id, busy_retries=len(busy), catalog_rev=revision, result_count=count,
                                         update=int(update), input_ns=start, presented_ns=int(ns),
                                         latency_ms=(int(ns) - start) / 1e6, ui_cpu_ms=(int(end_cpu) - cpu) / 1e6,
                                         daemon_cpu_ms=(after_daemon - before_daemon) / 1e6,
                                         worker_cpu_ms=sum(value - before_workers.get(pid, 0) for pid, value in after_workers.items()) / 1e6,
                                         refresh_ns=int(refresh) or None, profiles=profiles))
                        time.sleep(.04)
                    assert 'PRESENTATION clock_id=1' in matrix.log_text(log)
                    thread_profile = clocks.thread_delta(thread_before, clocks.thread_snapshot(client), len(rows)) if thread_before is not None else None
                    subprocess.run(['grim', str(raw / 'engine-typing.png')], env=env, capture_output=True, check=True, timeout=8)
                    memory = {'ui': matrix.measure.memory(client.pid), 'daemon': matrix.measure.memory(daemon.pid),
                              'workers': [matrix.measure.memory(pid) for pid in workers]}
                    # Outside typing clocks: a catalog change must refresh an
                    # unchanged query through the menu topic, without a key.
                    offset = len(matrix.log_text(log))
                    (root / 'data/applications/fixture-added.desktop').write_text(
                        '[Desktop Entry]\nType=Application\nName=Ap\nExec=bead-fixture-never-activate\nIcon=fixture\n')
                    def refreshed(text):
                        segment = text[offset:]
                        for reply in re.finditer(r'MENU_RPC_RETURN id=(\d+) ok=true catalog_rev=(\d+) count=(\d+)', segment):
                            if int(reply[2]) > initial['catalog_rev'] and presented_catalog(segment[reply.end():], int(reply[3])):
                                return True
                        return False
                    # No concurrent diagnostic query: UI query + provider refresh
                    # already use the daemon's two-worker pool. Probe afterward.
                    matrix.wait_log(log, refreshed, timeout=10)
                    revised = call('menu.query', {'query': '', 'mode': 'apps', 'route': 'apps'})
                    assert revised and revised['catalog_rev'] > initial['catalog_rev'] and len(revised['order']) == 321
                    (raw / 'engine-catalog-change.json').write_text(json.dumps({
                        'catalog_rev': revised['catalog_rev'], 'count': len(revised['order']),
                        'first': revised['order'][:3]}, indent=2) + '\n')
                    assert re.search(r'MENU_RPC_RETURN id=\d+ ok=true catalog_rev=' + str(revised['catalog_rev']) + r' count=', matrix.log_text(log)[offset:])
                    pointer.close(); pointer = None
                    stop(client); client = None
                    stop(keeper); keeper = None
                stop_engine(); daemon = None
        result = dict(compositor=compositor, catalog_size=320, source='real nacre-daemon menu.query', keys=len(rows), rows=rows,
                      helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(), memory=memory,
                      cpu_method='UI: input receipt to exact presentation, process CPU; daemon/workers: high-resolution process clocks bracketing injection through presentation, includes background work',
                      worker_count=len(workers), catalog_invalidated_without_key=True,
                      settle_seconds=settle, profile_mode=profile_mode, diagnostic=diagnostic, thread_profile=thread_profile,
                      owned_workers_cleaned=True, unknown_refresh_predictions=sum(row['refresh_ns'] is None for row in rows), valid=True)
        for metric in ['latency_ms', 'ui_cpu_ms', 'daemon_cpu_ms', 'worker_cpu_ms']:
            values = [row[metric] for row in rows]
            result[metric] = {**matrix.percentiles(values), 'mean': sum(values) / len(values), 'max': max(values)}
        (raw / 'menu-engine-proof.json').write_text(json.dumps(result, indent=2) + '\n')
        print('MENU_ENGINE_PROOF keys=' + str(len(rows)) + ' valid=true', flush=True)
    finally:
        original = sys.exc_info()[1]
        failures = []
        # A failing helper must never prevent the remaining owned cleanup.
        if pointer:
            try:
                pointer.close()
            except Exception as error:
                failures.append(error)
            finally:
                stop(pointer.process)
        for process in [client, keeper]:
            if process:
                try:
                    stop(process)
                except Exception as error:
                    failures.append(error)
        if daemon:
            try:
                stop_engine()
            except Exception as error:
                failures.append(error)
        if original or failures:
            (raw / 'menu-engine-failure.json').write_text(json.dumps({
                'valid': False, 'completed_keys': len(rows), 'error': str(original),
                'cleanup_errors': [str(error) for error in failures], 'rows': rows}, indent=2) + '\n')
        if failures and original is None:
            raise failures[0]
