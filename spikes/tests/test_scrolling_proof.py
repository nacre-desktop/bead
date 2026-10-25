#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic log joins for proving a presented scroll direction reversal."""
import importlib.util
import os
import pathlib
import sys
from subprocess import CompletedProcess
import tempfile
from types import SimpleNamespace
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

    def test_presented_cadence_counts_feedback_discards_not_commits(self):
        log = self.root / "presentation.log"
        log.write_text(
            "\n".join(
                [
                    "WINDOW_PRESENTED monotonic_ns=100000000 refresh_ns=8333333 epoch=3 update=1 generation=9",
                    "WINDOW_PRESENTED monotonic_ns=108333333 refresh_ns=8333333 epoch=3 update=2 generation=9",
                    "WINDOW_DISCARDED monotonic_ns=110000000 epoch=3 update=3 generation=9",
                    "WINDOW_PRESENTED monotonic_ns=116666666 refresh_ns=8333333 epoch=3 update=4 generation=9",
                ]
            )
            + "\n"
        )
        metrics = pacing.presented_cadence([log], [], 90_000_000, 125_000_000, 0.09, 0.125)
        self.assertEqual(metrics["presentation_source"], "window:presentation.log:3")
        self.assertEqual(metrics["presentation_interval_ms"]["p95"], 8.33)
        self.assertEqual(metrics["presented_dropped"], 0)
        self.assertEqual(metrics["presented_discarded"], 1)

    def test_presented_cadence_keeps_long_jank_gap_in_drop_metrics(self):
        log = self.root / "presentation-long-gap.log"
        log.write_text(
            "\n".join(
                [
                    "WINDOW_PRESENTED monotonic_ns=100000000 refresh_ns=8333333 epoch=3 update=1 generation=9",
                    "WINDOW_PRESENTED monotonic_ns=108333333 refresh_ns=8333333 epoch=3 update=2 generation=9",
                    "WINDOW_PRESENTED monotonic_ns=608333333 refresh_ns=8333333 epoch=3 update=3 generation=9",
                ]
            )
            + "\n"
        )
        metrics = pacing.presented_cadence([log], [], 90_000_000, 625_000_000, 0.09, 0.625)
        self.assertEqual(metrics["presented_long_gaps"], 1)
        self.assertEqual(metrics["presented_max_gap_ms"], 500.0)
        self.assertEqual(metrics["presented_dropped"], 29)

    def test_app_log_archiving_preserves_each_spawn_and_open_writer(self):
        log_dir = self.root / "logs"
        log_dir.mkdir()
        current = log_dir / "app-settings.log"
        current.write_text("first spawn\n")
        writer = current.open("a")
        first = pacing.archive_app_logs(log_dir, "scroll-settings-content-touchpad", 0)[0]
        writer.write("late first-spawn line\n")
        writer.close()
        current.write_text("second spawn\n")
        second = pacing.archive_app_logs(log_dir, "scroll-settings-content-touchpad", 1)[0]
        self.assertEqual(first.read_text(), "first spawn\nlate first-spawn line\n")
        self.assertEqual(second.read_text(), "second spawn\n")
        self.assertFalse(current.exists())

    def test_bead_app_processes_inherit_trace_log_directory(self):
        env = pacing.shell_trace_env("bead", self.root / "logs", {"KEEP": "value"})
        self.assertEqual(env["BEAD_APP_LOG_DIR"], str(self.root / "logs"))
        self.assertEqual(env["WAYLAND_DEBUG"], "client")
        self.assertEqual(env["BEAD_FRAME_TRACE"], "1")
        self.assertEqual(env["KEEP"], "value")
        self.assertNotIn("BEAD_APP_LOG_DIR", pacing.shell_trace_env("qml", self.root / "logs"))

    def test_active_scroll_oscillation_changes_finger_stream_only(self):
        self.assertEqual(pacing.scroll_stream_mode("pixel", True, True), "stream-oscillate")
        self.assertEqual(pacing.scroll_stream_mode("wheel", True, True), "stream-reverse")
        self.assertEqual(pacing.scroll_stream_mode("pixel", True, False), "stream-reverse")

    def test_native_scroll_scenes_close_the_app_they_open(self):
        for scene in ("colors-app", "colors-rail"):
            for device in ("touchpad", "wheel"):
                self.assertEqual(pacing.SCROLL_CLEANUPS[f"scroll-{scene}-{device}"], [("app_close", "colors")])
        for scene in ("welcome-hero", "welcome-shortcuts", "welcome-apps", "welcome-ready", "welcome-help"):
            for device in ("touchpad", "wheel"):
                self.assertEqual(pacing.SCROLL_CLEANUPS[f"scroll-{scene}-{device}"], [("app_close", "welcome")])

    def test_notification_history_closes_active_fixture_rows_before_open(self):
        prepare = pacing.SCROLL_SCENES["notifications-list"][0]
        self.assertLess(prepare.index(("close-fixture-notices",)), prepare.index(("ui", "nacre", "controls", "notifications")))
        calls = []

        def run(command, check=False):
            calls.append(command)
            return CompletedProcess(command, 0, stdout="()", stderr="")

        arm = SimpleNamespace(fixture_active_ids=[17, 18], fixture_closed_ids=[], errors=[], run=run)
        self.assertEqual(pacing.close_active_fixture_notifications(arm), [17, 18])
        self.assertEqual(arm.fixture_active_ids, [])
        self.assertEqual(arm.fixture_closed_ids, [17, 18])
        self.assertEqual([command[-1] for command in calls], ["17", "18"])

    def test_scroll_notification_fixture_uses_long_ttl_and_closes_exact_ids(self):
        calls = []

        def run(command, check=False):
            calls.append(command)
            output = "(uint32 17,)" if command[-9].endswith(".Notify") else "()"
            return CompletedProcess(command, 0, stdout=output, stderr="")

        arm = SimpleNamespace(scroll_notice_fixture=True, fixture_notice_ids=[], errors=[], run=run)
        pacing.notify(arm, "Scroll fixture", "Private test row")
        self.assertEqual(calls[0][-1], "60000")
        self.assertEqual(arm.fixture_notice_ids, [17])
        self.assertEqual(pacing.close_fixture_notifications(arm, [17]), [17])
        self.assertTrue(calls[1][-2].endswith(".CloseNotification"))
        self.assertEqual(calls[1][-1], "17")

    def test_existing_notification_animation_keeps_original_ttl(self):
        calls = []

        def run(command, check=False):
            calls.append(command)
            return CompletedProcess(command, 0, stdout="(uint32 7,)", stderr="")

        arm = SimpleNamespace(scroll_notice_fixture=False, run=run)
        pacing.notify(arm, "Pacing check", "Existing animation fixture")
        self.assertEqual(calls[0][-1], "8000")


if __name__ == "__main__":
    unittest.main()
