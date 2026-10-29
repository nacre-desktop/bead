#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0200: private nested detach/re-attach frames with the owner's niri animations.

Wraps parity-compare.py (same required arguments, `--arms bead --pointer-tool`).
Unlike pearl-release-proof.py, niri animations stay ON (the owner's session uses
the defaults) and every frame is the FULL output, so a stale surface anywhere on
the screen (the B0200 "ghost") is visible. For each view: open, drag it off its
edge with the held relative pointer, capture 25 ms full-output frames through
release and settle, list compositor windows and layer surfaces, then dock it back
and capture again. Frames are proxies (screencopy), not scanout.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time

from PIL import Image, ImageChops

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__  # capture-safety re-execs must re-enter this wrapper
m.STATES['detach-ghost'] = ([], [], 0)
# The owner's session: default animations, plus the shipped rounded-window rule.
m.NIRI_CONFIG = m.NIRI_CONFIG.replace('animations {\n    off\n}\n', 'animations {\n}\n') + '''window-rule {
    exclude app-id="^org[.]quickshell$" title="^Nacre Pearl "
    geometry-corner-radius 10
    clip-to-geometry true
}
'''
VIEWS = os.environ.get('DETACH_VIEWS', 'launcher,dashboard,control').split(',')
OPEN = {
    'launcher': [('nacre', 'launcher', '')],
    'dashboard': [('drawers', 'toggle', 'dashboard'), ('nacre', 'tab', '0')],
    'control': [('nacre', 'controls', 'home')],
}
REQUEST = re.compile(r'->\s*(\w+)[@#](\d+)\.(\w+)\((.*)\)\s*$')


def mapped_surfaces(trace):
    """Replay WAYLAND_DEBUG=client requests: {wl_surface id: role} whose last commit had a buffer."""
    pending, mapped, roles = {}, {}, {}
    for line in trace.splitlines():
        match = REQUEST.search(line)
        if not match:
            continue
        interface, number, request, args = match.groups()
        refs = re.findall(r'(\w+)[@#](\d+)', args)
        if interface == 'wl_compositor' and request == 'create_surface':
            surface = refs[0][1]
            pending.pop(surface, None), mapped.pop(surface, None), roles.pop(surface, None)
        elif interface == 'wl_surface' and request == 'attach':
            pending[number] = not args.lstrip().startswith('nil')
        elif interface == 'wl_surface' and request == 'commit' and number in pending:
            mapped[number] = pending.pop(number)
        elif interface == 'wl_surface' and request == 'destroy':
            pending.pop(number, None), mapped.pop(number, None), roles.pop(number, None)
        elif request == 'get_xdg_surface':
            roles[refs[1][1]] = 'xdg'
        elif request == 'get_layer_surface':
            namespace = re.search(r'"([^"]*)"', args)
            roles[refs[1][1]] = 'layer:' + (namespace.group(1) if namespace else '')
        elif request == 'get_subsurface':
            roles[refs[1][1]] = 'subsurface'
    return {surface: roles.get(surface, 'unknown') for surface, buffer in mapped.items() if buffer}


def surface_delta(before, after):
    """Role surfaces mapped/unmapped between two replays; subsurfaces follow their parent (bar churn)."""
    before = {s: r for s, r in before.items() if r != 'subsurface'}
    after = {s: r for s, r in after.items() if r != 'subsurface'}
    return {'added': sorted(f'{s}:{r}' for s, r in after.items() if before.get(s) != r),
            'removed': sorted(f'{s}:{r}' for s, r in before.items() if after.get(s) != r)}


INTERVAL = float(os.environ.get('DETACH_INTERVAL', '0.025'))


def panel_call(self, target, method, args=()):
    reply = self.socket_call(self.runtime / 'bead-command.sock', target, method, list(args))
    if reply.get('error'):
        raise RuntimeError(f'{target} {method}: {reply["error"]}')
    result = reply['result']
    return json.loads(result) if isinstance(result, str) else result


def compositor_state(self):
    windows = json.loads(self.run(['niri', 'msg', '-j', 'windows']).stdout)
    layers = json.loads(self.run(['niri', 'msg', '-j', 'layers']).stdout)
    return {'windows': [{k: w.get(k) for k in ('id', 'title', 'app_id', 'is_floating', 'layout')}
                        for w in windows],
            'layers': [{k: layer.get(k) for k in ('namespace', 'output', 'layer')} for layer in layers]}


def frames(self, directory, trigger, seconds):
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    jobs = []
    start = time.monotonic()
    runner = threading.Thread(target=trigger)
    runner.start()
    index = 0
    while runner.is_alive() or time.monotonic() - start < seconds:
        before = time.monotonic()
        jobs.append((subprocess.Popen(['grim', '-s', '0.5', str(directory / f'{index:03}.ppm')], env=self.env,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL), before))
        index += 1
        time.sleep(max(0.0, before + INTERVAL - time.monotonic()))
    runner.join()
    for process, _ in jobs:
        process.wait(timeout=30)
    stamps = []
    for number, (_, before) in enumerate(jobs):
        path = directory / f'{number:03}.ppm'
        if path.exists():
            with Image.open(path) as image:
                image.convert('RGB').save(directory / f'{number:03}.png', optimize=False, compress_level=1)
            path.unlink()
            stamps.append(round((before - start) * 1000))
    (directory / 'stamps.json').write_text(json.dumps(stamps))
    return stamps


def stray_pixels(frame_path, reference_path, keep):
    """Count pixels differing from the settled reference outside `keep` (logical rects, half scale frames)."""
    with Image.open(frame_path) as frame, Image.open(reference_path) as reference:
        diff = ImageChops.difference(frame.convert('RGB'), reference.convert('RGB')).convert('L')
    mask = diff.point(lambda p: 255 if p > 24 else 0)
    for x, y, w, h in keep:
        # Frames are captured at grim -s 0.5 of logical size; dilate for motion blur and shadow.
        mask.paste(0, (max(0, round((x - 40) * 0.5)), max(0, round((y - 40) * 0.5)),
                       round((x + w + 40) * 0.5), round((y + h + 40) * 0.5)))
    bbox = mask.getbbox()
    return (sum(1 for p in mask.getdata() if p), [round(v * 2) for v in bbox] if bbox else None)


def trace_text(self):
    path = self.logs / 'shell.log'
    return path.read_text(errors='replace') if path.exists() else ''


def drive(self, _names):
    out = self.out / 'detach-ghost'
    out.mkdir(parents=True, exist_ok=True)
    report = {'method': 'full-output grim -s 0.5 frames every 25 ms, niri default animations, owner rounded-window rule',
              'renderer': 'cpu' if self.args.cpu else 'femtovg-gl', 'views': []}
    logical_w, logical_h = self.geometry['logical']
    scale = self.geometry['scale']
    try:
        for view in VIEWS:
            row = {'view': view}
            report['views'].append(row)
            time.sleep(0.5)
            closed_surfaces = mapped_surfaces(trace_text(self))
            directory = out / view
            directory.mkdir(parents=True, exist_ok=True)
            for step in OPEN[view]:
                self.dispatch(*step)
            time.sleep(2.0)
            state = panel_call(self, 'bead.panel', 'state')
            row['docked_state'] = {k: state.get(k) for k in ('view', 'origin', 'content_frame', 'edge', 'size')}
            origin = state.get('origin') or [0, 0]
            regions = panel_call(self, 'bead', 'panel-regions')
            offset = regions.get('offset') or [0, 0]
            cx, cy, body_w, body_h = state['content_frame']
            body_x, body_y = origin[0] + offset[0] + cx, origin[1] + offset[1] + cy
            usable = [r['rect'] for r in regions.get('regions', []) if r['rect'][2] >= 24 and r['rect'][3] >= 20]
            rx, ry, rw, rh = max(usable, key=lambda r: r[2] * r[3])
            start_logical = [origin[0] + offset[0] + rx + rw / 2, origin[1] + offset[1] + ry + rh / 2]
            edge = str(state.get('edge', 'top')).lower()
            dy = -min(160, max(0, body_y)) if edge == 'bottom' else min(160, max(0, logical_h - body_y - body_h))
            dx = -min(120, max(0, body_x)) if edge == 'right' else min(120, max(0, logical_w - body_x - body_w))
            start = [round(v * scale) for v in start_logical]
            end = [round((start_logical[0] + dx) * scale), round((start_logical[1] + dy) * scale)]
            self.grab(directory / 'docked.png')
            row['docked_compositor'] = compositor_state(self)
            self.release_pointer()

            def detach():
                proc = self.run([self.args.pointer_tool, *map(str, start), *map(str, self.args.size),
                                 'relative-drag', *map(str, end), '650', '1.5'], timeout=15, check=False)
                row['pointer'] = {'rc': proc.returncode, 'stdout': proc.stdout[-400:], 'stderr': proc.stderr[-400:],
                                  'start_px': start, 'end_px': end}

            row['detach_stamps_ms'] = frames(self, directory / 'detach', detach, 3.0)
            time.sleep(1.0)
            self.grab(directory / 'detached.png')
            pearls = panel_call(self, 'bead.panel', 'pearls')
            row['pearls'] = pearls
            row['detached_compositor'] = compositor_state(self)
            detached_surfaces = mapped_surfaces(trace_text(self))
            row['detached_surface_delta'] = surface_delta(closed_surfaces, detached_surfaces)
            added = row['detached_surface_delta']['added']
            # The only surface mapped beyond the closed shell is the new pearl window.
            row['only_new_window_mapped'] = (len(added) == 1 and added[0].endswith(':xdg')
                                             and not row['detached_surface_delta']['removed'])
            mine = [p for p in pearls if p.get('view', '').split('-')[0] == view.split('-')[0]
                    or p.get('view') == state.get('view')]
            if not mine:
                row['failure'] = 'release did not detach a pearl'
                self.dispatch('nacre', 'close')
                time.sleep(0.8)
                continue
            pearl = mine[-1]
            keep = [[body_x, body_y, body_w, body_h], pearl['rect']]
            # Settled reference: the detached screen at half scale.
            with Image.open(directory / 'detached.png') as full:
                full.convert('RGB').resize((full.width // 3, full.height // 3)).save(directory / 'detached-half.png')
            rows = []
            for path, stamp in zip(sorted((directory / 'detach').glob('[0-9][0-9][0-9].png')),
                                   row['detach_stamps_ms']):
                count, bbox = stray_pixels(path, directory / 'detached-half.png', keep)
                rows.append({'frame': path.name, 'stamp_ms': stamp, 'stray_px': count, 'stray_bbox_logical': bbox})
            row['detach_frames'] = rows
            release = re.search(r'POINTER_RELEASE monotonic_ns=(\d+)', row['pointer']['stdout'])
            row['detach_max_stray_px'] = max((r['stray_px'] for r in rows), default=0)

            def dock():
                panel_call(self, 'bead.panel', 'dock', [pearl['id']])
                time.sleep(1.5)

            row['dock_stamps_ms'] = frames(self, directory / 'dock', dock, 2.0)
            time.sleep(0.8)
            self.grab(directory / 'docked-again.png')
            row['docked_again_compositor'] = compositor_state(self)
            row['pearls_after_dock'] = panel_call(self, 'bead.panel', 'pearls')
            row['docked_again_surface_delta'] = surface_delta(closed_surfaces, mapped_surfaces(trace_text(self)))
            self.dispatch('nacre', 'close')
            time.sleep(1.2)
            self.grab(directory / 'closed.png')
            row['closed_compositor'] = compositor_state(self)
            row['closed_surface_delta'] = surface_delta(closed_surfaces, mapped_surfaces(trace_text(self)))
            row['clean_after_close'] = not any(row['closed_surface_delta'].values())
            row['release_seen'] = bool(release)
    finally:
        try:
            panel_call(self, 'bead.panel', 'pearls-reset')
            self.dispatch('nacre', 'close')
        except Exception:
            pass
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    bad = [row['view'] for row in report['views']
           if row.get('failure') or not row.get('only_new_window_mapped') or not row.get('clean_after_close')]
    if bad and os.environ.get('DETACH_STRICT', '1') == '1':
        raise RuntimeError('detach left extra mapped surfaces or failed: ' + ','.join(bad))


if __name__ == '__main__':
    _spawn = m.Arm.spawn

    def spawn(self, command, name, env=None, cwd=None):
        if name == 'shell':
            env = dict(env or self.env, WAYLAND_DEBUG='client', BEAD_VERBOSE_LOG='1', BEAD_PRIVATE_INPUT_PROOF='1')
            command += ['--trace-state']
            if not self.args.cpu:
                command += ['--panel-renderer', 'femtovg-gl']
        return _spawn(self, command, name, env, cwd)

    m.Arm.spawn = spawn
    m.Arm.drive = drive
    m.build_gallery = lambda *args: {}
    sys.exit(m.main())
