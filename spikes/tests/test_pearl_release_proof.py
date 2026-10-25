import importlib.util
from pathlib import Path
import tempfile
import unittest

from PIL import Image, ImageDraw

SCRIPT = Path(__file__).resolve().parents[1] / 'pearl-release-proof.py'
spec = importlib.util.spec_from_file_location('pearl_release_proof', SCRIPT)
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


class PearlReleaseClassifierTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.rect = [40, 60, 60, 40]
        self.color = (201, 211, 221)
        self.final = self.root / 'final.png'
        self.draw_frame(self.final, 40)
        self.frames = self.root / 'sequence'
        self.frames.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def draw_frame(self, path, x=None):
        image = Image.new('RGB', (160, 140), (12, 14, 18))
        if x is not None:
            ImageDraw.Draw(image).rectangle((x, 60, x + 59, 99), fill=self.color)
        image.save(path)

    def analyze(self, stamps):
        return proof.analyze(self.frames, self.final, stamps, self.rect, 1.0, 850, [0, 0])

    def write_frames(self, xs):
        for index, x in enumerate(xs):
            self.draw_frame(self.frames / f'{index:03}.png', x)

    def test_blank_frame_cannot_choose_background_as_fill(self):
        self.write_frames([None] * 8)
        result = self.analyze([900 + 25 * i for i in range(8)])
        self.assertEqual(result['fill_rgb'], self.color)
        self.assertEqual(result['post_release_blank_frames'], 8)
        self.assertFalse(result['final_body_stable'])

    def test_body_motion_after_release_is_detected(self):
        self.write_frames([40, 40, 44, 44, 44, 44, 44, 44])
        result = self.analyze([900 + 25 * i for i in range(8)])
        self.assertTrue(result['post_release_moving'])
        self.assertFalse(result['final_body_stable'])

    def test_identical_covered_and_native_body_bbox_is_stable(self):
        self.write_frames([40] * 8)
        result = self.analyze([900 + 25 * i for i in range(8)])
        self.assertFalse(result['post_release_blank_frames'])
        self.assertFalse(result['post_release_moving'])
        self.assertTrue(result['final_body_stable'])

    def test_fewer_than_eight_post_release_frames_is_rejected(self):
        self.write_frames([40] * 8)
        with self.assertRaisesRegex(RuntimeError, 'at least 8'):
            self.analyze([700 + 25 * i for i in range(8)])


class PearlReleaseTimestampTests(unittest.TestCase):
    def test_screencopy_ready_timestamp_combines_high_low_seconds_and_nsec(self):
        text = ' -> zwlr_screencopy_frame_v1#27.ready(1, 2, 300)'
        self.assertEqual(proof.parse_capture_ns(text), ((1 << 32) + 2) * 1_000_000_000 + 300)

    def test_ext_image_presentation_timestamp_is_supported(self):
        text = '<- ext_image_copy_capture_frame_v1@14.presentation_time(0, 7, 900)'
        self.assertEqual(proof.parse_capture_ns(text), 7_000_000_900)

    def test_missing_screencopy_timestamp_fails(self):
        with self.assertRaisesRegex(RuntimeError, 'no ready/presentation timestamp'):
            proof.parse_capture_ns('wl_display.delete_id(12)')

    def test_release_proxy_uses_exact_pointer_release_time(self):
        output = 'POINTER_SENT monotonic_ns=9010000000 x=1 y=2\nPOINTER_RELEASE monotonic_ns=9123456789\n'
        self.assertAlmostEqual(proof.release_proxy_ms(output, 9_000_000_000), 123.456789)

    def test_missing_pointer_release_time_fails(self):
        with self.assertRaisesRegex(RuntimeError, 'no POINTER_RELEASE timestamp'):
            proof.release_proxy_ms('POINTER_SENT monotonic_ns=1 x=2 y=3', 0)


if __name__ == '__main__':
    unittest.main()
