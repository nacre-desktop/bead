#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Private nested-compositor release handoff proof, wrapping parity capture safety.

Run with parity-compare.py's required --nacre/--bead-bin/--bead-shell/--out,
plus --arms bead --pointer-tool PATH --parent-output-tool wlr-randr. Captures
are full-output PNG proxies launched every 25 ms. Protocol timestamps mark
screencopy readiness, not physical scanout.
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
m.STATES['release-proof'] = ([], [], 0)
TRIALS = int(os.environ.get('PEARL_TRIALS', '20'))


def spawn(self, command, name, env=None, cwd=None):
    if name == 'shell':
        env = dict(env or self.env, WAYLAND_DEBUG='client', BEAD_VERBOSE_LOG='1',
                   BEAD_FRAME_TRACE='1', BEAD_PRIVATE_INPUT_PROOF='1')
        command += ['--trace-state']
    return _spawn(self, command, name, env, cwd)


_spawn = m.Arm.spawn


def panel_call(self, method):
    target = 'bead' if method == 'panel-regions' else 'bead.panel'
    reply = self.socket_call(self.runtime / 'bead-command.sock', target, method, [])['result']
    return json.loads(reply) if isinstance(reply, str) else reply


def capture_sequence(self, directory, trigger, seconds=2.0):
    """Full output every 25 ms; record each screencopy protocol-ready timestamp."""
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    jobs, errors, marks = [], [], {}
    marks['capture_start_monotonic_ns'] = time.monotonic_ns()
    started = marks['capture_start_monotonic_ns'] / 1_000_000_000
    protocol_dir = directory / 'protocol'
    protocol_dir.mkdir()

    def run_trigger():
        try:
            trigger(marks)
        except BaseException as error:
            errors.append(error)

    runner = threading.Thread(target=run_trigger)
    runner.start()
    index = 0
    interval = 0.025
    while time.monotonic() - started < seconds:
        stamp = time.monotonic()
        ppm = directory / f'{index:03}.ppm'
        protocol_path = protocol_dir / f'{index:03}.log'
        protocol_file = protocol_path.open('w')
        process = subprocess.Popen(['grim', str(ppm)], env=dict(self.env, WAYLAND_DEBUG='client'),
                                   stdout=subprocess.DEVNULL, stderr=protocol_file)
        jobs.append((process, stamp, protocol_file, protocol_path))
        index += 1
        time.sleep(max(0.0, stamp + interval - time.monotonic()))
    runner.join()
    for process, _, protocol_file, _ in jobs:
        try:
            process.wait(timeout=30)
        finally:
            protocol_file.close()
    stamps = []
    spawn_stamps = []
    for number, (process, stamp, _, protocol_path) in enumerate(jobs):
        if process.returncode:
            raise RuntimeError(f'screencopy process {number:03} failed: {process.returncode}')
        capture_ns = parse_capture_ns(protocol_path.read_text(errors='replace'))
        ppm = directory / f'{number:03}.ppm'
        with Image.open(ppm) as image:
            image.convert('RGB').save(directory / f'{number:03}.png')
        ppm.unlink()
        stamps.append((capture_ns - marks['capture_start_monotonic_ns']) / 1_000_000)
        spawn_stamps.append(round((stamp - started) * 1000))
    (directory / 'stamps.json').write_text(json.dumps(stamps, indent=2) + '\n')
    (directory / 'spawn-stamps.json').write_text(json.dumps(spawn_stamps, indent=2) + '\n')
    if errors:
        raise RuntimeError(f'release trigger failed: {errors[0]}')
    return marks, stamps


def parse_capture_ns(protocol_log):
    """Return absolute screencopy-ready/presented monotonic ns from WAYLAND_DEBUG."""
    patterns = (
        r'zwlr_screencopy_frame_v1[@#]\d+\.ready\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)',
        r'ext_image_copy_capture_frame_v1[@#]\d+\.presentation_time\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)',
    )
    for pattern in patterns:
        match = re.search(pattern, protocol_log)
        if match:
            high, low, nsec = map(int, match.groups())
            return (((high << 32) | low) * 1_000_000_000) + nsec
    raise RuntimeError('screencopy protocol log has no ready/presentation timestamp')


def dominant_fill(image_path, rect, scale):
    image = Image.open(image_path).convert('RGB')
    x, y, w, h = rect
    inset = 12
    x0, y0 = round((x + inset) * scale), round((y + inset) * scale)
    x1, y1 = round((x + w - inset) * scale), round((y + h - inset) * scale)
    interior = image.crop((x0, y0, x1, y1))
    colors = interior.getcolors(maxcolors=interior.width * interior.height)
    if not colors:
        return None
    return max(colors, key=lambda row: row[0])[1]


def locate_body(image_path, rect, scale, fill):
    """Find the fixed final-screen body fill within the output interior."""
    image = Image.open(image_path).convert('RGB')
    x, y, w, h = rect
    # A tight search omits unrelated frame chrome. A misplaced destination is
    # clipped differently here and fails the final-body bbox check.
    margin = 24
    sx0 = max(round(16 * scale), round((x - margin) * scale))
    sy0 = max(round(48 * scale), round((y - margin) * scale))  # omit the top bar
    sx1 = min(image.width - round(16 * scale), round((x + w + margin) * scale))
    sy1 = min(image.height - round(16 * scale), round((y + h + margin) * scale))
    search = image.crop((sx0, sy0, sx1, sy1))
    difference = ImageChops.difference(search, Image.new('RGB', search.size, fill))
    mask = difference.convert('L').point(lambda pixel: 255 if pixel == 0 else 0)
    bbox = mask.getbbox()
    if not bbox:
        return fill, None
    return fill, [sx0 + bbox[0], sy0 + bbox[1], sx0 + bbox[2], sy0 + bbox[3]]


def analyze(directory, final_path, stamps, rect, scale, post_release_start_ms, delta):
    fill = dominant_fill(final_path, rect, scale)
    if fill is None:
        raise RuntimeError('final screenshot has no measurable pearl body fill')
    paths = sorted(directory.glob('[0-9][0-9][0-9].png'))
    frames = []
    for path, stamp in zip(paths, stamps):
        _, bbox = locate_body(path, rect, scale, fill)
        frames.append({'stamp_ms': stamp, 'bbox_px': bbox,
                       'post_release_proxy': stamp >= post_release_start_ms})
    post = [frame for frame in frames if frame['post_release_proxy']]
    if len(post) < 8:
        raise RuntimeError(f'only {len(post)} post-release proxy frames; at least 8 required')
    visible = [f for f in post if f['bbox_px']]
    blank = len(post) - len(visible)
    centers = [((f['bbox_px'][0] + f['bbox_px'][2]) / 2,
                (f['bbox_px'][1] + f['bbox_px'][3]) / 2) for f in visible]
    dx, dy = delta[0] * scale, delta[1] * scale
    length = max(1.0, (dx * dx + dy * dy) ** 0.5)
    projections = [(x * dx + y * dy) / length for x, y in centers]
    reversals = sum(b < a - 2 for a, b in zip(projections, projections[1:]))
    reference_bbox = locate_body(final_path, rect, scale, fill)[1]
    if not reference_bbox:
        raise RuntimeError('fixed dominant RGB did not produce a final-body bbox')
    reference_size = [reference_bbox[2] - reference_bbox[0], reference_bbox[3] - reference_bbox[1]]
    moving = any(not f['bbox_px'] or
                 max(abs(f['bbox_px'][i] - reference_bbox[i]) for i in range(4)) > 2
                 for f in visible)
    unstable_size = any(not f['bbox_px'] or
                        max(abs((f['bbox_px'][2] - f['bbox_px'][0]) - reference_size[0]),
                            abs((f['bbox_px'][3] - f['bbox_px'][1]) - reference_size[1])) > 2
                        for f in visible)
    return {'fill_rgb': fill, 'frames': frames, 'post_release_frame_count': len(post),
            'post_release_blank_frames': blank, 'post_release_reversals': reversals,
            'post_release_moving': moving, 'final_body_bbox_px': reference_bbox,
            'final_body_stable': blank == 0 and not unstable_size and not moving,
            'post_release_positions_px': centers,
            'post_release_sizes_px': [
                [f['bbox_px'][2] - f['bbox_px'][0], f['bbox_px'][3] - f['bbox_px'][1]]
                for f in visible]}


def release_proxy_ms(pointer_stdout, capture_start_ns):
    match = re.search(r'POINTER_RELEASE monotonic_ns=(\d+)', pointer_stdout or '')
    if not match:
        raise RuntimeError('pointer tool emitted no POINTER_RELEASE timestamp')
    return (int(match.group(1)) - capture_start_ns) / 1_000_000


def drive(self, _names):
    if self.name != 'bead':
        raise RuntimeError('pearl-release-proof requires --arms bead')
    if not self.args.pointer_tool:
        raise RuntimeError('--pointer-tool is required')
    if tuple(self.args.size) != (2880, 1800) or self.geometry['scale'] != 1.5:
        raise RuntimeError('release proof requires 2880x1800 at scale 1.5')
    if TRIALS < 20:
        raise RuntimeError(f'PEARL_TRIALS={TRIALS} is fewer than the required 20 trials')
    out = self.out / 'release-proof'
    out.mkdir(parents=True, exist_ok=True)
    report = {'method': '25ms full-output screencopy-ready PNG proxy; not physical scanout',
              'renderer': 'cpu', 'idle_memory_kib': self.rss(),
              'scale': 1.5, 'size': [2880, 1800], 'trials': [], 'required_trials': 20}
    shell_log = self.logs / 'shell.log'
    logical_w, logical_h = self.geometry['logical']
    try:
        for trial in range(TRIALS):
            self.dispatch('nacre', 'launcher', '')
            time.sleep(2)
            state = panel_call(self, 'state')
            docked_memory = self.rss()
            regions = panel_call(self, 'panel-regions')
            rows = [row for row in regions.get('regions', [])
                    if row.get('rect', [0, 0, 0, 0])[2] >= 24 and row.get('rect', [0, 0, 0, 0])[3] >= 20]
            if not rows:
                raise RuntimeError(f'trial {trial + 1}: launcher has no usable panel input region')
            row = max(rows, key=lambda item: item['rect'][2] * item['rect'][3])
            origin = state.get('origin') or [0, 0]
            offset = regions.get('offset') or [0, 0]
            rx, ry, rw, rh = row['rect']
            start_logical = [origin[0] + offset[0] + rx + rw / 2,
                             origin[1] + offset[1] + ry + rh / 2]
            content = state.get('content_frame')
            if content and len(content) == 4:
                cx, cy, body_w, body_h = content
                body_x, body_y = origin[0] + offset[0] + cx, origin[1] + offset[1] + cy
            else:
                extents = [r['rect'] for r in rows]
                bx0, by0 = min(r[0] for r in extents), min(r[1] for r in extents)
                bx1, by1 = max(r[0] + r[2] for r in extents), max(r[1] + r[3] for r in extents)
                body_x, body_y = origin[0] + offset[0] + bx0, origin[1] + offset[1] + by0
                body_w, body_h = bx1 - bx0, by1 - by0
            edge = str(state.get('edge', 'top')).lower()
            if edge == 'bottom':
                dy = -min(160, max(0, body_y))
            else:
                dy = min(160, max(0, logical_h - body_y - body_h))
            if not dy:
                dy = min(160, max(0, logical_h - body_y - body_h)) if edge == 'bottom' else -min(160, max(0, body_y))
            if edge == 'right':
                dx = -min(120, max(0, body_x))
            else:
                dx = min(120, max(0, logical_w - body_x - body_w))
            if not dx:
                dx = min(120, max(0, logical_w - body_x - body_w)) if edge == 'right' else -min(120, max(0, body_x))
            if not dx and not dy:
                raise RuntimeError(f'trial {trial + 1}: no in-bounds drag away from panel edge')
            start = [round(v * 1.5) for v in start_logical]
            end = [round((start_logical[0] + dx) * 1.5), round((start_logical[1] + dy) * 1.5)]
            log_mark = shell_log.stat().st_size if shell_log.exists() else 0
            self.release_pointer()

            def trigger(marks):
                marks['pointer_start_monotonic_ns'] = time.monotonic_ns()
                marks['pointer_start_ms'] = round(
                    (marks['pointer_start_monotonic_ns'] - marks['capture_start_monotonic_ns']) / 1_000_000)
                proc = self.run([self.args.pointer_tool, *map(str, start), '2880', '1800',
                                 'relative-drag', *map(str, end), '650', '1.5'],
                                timeout=15, env=self.env)
                marks['pointer_stdout'] = proc.stdout
                marks['handoff_memory_kib'] = self.rss()
                time.sleep(1)

            directory = out / f'trial-{trial + 1:02d}' / 'sequence'
            marks, stamps = capture_sequence(self, directory, trigger)
            trace = shell_log.read_bytes()[log_mark:].decode(errors='replace') if shell_log.exists() else ''
            (directory.parent / 'native-frame-trace.log').write_text(
                '\n'.join(line for line in trace.splitlines()
                          if re.search(r'wl_surface[@#]\d+', line) or 'UI_FRAME' in line
                          or 'UI_PRESENTED' in line or 'PEARL_HANDOFF_' in line
                          or 'set_window_geometry' in line) + '\n')
            pearls = panel_call(self, 'pearls')
            detached = [p for p in pearls if p.get('view') == 'launcher']
            if not detached:
                self.grab(directory.parent / 'final-screen.png')
                report['trials'].append({'trial': trial + 1, 'detached': False,
                                         'failure': 'release did not detach a launcher pearl',
                                         'handoff_committed': False,
                                         'pointer_stdout': marks.get('pointer_stdout'),
                                         'sequence_stamps_ms': stamps})
                raise RuntimeError(f'trial {trial + 1}: release did not detach a launcher pearl')
            pearl = detached[-1]
            rect = pearl['rect']
            final_screen = directory.parent / 'final-screen.png'
            self.grab(final_screen)
            handoff = f"PEARL_HANDOFF_COMMITTED id={pearl['id']}" in trace
            if not handoff:
                report['trials'].append({'trial': trial + 1, 'detached': True,
                                         'handoff_committed': False, 'pearl_id': pearl['id'],
                                         'failure': 'PEARL_HANDOFF_COMMITTED marker absent'})
                raise RuntimeError(f"trial {trial + 1}: missing PEARL_HANDOFF_COMMITTED id={pearl['id']}")
            try:
                release_ms = release_proxy_ms(marks.get('pointer_stdout'),
                                              marks['capture_start_monotonic_ns'])
                analysis = analyze(directory, final_screen, stamps, rect, 1.5,
                                   release_ms, [dx, dy])
            except RuntimeError as error:
                report['trials'].append({'trial': trial + 1, 'detached': True,
                                         'handoff_committed': True, 'pearl_id': pearl['id'],
                                         'failure': str(error)})
                raise
            result = {'trial': trial + 1, 'detached': True, 'handoff_committed': True,
                      'initial_body_bounds_logical': [body_x, body_y, body_w, body_h],
                      'pointer_start_px': start, 'pointer_end_px': end, 'delta_logical': [dx, dy],
                      'pearl': pearl, 'pointer_stdout': marks['pointer_stdout'],
                      'docked_memory_kib': docked_memory,
                      'handoff_memory_kib': marks.get('handoff_memory_kib'),
                      'detached_memory_kib': self.rss(), **analysis}
            report['trials'].append(result)
            self.dispatch('pearls', 'close', str(pearl['id']))
            self.dispatch('nacre', 'close')
            time.sleep(0.5)
            result['closed_memory_kib'] = self.rss()
            if analysis['post_release_frame_count'] < 8:
                raise RuntimeError(f"trial {trial + 1}: only {analysis['post_release_frame_count']} "
                                   'post-release proxy frames; at least 8 required')
            if (analysis['post_release_blank_frames'] or analysis['post_release_reversals'] or
                    analysis['post_release_moving'] or not analysis['final_body_stable']):
                raise RuntimeError(f'trial {trial + 1}: post-release handoff failure: '
                                   f"blank={analysis['post_release_blank_frames']} "
                                   f"reversals={analysis['post_release_reversals']} "
                                   f"moving={analysis['post_release_moving']} stable={analysis['final_body_stable']}")
    finally:
        try:
            self.dispatch('pearls', 'close', 'all')
            self.dispatch('nacre', 'close')
        except Exception:
            pass
        (out / 'release-proof.json').write_text(json.dumps(report, indent=2) + '\n')
    if len(report['trials']) < 20:
        raise RuntimeError(f"only {len(report['trials'])} release trials completed; 20 required")
    (out / 'release-proof.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    m.Arm.spawn = spawn
    m.Arm.drive = drive
    m.build_gallery = lambda *args: {}
    sys.exit(m.main())
