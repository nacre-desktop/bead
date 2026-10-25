"""Guarded private title/tab parity capture, extending the shipped driver."""
import importlib.util
import json
from pathlib import Path
import sys
import time
import os
import shutil
import subprocess
import threading

base = Path(__file__).resolve().parent
spikes = base / 'candidate/spikes'
sys.path.insert(0, str(spikes))
import capture_safety

arm_child = '--run-arm' in sys.argv
if '--capture-guard-child' not in sys.argv and not arm_child:
    raise SystemExit(capture_safety.run_guarded(
        [sys.executable, __file__, *sys.argv[1:]],
        report_path=base / 'evidence' / (os.environ.get('B0108_PHASE', 'parity') + '-containment.json')))
if not arm_child:
    capture_safety.wait_for_release(
        sys.argv[sys.argv.index('--capture-guard-ready-file') + 1],
        sys.argv[sys.argv.index('--capture-guard-release-file') + 1])
    for flag in ['--capture-guard-ready-file', '--capture-guard-release-file']:
        index = sys.argv.index(flag)
        del sys.argv[index:index+2]
    sys.argv.remove('--capture-guard-child')
spec = importlib.util.spec_from_file_location('parity', spikes / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__

def sequence(self, name, trigger, seconds):
    from PIL import Image
    frames = self.root / 'seq'
    shutil.rmtree(frames, ignore_errors=True)
    frames.mkdir()
    errors = []
    def run_trigger():
        try:
            trigger()
        except Exception as error:
            errors.append(error)
    jobs = []
    started = time.monotonic()
    runner = threading.Thread(target=run_trigger)
    runner.start()
    while time.monotonic() - started < seconds:
        before = time.monotonic()
        for job in jobs:
            if job['completed'] is None and job['process'].poll() is not None:
                job['completed'] = time.monotonic()
        process = subprocess.Popen(['grim', '-s', str(self.args.seq_scale),
            str(frames / f'{len(jobs):03}.ppm')], env=self.env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        jobs.append({'process': process, 'spawned': before, 'completed': None})
        time.sleep(max(0, before + self.args.seq_interval / 1000 - time.monotonic()))
    for job in jobs:
        job['process'].wait(timeout=30)
        if job['completed'] is None:
            job['completed'] = time.monotonic()
        if job['process'].returncode:
            errors.append(RuntimeError('transition screencopy failed'))
    runner.join()
    if errors:
        raise errors[0]
    latencies = sorted(j['completed'] - j['spawned'] for j in jobs)
    fast = latencies[:max(1, len(jobs) // 4)]
    half_latency = sum(fast) / len(fast) / 2
    stamps = [round((j['spawned'] + half_latency - started) * 1000) for j in jobs]
    out = self.out / 'seq' / name
    out.mkdir(parents=True, exist_ok=True)
    for index in range(len(jobs)):
        with Image.open(frames / f'{index:03}.ppm') as image:
            if image.size != tuple(self.args.size):
                raise RuntimeError('transition frame is not at native output resolution')
            image.convert('RGB').save(out / f'{index:03}.jpg', quality=90)
    (out / 'stamps.json').write_text(json.dumps(stamps))
    (out / 'capture-clocks.json').write_text(json.dumps({
        'interval_ms': self.args.seq_interval,
        'fast_quartile_mean_capture_ms': half_latency * 2000,
        'jobs': [{'spawn_ms': (j['spawned']-started)*1000,
                  'observed_complete_ms': (j['completed']-started)*1000} for j in jobs]}))
    return stamps
m.Arm.sequence = sequence

original_spawn = m.Arm.spawn
def spawn(self, command, name, *args, **kwargs):
    if self.name == 'bead' and name == 'shell':
        # This flag makes every panel press a pearl drag; exercise normal clicks.
        self.env.pop('BEAD_PRIVATE_INPUT_PROOF', None)
        command = [*command, '--trace-state']
    return original_spawn(self, command, name, *args, **kwargs)
m.Arm.spawn = spawn

original_start = m.Arm.start_shell
def start_shell(self):
    original_start(self)
    self.steps([('bar-window',), ('pointer', 960, 600)])
    time.sleep(1)
    windows = json.loads(self.run(['niri', 'msg', '--json', 'windows']).stdout)
    if not any(w.get('is_focused') and w.get('title') == 'Bar parity fixture' for w in windows):
        raise RuntimeError('the title fixture is not the focused real window')
m.Arm.start_shell = start_shell

def selected(self, page):
    state = self.dispatch('nacre', 'state')
    if isinstance(state, str): state = json.loads(state)
    # Registry indices are independent of the displayed tab order in both shells.
    indices = {'today': 0, 'weather': 1, 'media': 2, 'system': 3}
    return state.get('dashboard') and state.get('tab') == indices[page]

original_steps = m.Arm.steps
def steps(self, operations):
    for step in operations:
        if step[0] == 'click':
            self.pointer(step[1], step[2], 'click', settle=0)
        elif step[0] == 'page-ready':
            m.wait_until(lambda: selected(self, step[1]), 15, 'selected semantic page '+step[1])
        else:
            original_steps(self, [step])
m.Arm.steps = steps

m.STATES['bar-title-open'] = (
    [('pointer-fast', 960, 25)], [('pointer', 960, 900), ('ui', 'nacre', 'close')], 1.2)
for index, x in enumerate([610, 842, 1080, 1310]):
    name = 'tabs-' + str(index)
    # Begin from System, except System begins from Today. The strip is already open.
    initial_x = 610 if index == 3 else 1310
    m.STATE_PREPARE[name] = [('ui', 'drawers', 'toggle', 'dashboard'), ('sleep', 3),
        ('click', initial_x, 78), ('page-ready', 'today' if index == 3 else 'system'), ('sleep', 1)]
    m.STATES[name] = ([('click', x, 78)],
        [('pointer', 960, 900), ('ui', 'nacre', 'close')], 1.0)
    m.STATE_SETTLE[name] = 0.6
for kind in ['hover', 'pressed']:
    m.STATE_PREPARE['tabs-' + kind] = [('ui', 'drawers', 'toggle', 'dashboard'), ('sleep', 3),
        ('click', 610, 78), ('page-ready', 'today'), ('sleep', 1)]
    m.STATES['tabs-' + kind] = ([('pointer-fast' if kind == 'hover' else 'press', 842, 78)],
        [('release',), ('pointer', 960, 900), ('ui', 'nacre', 'close')], 0)
    m.STATE_SETTLE['tabs-' + kind] = 0.08

original_grab = m.Arm.grab
def grab(self, path, *args, **kwargs):
    if path.stem.startswith('tabs-') and path.stem[-1:] in '0123':
        page = ['today','media','weather','system'][int(path.stem[-1])]
        if not selected(self, page):
            raise RuntimeError('tab click did not select '+page)
    original_grab(self, path, *args, **kwargs)
    from PIL import Image
    with Image.open(path) as shot:
        if shot.size != tuple(self.args.size):
            raise RuntimeError('capture is not the full requested output')
    if self.name == 'bead' and os.environ.get('B0108_CONTROL') != '1':
        title = None
        for line in (self.logs / 'shell.log').read_text(errors='replace').splitlines():
            if line.startswith('UI_STATE '):
                state = json.loads(line[9:])
                title = state.get('bead.focused_title', title)
        if title != 'Bar parity fixture':
            raise RuntimeError('bar title changed while a shell panel was open: ' + str(title))
m.Arm.grab = grab
original_drive = m.Arm.drive
def memory_drive(self):
    def vm_swapin():
        return int(next(line.split()[1] for line in Path('/proc/vmstat').read_text().splitlines()
                        if line.startswith('pswpin ')))
    start_swapin = vm_swapin()
    phases = {}
    diagnostics = {}
    def sample(name):
        rows = []
        for _ in range(5):
            row = self.rss()
            required = ['Pss', 'Pss_Anon', 'Pss_Shmem', 'Pss_File', 'SwapPss', 'DRM']
            if any(row.get(key) is None for key in required) or row['SwapPss'] != 0:
                raise RuntimeError('missing memory components or nonzero SwapPss')
            rows.append(row)
            time.sleep(.1)
        phases[name] = rows
        reply = self.socket_call(self.runtime/'bead-command.sock', 'bead.panel', 'state', [])
        value = reply.get('result', {})
        diagnostics[name] = {key: value[key] for key in ['heap_bytes', 'cache_bytes'] if key in value}
    def ready(page):
        def committed():
            latest = None
            for line in (self.logs / 'shell.log').read_text(errors='replace').splitlines():
                if line.startswith('UI_STATE '):
                    value = json.loads(line[9:])
                    if 'dashboard.page' in value:
                        latest = value
            return latest and latest.get('panel.open') and latest.get('dashboard.page') == page
        m.wait_until(committed, 15, 'the requested page ' + page)
        time.sleep(2.5)
    self.errors = []
    self.dispatch_trace = []
    self.state_start = time.monotonic()
    shots = self.out / 'shots'
    shots.mkdir(exist_ok=True)
    sample('idle')
    self.grab(shots / 'idle.png')
    self.steps([('ui', 'drawers', 'toggle', 'dashboard')])
    ready('today')
    sample('today')
    self.grab(shots / 'today-open.png')
    panel = self.socket_call(self.runtime/'bead-command.sock', 'bead.panel', 'state', [])
    (self.out/'open-layout.json').write_text(json.dumps(panel))
    for page, x in [('media', 842), ('weather', 1080), ('system', 1310)]:
        self.steps([('click', x, 78)])
        ready(page)
        sample(page)
    self.steps([('pointer', 960, 900), ('ui', 'nacre', 'close')])
    time.sleep(2.5)
    sample('closed')
    if self.errors or vm_swapin() != start_swapin:
        raise RuntimeError('private memory arm errors or host swap-in')
    (self.out / 'memory.json').write_text(json.dumps({'backend': 'cpu',
        'source_revision': (Path(self.args.bead_shell).parent/'REVISION').read_text().strip(),
        'host_swap_in_pages': vm_swapin()-start_swapin, 'components_kib': phases, 'diagnostics': diagnostics}, indent=1))
    (self.out / 'drive.json').write_text(json.dumps({'idle': {
        'errors': [], 'rss_kib': phases['idle'][2], 'rss_closed_kib': phases['closed'][2]}}))

def drive(self, names):
    if os.environ.get('B0108_MEMORY') == '1':
        memory_drive(self)
    else:
        original_drive(self, names)
    if self.name == 'bead' and os.environ.get('B0108_CONTROL') != '1':
        observed = []
        for line in (self.logs / 'shell.log').read_text(errors='replace').splitlines():
            if line.startswith('UI_STATE '):
                state = json.loads(line[9:])
                if 'bead.focused_title' in state:
                    observed.append(state['bead.focused_title'])
        first = observed.index('Bar parity fixture')
        after = observed[first:]
        (self.out / 'title-proof.json').write_text(json.dumps({
            'primary_states_after_focus': len(after),
            'changed_title_states': sum(t != 'Bar parity fixture' for t in after)}))
        if any(t != 'Bar parity fixture' for t in after):
            raise RuntimeError('a primary UI state lost the focused title during the capture')
m.Arm.drive = drive
if os.environ.get('B0108_MEMORY') == '1':
    for name in ['bar-title-open', 'tabs-0', 'tabs-1', 'tabs-2', 'tabs-3']:
        opening, closing, _ = m.STATES[name]
        m.STATES[name] = (opening, closing, 0)
        m.STATE_SETTLE[name] = 1.0
raise SystemExit(m.main())
