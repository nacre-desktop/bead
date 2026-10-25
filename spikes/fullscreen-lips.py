#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0125 fullscreen/edge input proof using parity-compare's guarded private arms.

Use parity-compare.py's arguments, --states idle --arms qml,bead, and the
private pointer tool. Run under the desktop capture lock, never on the laptop.
"""
import importlib.util
import json
import sys
import time
from pathlib import Path

from PIL import Image, ImageChops

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('parity', HERE / 'parity-compare.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.__file__ = __file__

BACKGROUND = (0x23, 0x45, 0x67)
CLIENT = '''import os,sys,tty
tty.setraw(sys.stdin.fileno())
sys.stdout.write("\\033[?1000h\\033[?1006h")
sys.stdout.flush()
with open(sys.argv[1], "ab", buffering=0) as sink:
 while True:
  sink.write(os.read(sys.stdin.fileno(), 4096))
'''


def check_cover(image):
    """The opaque fullscreen client must own the frame and both lip positions.

    Sample entire edge bands away from the terminal's cursor/text and pointer,
    rather than checking a single pixel that could miss the chevron.
    """
    image = image.convert('RGB')
    w, h = image.size
    bands = [(0, h // 3, 15, 2 * h // 3), (w - 15, h // 3, w, 2 * h // 3),
             (w // 3, 0, 2 * w // 3, 15)]
    for box in bands:
        crop = image.crop(box)
        area = crop.width * crop.height
        wrong = sum(count for count, pixel in crop.getcolors(area) if pixel != BACKGROUND)
        if wrong:
            raise AssertionError(f'fullscreen edge {box}: {wrong} non-client pixels')


def windows(arm):
    rows = json.loads(arm.run(['niri', 'msg', '-j', 'windows']).stdout)
    return rows.get('windows', []) if isinstance(rows, dict) else rows


def drive(self, _names):
    if not self.args.pointer_tool:
        raise RuntimeError('fullscreen proof needs --pointer-tool')
    self.errors = []
    script = self.root / 'fullscreen-client.py'
    sink = self.out / 'client-input.raw'
    script.write_text(CLIENT)
    self.spawn(['alacritty', '-o', 'colors.primary.background="#234567"',
                '--class', 'fullscreen-proof', '-e', sys.executable, str(script), str(sink)], 'fullscreen-client')
    m.wait_until(lambda: any(w.get('app_id') == 'fullscreen-proof' for w in windows(self)), 20, 'fullscreen client')
    window = next(w for w in windows(self) if w.get('app_id') == 'fullscreen-proof')
    ident = str(window['id'])
    width, height = self.geometry['logical']
    shots = self.out / 'shots'
    shots.mkdir(exist_ok=True)
    result = {'renderer': 'qml-default' if self.name == 'qml' else 'cpu', 'checks': {}}

    def shot(name):
        path = shots / (name + '.png')
        self.run(['grim', str(path)])
        return Image.open(path).convert('RGB')

    def action():
        self.run(['niri', 'msg', 'action', 'fullscreen-window', '--id', ident])
        time.sleep(.8)

    def hover(name):
        before = shot(name + '-before')
        self.pointer(width - 3, height // 2, settle=1.2)
        opened = shot(name)
        changed = ImageChops.difference(before, opened).convert('L').point(lambda v: 255 if v > 12 else 0)
        assert changed.histogram()[255] > 1000, 'hover did not reveal a panel'
        self.pointer(width // 2, height // 2)
        self.steps([('ui', 'nacre', 'close'), ('sleep', .5)])
        return opened

    self.pointer(width // 2, height // 2)
    idle = shot('idle')
    hover('hover')
    result['checks']['normal_hover'] = True
    self.pointer(width - 3, height // 2, mode='press', settle=.1)
    shot('pressed')
    self.release_pointer()
    time.sleep(.8)
    clicked = shot('click-open')
    assert ImageChops.difference(idle, clicked).getbbox(), 'click did not reveal a panel'
    result['checks']['normal_click'] = True
    self.pointer(width // 2, height // 2)
    self.steps([('ui', 'nacre', 'close'), ('sleep', .5)])
    layers = json.loads(self.run(['niri', 'msg', '-j', 'layers']).stdout)
    result['layers'] = layers
    if self.name == 'bead':
        edges = [row for row in layers if row.get('namespace', '').endswith('-edge')]
        assert edges and all(row['layer'] == 'Top' and row['keyboard_interactivity'] == 'None' for row in edges), edges
        result['checks']['edges_top_keyboard_none'] = True
    action()
    check_cover(shot('fullscreen'))
    result['checks']['fullscreen_pixels_clear'] = True
    for x, token in [(2, 'LEFT'), (width - 3, 'RIGHT'), (width // 2, 'TOP')]:
        y = 2 if token == 'TOP' else height // 2
        self.pointer(x, y, settle=1.2)
        # A covered edge must neither dwell-open a panel nor take a click/key.
        check_cover(shot('fullscreen-' + token.lower()))
        before = sink.stat().st_size if sink.exists() else 0
        self.pointer(x, y, mode='click')
        self.run(['wtype', token])
        m.wait_until(lambda: sink.exists() and token.encode() in sink.read_bytes()[before:]
                     and b'\x1b[<' in sink.read_bytes()[before:], 5, 'client received mouse and ' + token)
    result['checks']['fullscreen_edges_deliver_input'] = True
    result['fullscreen_windows'] = windows(self)
    self.pointer(width // 2, height // 2)
    self.notify('Fullscreen proof', 'Notification cards follow the chrome.')
    time.sleep(.5)
    notification = shot('fullscreen-notification')
    result['notification_layers'] = json.loads(self.run(['niri', 'msg', '-j', 'layers']).stdout)
    if self.name == 'bead':
        # The card must actually exist below fullscreen; an absent producer
        # would otherwise make a clear screenshot a false pass.
        count = lambda rows: sum(row['namespace'] == 'bead-dev' and row['layer'] == 'Top' for row in rows)
        assert count(result['notification_layers']) > count(layers), 'notification surface was not created on Top'
    w, h = notification.size
    card_area = notification.crop((2 * w // 3, 40, w - 20, h // 3))
    assert not ImageChops.difference(card_area, Image.new('RGB', card_area.size, BACKGROUND)).getbbox(), 'notification showed over fullscreen'
    result['checks']['fullscreen_notification_covered'] = True
    if self.name == 'qml':
        self.run(['quickshell', 'ipc', '--path', str(self.qml_entry), 'call', 'nacre', 'previewFeedback', 'volume'])
    else:
        self.socket_call(self.runtime / 'bead-command.sock', 'bead.osd', 'preview', ['volume', .45, False, '', ''])
    time.sleep(.35)
    result['osd_layers'] = json.loads(self.run(['niri', 'msg', '-j', 'layers']).stdout)
    osd = shot('fullscreen-osd').crop((w // 3, 3 * h // 4, 2 * w // 3, h - 20))
    assert ImageChops.difference(osd, Image.new('RGB', osd.size, BACKGROUND)).getbbox(), 'explicit level OSD not visible over fullscreen'
    result['checks']['fullscreen_osd_visible'] = True
    time.sleep(3)
    action()
    restored = shot('restored')
    try:
        check_cover(restored)
    except AssertionError:
        result['checks']['chrome_restored'] = True
    else:
        raise AssertionError('chrome did not return after fullscreen')
    hover('restored-hover')
    result['checks']['restored_hover'] = True
    (self.out / 'fullscreen.json').write_text(json.dumps(result, indent=2) + '\n')
    (self.out / 'drive.json').write_text(json.dumps({'idle': {'errors': []}}))


m.Arm.drive = drive
if __name__ == '__main__':
    sys.exit(m.main())
