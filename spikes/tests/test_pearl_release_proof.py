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


if __name__ == '__main__':
    unittest.main()
