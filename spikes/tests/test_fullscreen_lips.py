# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the capture oracle with a fake compositor's fullscreen toggle.

The real layer placement and pointer/keyboard routing are exercised by the
guarded nested niri proof; this test rejects the original Overlay regression.
"""
import importlib.util
from pathlib import Path
import unittest

from PIL import Image, ImageDraw

spec = importlib.util.spec_from_file_location('lips', Path(__file__).resolve().parents[1] / 'fullscreen-lips.py')
lips = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lips)


class Compositor:
    def __init__(self, layer):
        self.layer = layer
        self.fullscreen = False

    def toggle(self):
        self.fullscreen = not self.fullscreen

    def capture(self):
        image = Image.new('RGB', (800, 500), lips.BACKGROUND)
        if not self.fullscreen or self.layer == 'Overlay':
            painter = ImageDraw.Draw(image)
            painter.rectangle((0, 0, 799, 10), fill='black')
            painter.polygon([(2, 245), (8, 250), (2, 255)], fill='white')
            painter.polygon([(798, 245), (792, 250), (798, 255)], fill='white')
        return image


class FullscreenLips(unittest.TestCase):
    def test_top_lips_covered_and_restored(self):
        compositor = Compositor('Top')
        before = compositor.capture()
        with self.assertRaises(AssertionError):
            lips.check_cover(before)
        compositor.toggle()
        lips.check_cover(compositor.capture())
        compositor.toggle()
        self.assertEqual(before.tobytes(), compositor.capture().tobytes())

    def test_overlay_regression_is_rejected(self):
        compositor = Compositor('Overlay')
        compositor.toggle()
        with self.assertRaisesRegex(AssertionError, 'non-client pixels'):
            lips.check_cover(compositor.capture())

    def test_each_edge_is_checked_independently(self):
        for point in [(3, 250), (795, 250), (400, 3)]:
            image = Image.new('RGB', (800, 500), lips.BACKGROUND)
            image.putpixel(point, (255, 255, 255))
            with self.subTest(point=point), self.assertRaises(AssertionError):
                lips.check_cover(image)


if __name__ == '__main__':
    unittest.main()
