# SPDX-License-Identifier: GPL-3.0-only
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    'launcher_icons_proof', Path(__file__).resolve().parents[1] / 'launcher-icons-proof.py')
proof = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(proof)


class IconFrameScoreTests(unittest.TestCase):
    def test_empty_initial_scene_counts_until_populated_rasters_commit(self):
        trace = '''UI_IMAGE_BYTES scale=1.5 images=0 missing=0
UI_STATE {"launcher.pending":true,"launcher.result_count":0}
UI_COMMIT monotonic_ns=1000000 companion=true
UI_FRAME monotonic_ns=2000000 companion=true
UI_FRAME monotonic_ns=3000000 companion=true
UI_IMAGE_BYTES scale=1.5 images=24 missing=0
UI_STATE {"launcher.pending":false,"launcher.result_count":64}
UI_COMMIT monotonic_ns=4000000 companion=true
UI_FRAME monotonic_ns=5000000 companion=true
'''
        result = proof.score(trace, 0)
        self.assertEqual(result['missing_icon_frames'], 2)
        self.assertEqual(result['first_all_icons_ms'], 5)

    def test_warm_open_can_reuse_previously_committed_rasters(self):
        trace = '''UI_IMAGE_BYTES scale=1.5 images=24 missing=0
UI_STATE {"launcher.pending":false,"launcher.result_count":64}
UI_COMMIT monotonic_ns=1000000 companion=true
UI_FRAME monotonic_ns=2000000 companion=true
UI_FRAME monotonic_ns=5000000 companion=false
UI_FRAME monotonic_ns=6000000 companion=true
'''
        result = proof.score(trace, 4000000)
        self.assertEqual(result['frames'], 1)
        self.assertEqual(result['missing_icon_frames'], 0)
        self.assertEqual(result['first_all_icons_ms'], 2)

    def test_missing_raster_does_not_pass_with_populated_model(self):
        trace = '''UI_IMAGE_BYTES scale=1.5 images=24 missing=1
UI_STATE {"launcher.pending":false,"launcher.result_count":64}
UI_COMMIT monotonic_ns=1000000 companion=true
UI_FRAME monotonic_ns=2000000 companion=true
'''
        result = proof.score(trace, 0)
        self.assertEqual(result['missing_icon_frames'], 1)
        self.assertIsNone(result['first_all_icons_ms'])

    def test_presentation_uses_its_committed_update_not_the_newest_scene(self):
        trace = '''UI_IMAGE_BYTES scale=1.5 images=0 missing=0
UI_STATE {"launcher.pending":true,"launcher.result_count":0}
UI_COMMIT monotonic_ns=1000000 companion=true update=1
UI_IMAGE_BYTES scale=1.5 images=12 missing=0
UI_STATE {"launcher.pending":false,"launcher.result_count":12}
UI_COMMIT monotonic_ns=2000000 companion=true update=2
PANEL_PRESENTED monotonic_ns=3000000
UI_PRESENTED monotonic_ns=3000000 companion=true update=1
PANEL_PRESENTED monotonic_ns=3500000
PANEL_PRESENTED monotonic_ns=4000000
UI_PRESENTED monotonic_ns=4000000 companion=true update=2
'''
        result = proof.score(trace, 0)
        self.assertEqual(result['missing_icon_presentations'], 2)
        self.assertEqual(result['first_all_icons_presented_ms'], 4)


if __name__ == '__main__':
    unittest.main()
