#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fake-clock tests for the dashboard capture timing helpers."""
import importlib.util
import os
import pathlib
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


residency = load_module("dashboard_residency", ROOT / "dashboard-residency.py")
launcher = load_module("launcher_proof", ROOT / "launcher-proof.py")


class FakeClock:
    def __init__(self):
        self.now = 0.0
        self.events = []

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.events.append(("sleep", seconds))
        self.now += seconds

    def advance(self, seconds):
        self.now += seconds

    def idle(self, pid, seconds):
        self.events.append(("idle", pid, seconds))
        self.now += seconds
        return {"seconds": seconds}


class CaptureWindowTests(unittest.TestCase):
    def test_acceptance_full_idle_not_sleep_then_one_second(self):
        clock = FakeClock()
        began = clock.monotonic()
        clock.advance(2)
        row, timing = residency.finish_phase_window(
            "never-opened", began, 65,
            lambda seconds: residency.observe_window(clock.idle, 17, seconds, require_full=True),
            True, clock.monotonic, clock.sleep,
        )
        self.assertEqual(clock.events, [("sleep", 3), ("idle", 17, 60)])
        self.assertEqual(row["seconds"], 60)
        self.assertEqual(timing["pre_window_wait_seconds"], 3)
        self.assertEqual(timing["actual_idle_seconds"], 60)

    def test_acceptance_warmup_precedes_bar_baseline(self):
        clock = FakeClock()
        row, timing = residency.acceptance_baseline(22, clock.idle, 600, clock.monotonic, clock.sleep)
        self.assertEqual(clock.events, [("sleep", 600), ("idle", 22, 60)])
        self.assertEqual(timing, {
            "mode": "acceptance",
            "warmup_requested_seconds": 600,
            "warmup_elapsed_seconds": 600,
            "baseline_idle_seconds": 60,
        })
        self.assertEqual(row["seconds"], 60)

    def test_acceptance_phase_overrun_rejected(self):
        clock = FakeClock()
        began = clock.monotonic()
        clock.advance(6)
        with self.assertRaisesRegex(AssertionError, "overran"):
            residency.finish_phase_window(
                "never-opened", began, 65, lambda _: self.fail("capture after overrun"),
                True, clock.monotonic, clock.sleep,
            )
        self.assertEqual(clock.events, [])

    def test_acceptance_both_arms_have_same_schedule(self):
        budgets = list(residency.ACCEPTANCE_PHASE_BUDGETS.items())
        self.assertEqual(budgets, [
            ("never-opened", 65), ("opened-once", 85),
            ("opened-several", 105), ("combined-legacy", 95),
        ])

        def arm_schedule():
            clock = FakeClock()
            schedule = []
            for label, budget in budgets:
                began = clock.monotonic()
                activity = {"never-opened": 2, "opened-once": 12,
                            "opened-several": 22, "combined-legacy": 18}[label]
                clock.advance(activity)
                _, timing = residency.finish_phase_window(
                    label, began, budget,
                    lambda seconds: residency.observe_window(clock.idle, 4, seconds, require_full=True),
                    True, clock.monotonic, clock.sleep,
                )
                schedule.append((timing["phase_budget_seconds"], timing["activity_seconds"],
                                 timing["pre_window_wait_seconds"], timing["actual_idle_seconds"]))
            return schedule

        self.assertEqual(arm_schedule(), arm_schedule())

    def test_legacy_mode_preserves_existing_windows_and_close_idle(self):
        self.assertIsNone(residency.acceptance_config({}))
        clock = FakeClock()
        began = clock.monotonic()
        clock.advance(4)
        _, timing = residency.finish_phase_window(
            "never-opened", began, 35,
            lambda seconds: residency.observe_window(clock.idle, 9, seconds),
            False, clock.monotonic, clock.sleep,
        )
        self.assertEqual(clock.events, [("sleep", 31), ("idle", 9, 1)])
        self.assertEqual(timing, {"mode": "legacy", "phase_seconds": 36})

    def test_invalid_duration_fails_before_launch(self):
        environment = {
            "BEAD_DASHBOARD_ACCEPTANCE": "1",
            "BEAD_DASHBOARD_RESIDENCY": "1",
            "BEAD_DASHBOARD_RESIDENCY_SLOTS": "4",
            "BEAD_DASHBOARD_RESIDENCY_LEGACY": "1",
            "BEAD_DASHBOARD_WARMUP_SECONDS": "300",
        }
        with mock.patch.dict(os.environ, environment), mock.patch.object(launcher.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(ValueError, "must be 0 or 600"):
                launcher.sample({}, pathlib.Path("unused"), "niri", lambda _: None)
            popen.assert_not_called()

    def test_close_swap_and_memory_rails_still_fail(self):
        bar = {"pss_plus_drm_mib": 20, "memory": {"swap_pss_mib": 0}}
        external = {"processes": [{"memory": {"swap_pss_mib": 0}}]}
        total = lambda row: row["pss_plus_drm_mib"] + row["memory"]["swap_pss_mib"]
        good = {"pss_plus_drm_mib": 24, "memory": {"swap_pss_mib": 0}}
        residency.assert_capture_rails(good, bar, external, total)
        with self.assertRaisesRegex(AssertionError, "UI swap"):
            residency.assert_capture_rails({**good, "memory": {"swap_pss_mib": 1}}, bar, external, total)
        daemon_swap = {"processes": [{"memory": {"swap_pss_mib": 1}}]}
        with self.assertRaisesRegex(AssertionError, "daemon swap"):
            residency.assert_capture_rails(good, bar, daemon_swap, total)
        with self.assertRaisesRegex(AssertionError, r"bar\+5"):
            residency.assert_capture_rails({**good, "pss_plus_drm_mib": 26}, bar, external, total)
        near_ceiling_bar = {"pss_plus_drm_mib": 22, "memory": {"swap_pss_mib": 0}}
        with self.assertRaisesRegex(AssertionError, "hard ceiling"):
            residency.assert_capture_rails({**good, "pss_plus_drm_mib": 26}, near_ceiling_bar, external, total)


if __name__ == "__main__":
    unittest.main()
