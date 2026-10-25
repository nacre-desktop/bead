#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic log joins for proving a presented scroll direction reversal."""
import importlib.util
import os
import pathlib
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
PACING_ENV = {key: os.environ.get(key) for key in ("BEAD_VERBOSE_LOG", "BEAD_FRAME_TRACE")}
spec = importlib.util.spec_from_file_location("scroll_pacing", ROOT / "animation-pacing.py")
pacing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pacing)
for key, value in PACING_ENV.items():
    if value is None:
        os.environ.pop(key, None)
    else:
        os.environ[key] = value


class ScrollReversalProofTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp.name)
        self.inputs = self.root / "axis-input.log"
        self.inputs.write_text(
            "\n".join(
                [
                    "AXIS_SENT monotonic_ns=100000000 source=finger direction=down index=0 delta=7",
                    "AXIS_SENT monotonic_ns=200000000 source=finger direction=down index=1 delta=7",
                    "AXIS_SENT monotonic_ns=300000000 source=finger direction=up index=2 delta=-7",
                ]
            )
            + "\n"
        )

    def tearDown(self):
        self.temp.cleanup()

    def proof(self, lines, animation="scroll-notifications-popup-touchpad"):
        log = self.root / "scroll.log"
        log.write_text("\n".join(lines) + "\n")
        return pacing.reversal_proof([log], self.inputs, animation, 0, 1_000_000_000)

    @staticmethod
    def ui_lines(values, presented_companion="true", presented_generation=None):
        lines = []
        for index, (stamp, delivered_delta, value) in enumerate(
            zip((120_000_000, 220_000_000, 318_333_333), (7, 7, -7), values), start=1
        ):
            lines.extend(
                [
                    f"AXIS_DELIVER monotonic_ns={stamp - 8_333_333} horizontal=false x=4 y=8 delta={delivered_delta}",
                    f'{"UI_STATE"} {{"notifications.scroll.y":{value}}}',
                    f"UI_COMMIT generation=9 update={index} companion=true source_error=false",
                    "UI_PRESENTED generation={} monotonic_ns={} refresh_ns=8333333 companion={} update={} cpu_ns=0".format(
                        presented_generation if presented_generation is not None else 9,
                        stamp,
                        presented_companion,
                        index,
                    ),
                ]
            )
        return lines

    def test_opposite_input_and_presented_offset_change_prove_latency(self):
        proof = self.proof(self.ui_lines([0, 10, 9]))
        self.assertEqual(proof["status"], "proven")
        self.assertEqual(proof["delivery_to_visible_ms"], 8.33)
        self.assertLessEqual(proof["delivery_to_visible_ms"], 1000 / 120)

    def test_frame_in_original_direction_is_missing(self):
        proof = self.proof(self.ui_lines([0, 10, 11]))
        self.assertEqual(proof["status"], "missing")

    def test_presented_companion_must_match_committed_state(self):
        proof = self.proof(self.ui_lines([0, 10, 9], presented_companion="false"))
        self.assertEqual(proof["status"], "missing")

    def test_presented_generation_must_match_committed_state(self):
        proof = self.proof(self.ui_lines([0, 10, 9], presented_generation=8))
        self.assertEqual(proof["status"], "missing")

    def test_inverted_compositor_delta_sign_is_calibrated(self):
        lines = []
        for index, (stamp, delivered_delta, value) in enumerate(
            zip((120_000_000, 220_000_000, 318_333_333), (-7, -7, 7), (0, 10, 9)), start=1
        ):
            lines.extend(
                [
                    f"AXIS_DELIVER monotonic_ns={stamp - 8_333_333} horizontal=false x=4 y=8 delta={delivered_delta}",
                    f'UI_STATE {{"notifications.scroll.y":{value}}}',
                    f"UI_COMMIT generation=9 update={index} companion=true source_error=false",
                    f"UI_PRESENTED generation=9 monotonic_ns={stamp} refresh_ns=8333333 companion=true update={index} cpu_ns=0",
                ]
            )
        proof = self.proof(lines)
        self.assertEqual(proof["status"], "proven")

    def test_window_presentation_joins_epoch_update_and_generation(self):
        lines = []
        for index, (stamp, delivered_delta, value) in enumerate(
            zip((120_000_000, 220_000_000, 318_333_333), (7, 7, -7), (0, 10, 9)), start=1
        ):
            lines.extend(
                [
                    f"AXIS_DELIVER monotonic_ns={stamp - 10_000_000} horizontal=false x=4 y=8 delta={delivered_delta}",
                    f'WINDOW_STATE monotonic_ns={stamp - 8_000_000} epoch=3 update={index} generation=9 state={{"notifications.scroll.y":{value}}}',
                    f"WINDOW_COMMIT monotonic_ns={stamp - 4_000_000} epoch=3 update={index} generation=9 frame={index}",
                    f"WINDOW_PRESENTED monotonic_ns={stamp} refresh_ns=8333333 epoch=3 update={index} generation=9",
                ]
            )
        proof = self.proof(lines)
        self.assertEqual(proof["status"], "proven")

    def test_window_presentation_generation_mismatch_is_missing(self):
        log = []
        for index, (stamp, delta, value) in enumerate(
            zip((120_000_000, 220_000_000, 318_333_333), (7, 7, -7), (0, 10, 9)), start=1
        ):
            log.extend(
                [
                    f"AXIS_DELIVER monotonic_ns={stamp - 8_333_333} horizontal=false x=4 y=8 delta={delta}",
                    f'WINDOW_STATE monotonic_ns={stamp - 8_000_000} epoch=3 update={index} generation=9 state={{"notifications.scroll.y":{value}}}',
                    f"WINDOW_COMMIT monotonic_ns={stamp - 4_000_000} epoch=3 update={index} generation=9 frame={index}",
                    f"WINDOW_PRESENTED monotonic_ns={stamp} refresh_ns=8333333 epoch=3 update={index} generation=8",
                ]
            )
        proof = self.proof(log)
        self.assertEqual(proof["status"], "missing")


if __name__ == "__main__":
    unittest.main()
