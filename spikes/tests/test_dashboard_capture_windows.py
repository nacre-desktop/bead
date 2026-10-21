#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fake-clock tests for the dashboard capture timing helpers."""
import importlib.util
import json
import os
import pathlib
import tempfile
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


residency = load_module("dashboard_residency", ROOT / "dashboard-residency.py")
batch_summary = load_module("dashboard_batch_summary", ROOT / "dashboard-batch-summary.py")
launcher = load_module("launcher_proof", ROOT / "launcher-proof.py")
capture_safety = load_module("capture_safety", ROOT / "capture_safety.py")
measure = load_module("measure", ROOT / "measure.py")


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


class DashboardOutputGeometryTests(unittest.TestCase):
    def test_hyprland_requires_exact_virtual_output_geometry(self):
        output = {"name": "bead-bench", "width": 2880, "height": 1800,
                  "scale": 1.5, "refreshRate": 120.0}
        self.assertEqual(measure.validate_dashboard_output_geometry("hyprland", [output]), output)
        for key, value in (("width", 1920), ("height", 1200), ("scale", 1.0), ("refreshRate", 60.0)):
            altered = {**output, key: value}
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, "geometry mismatch"):
                measure.validate_dashboard_output_geometry("hyprland", [altered])
        with self.assertRaisesRegex(RuntimeError, "exactly one"):
            measure.validate_dashboard_output_geometry("hyprland", [output, output])

    def test_niri_requires_physical_logical_scale_and_refresh_geometry(self):
        output = {"current_mode": 0, "modes": [{"width": 2880, "height": 1800, "refresh_rate": 60000}],
                  "logical": {"width": 1920, "height": 1200, "scale": 1.5}}
        self.assertEqual(measure.validate_dashboard_output_geometry("niri", {"HEADLESS-1": output}),
                         {"name": "HEADLESS-1", **output})
        mutations = (
            {**output, "logical": {**output["logical"], "width": 2881}},
            {**output, "logical": {**output["logical"], "height": 1201}},
            {**output, "logical": {**output["logical"], "scale": 1.0}},
            {**output, "modes": [{**output["modes"][0], "refresh_rate": 59940}]},
            {**output, "modes": [{**output["modes"][0], "width": 1920}]},
        )
        for altered in mutations:
            with self.subTest(altered=altered), self.assertRaisesRegex(RuntimeError, "exactly one"):
                measure.validate_dashboard_output_geometry("niri", {"HEADLESS-1": altered})


class FakeKeeper:
    def __init__(self, clock, lifetime_seconds):
        self.clock = clock
        self.expires_at = clock.now + lifetime_seconds

    def poll(self):
        return 0 if self.clock.now >= self.expires_at else None


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
        sampling_events = []
        row, timing = residency.acceptance_baseline(
            22, clock.idle, 600, clock.monotonic, clock.sleep,
            sampling_started=lambda: sampling_events.append(("start", clock.monotonic())))
        self.assertEqual(clock.events, [("sleep", 600), ("idle", 22, 60)])
        self.assertEqual(sampling_events, [("start", 600)])
        self.assertEqual(timing, {
            "mode": "acceptance",
            "warmup_requested_seconds": 600,
            "warmup_elapsed_seconds": 600,
            "baseline_idle_seconds": 60,
        })
        self.assertEqual(row["seconds"], 60)

    def test_acceptance_keeper_outlives_600_warmup(self):
        clock = FakeClock()
        keeper = FakeKeeper(clock, 1800)
        with mock.patch.object(launcher.subprocess, "Popen", return_value=keeper) as popen:
            owned = launcher.start_keyboard_keeper(True, True, {"fixture": "env"})
        popen.assert_called_once()
        self.assertEqual(popen.call_args.args[0], ["wtype", "-s", "1800000", ""])
        checks = []
        check = lambda stage: (checks.append(stage), launcher.check_keyboard_keeper(owned, stage))
        residency.acceptance_baseline(22, clock.idle, 600, clock.monotonic, clock.sleep, check)
        for label, budget in residency.ACCEPTANCE_PHASE_BUDGETS.items():
            began = clock.monotonic()
            clock.advance(2)
            residency.finish_phase_window(
                label, began, budget, lambda seconds: clock.idle(22, seconds), True,
                clock.monotonic, clock.sleep, check,
            )
        self.assertIn("after warmup", checks)
        self.assertIn("before baseline", checks)
        self.assertIn("after baseline", checks)
        self.assertEqual(checks.count("before phase idle observation"), 4)
        self.assertEqual(checks.count("after phase idle observation"), 4)
        self.assertLess(clock.now, 1800)

    def test_legacy_keeper_duration_unchanged(self):
        keeper = object()
        with mock.patch.object(launcher.subprocess, "Popen", return_value=keeper) as popen:
            self.assertIs(launcher.start_keyboard_keeper(True, False, {}), keeper)
        self.assertEqual(popen.call_args.args[0], ["wtype", "-s", "600000", ""])

    def test_acceptance_launcher_argv_covers_full_gate_schedule(self):
        acceptance = residency.acceptance_config({
            "BEAD_DASHBOARD_ACCEPTANCE": "1",
            "BEAD_DASHBOARD_RESIDENCY": "1",
            "BEAD_DASHBOARD_PAGES": "today,weather,media,system",
            "BEAD_DASHBOARD_RESIDENCY_SLOTS": "4",
            "BEAD_DASHBOARD_RESIDENCY_LEGACY": "1",
            "BEAD_DASHBOARD_WARMUP_SECONDS": "600",
        })
        command = launcher.build_launcher_command(
            "/frozen/nacre-shell", True, ["--menu-fixture", "/fixtures/menu.json"],
            pathlib.Path("/private/launcher.sock"), pathlib.Path("/private/raw"),
            True, True, "/fixtures/dashboard", False, None, "hyprland", acceptance,
        )
        lifetime_seconds = int(command[command.index("--seconds") + 1])
        required_seconds = (acceptance["warmup_seconds"] + 60
                            + sum(residency.ACCEPTANCE_PHASE_BUDGETS.values()))
        self.assertEqual(lifetime_seconds, 1800)
        self.assertEqual(lifetime_seconds * 1000, 1800000)
        self.assertEqual(required_seconds, 1010)
        self.assertLess(required_seconds, lifetime_seconds)
        self.assertIn("--dashboard-fixtures", command)
        self.assertEqual(command[-2:], ["--output", "bead-bench"])

    def test_legacy_launcher_argv_lifetimes_unchanged(self):
        core_command = launcher.build_launcher_command(
            "/frozen/nacre-shell", True, ["--menu-fixture", "/fixtures/menu.json"],
            pathlib.Path("/private/launcher.sock"), pathlib.Path("/private/raw"),
            False, True, None, False, None, "niri", None,
        )
        panel_command = launcher.build_launcher_command(
            "/frozen/nacre-shell", False, ["--menu-fixture", "/fixtures/menu.json"],
            pathlib.Path("/private/launcher.sock"), pathlib.Path("/private/raw"),
            False, False, None, False, None, "niri", None,
        )
        self.assertEqual(core_command[core_command.index("--seconds") + 1], "300")
        self.assertEqual(panel_command[panel_command.index("--seconds") + 1], "180")

    def test_early_keeper_exit_invalidates_and_cleans(self):
        for expires_at, window in ((600, "warmup"), (630, "baseline"), (690, "phase")):
            with self.subTest(window=window):
                clock = FakeClock()
                keeper = FakeKeeper(clock, expires_at)
                stopped = []
                with mock.patch.object(launcher.subprocess, "Popen", return_value=keeper) as popen:
                    owned = launcher.start_keyboard_keeper(True, True, {})
                check = lambda stage: launcher.check_keyboard_keeper(owned, stage)
                try:
                    if window == "warmup":
                        with self.assertRaisesRegex(AssertionError, "after warmup"):
                            residency.acceptance_baseline(22, clock.idle, 600, clock.monotonic, clock.sleep, check)
                        self.assertFalse(any(event[0] == "idle" for event in clock.events))
                    elif window == "baseline":
                        with self.assertRaisesRegex(AssertionError, "after baseline"):
                            residency.acceptance_baseline(22, clock.idle, 600, clock.monotonic, clock.sleep, check)
                    else:
                        clock.advance(660)
                        with self.assertRaisesRegex(AssertionError, "after phase idle observation"):
                            residency.finish_phase_window(
                                "never-opened", clock.monotonic(), 65,
                                lambda seconds: clock.idle(22, seconds), True,
                                clock.monotonic, clock.sleep, check,
                            )
                finally:
                    launcher.stop_keyboard_keeper(owned, stopped.append)
                popen.assert_called_once()
                self.assertEqual(stopped, [keeper])

    def test_keeper_not_requested_adds_no_process(self):
        with mock.patch.object(launcher.subprocess, "Popen") as popen:
            self.assertIsNone(launcher.start_keyboard_keeper(False, True, {}))
        popen.assert_not_called()

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

    def test_batch_median_mode_is_explicit_and_legacy_remains_strict(self):
        self.assertIsNone(residency.acceptance_config({}))
        env = {
            "BEAD_DASHBOARD_ACCEPTANCE": "1",
            "BEAD_DASHBOARD_RESIDENCY": "1",
            "BEAD_DASHBOARD_PAGES": "today,weather,media,system",
            "BEAD_DASHBOARD_RESIDENCY_SLOTS": "4",
            "BEAD_DASHBOARD_RESIDENCY_LEGACY": "1",
            "BEAD_DASHBOARD_WARMUP_SECONDS": "600",
            "BEAD_DASHBOARD_BATCH_MEDIANS": "1",
        }
        self.assertTrue(residency.acceptance_config(env)["batch_medians"])
        self.assertTrue(residency.acceptance_config(env)["legacy"])
        self.assertTrue(residency.acceptance_config({**env, "BEAD_DASHBOARD_OPENED_ONCE_ONLY": "1"})["opened_once_only"])
        self.assertTrue(residency.acceptance_config({**env, "BEAD_DASHBOARD_OPENED_ONCE_ONLY": "1"})["legacy"])
        self.assertFalse(residency.acceptance_config(env)["opened_once_only"])
        self.assertFalse(residency.acceptance_config({**env, "BEAD_DASHBOARD_BATCH_MEDIANS": "0"})["batch_medians"])
        self.assertEqual(residency.acceptance_config({**env, "BEAD_DASHBOARD_BATCH_MEDIANS": "0"})["warmup_seconds"], 600)

    def test_batch_median_gates_attribution_and_ignores_one_high_arm(self):
        def row(total, anon=5.0, file=8.0, shmem=1.0, drm=None):
            pss = anon + file + shmem
            if drm is None:
                drm = total - pss
            return {"pss_plus_drm_mib": total, "drm_mib": {"buffer": drm},
                    "memory": {"pss_mib": pss, "swap_pss_mib": 0.0,
                               "pss_anon_mib": anon, "pss_file_mib": file,
                               "pss_shmem_mib": shmem}}
        control = [row(24.0) for _ in range(5)]
        candidate = [row(24.9), row(25.0, file=9.0),
                     row(25.0, file=9.0), row(25.0, file=9.0), row(27.5, file=11.5)]
        summary = residency.summarize_batch_phase(control, candidate)
        self.assertEqual(summary["candidate_total_median_mib"], 25.0)
        self.assertEqual(summary["total_delta_median_mib"], 1.0)
        self.assertEqual(summary["category_delta_medians_mib"]["pss_file_mib"], 1.0)
        self.assertEqual(summary["category_medians_mib"]["control"]["pss_file_mib"], 8.0)
        self.assertEqual(summary["verdict"], "PASS-one-time-27MiB-file-backed-report")
        self.assertEqual(summary["candidate_range_mib"], [24.9, 27.5])
        self.assertEqual(summary["drm_attribution"], "measured")
        self.assertEqual(residency.summarize_batch_phase(control, [row(24.5, file=8.5) for _ in range(5)])["verdict"], "PASS")
        with self.assertRaisesRegex(AssertionError, "anonymous growth"):
            residency.summarize_batch_phase(control, [row(25.1, anon=6.1, file=8.0) for _ in range(5)])
        with self.assertRaisesRegex(AssertionError, "file-backed"):
            residency.summarize_batch_phase(control, [row(25.0, file=8.0) for _ in range(5)])
        with self.assertRaisesRegex(AssertionError, "27 MiB"):
            residency.summarize_batch_phase(control, [row(27.1, file=11.1) for _ in range(5)])
        with self.assertRaisesRegex(AssertionError, "one-time allowance"):
            residency.summarize_batch_phase(control, candidate, allow_27_report=False)
        with self.assertRaisesRegex(AssertionError, "exactly five"):
            residency.summarize_batch_phase(control[:4], candidate[:4])

    def test_batch_median_keeps_missing_drm_unknown_and_rejects_bad_values(self):
        base = {"pss_plus_drm_mib": 24.0, "drm_mib": None,
                "memory": {"pss_mib": 24.0, "swap_pss_mib": 0.0, "pss_anon_mib": 8.0,
                           "pss_file_mib": 12.0, "pss_shmem_mib": 4.0}}
        summary = residency.summarize_batch_phase([base.copy() for _ in range(5)], [base.copy() for _ in range(5)])
        self.assertEqual(summary["drm_attribution"], "unknown")
        self.assertIsNone(summary["control_drm_median_mib"])
        higher = {**base, "pss_plus_drm_mib": 25.0,
                  "memory": {**base["memory"], "pss_mib": 25.0, "pss_file_mib": 13.0}}
        with self.assertRaisesRegex(AssertionError, "measured DRM"):
            residency.summarize_batch_phase([base.copy() for _ in range(5)], [higher.copy() for _ in range(5)])
        bad = {**base, "memory": {**base["memory"], "pss_file_mib": float("nan")}}
        with self.assertRaisesRegex(AssertionError, "invalid or negative"):
            residency.summarize_batch_phase([base.copy() for _ in range(5)], [bad.copy() for _ in range(5)])

    def test_batch_summary_collects_all_phases_and_uses_27_mib_once(self):
        import tempfile
        def row(total, file):
            anon, shmem = 5.0, 1.0
            pss = anon + file + shmem
            return {"pss_plus_drm_mib": total, "drm_mib": {"buffer": total - pss},
                    "memory": {"pss_mib": pss, "swap_pss_mib": 0.0, "pss_anon_mib": anon,
                               "pss_file_mib": file, "pss_shmem_mib": shmem}}
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for compositor in batch_summary.COMPOSITORS:
                for pair in range(1, 6):
                    for arm, total, file in (("A", 24.0, 8.0), ("B", 24.5, 8.5)):
                        for phase in batch_summary.PHASES:
                            value, file_value = total, file
                            if compositor == "hyprland" and phase == "never-opened" and arm == "B":
                                value, file_value = 25.0, 9.0
                            if compositor == "niri" and phase == "opened-once" and arm == "B":
                                value, file_value = 26.0, 10.0
                            folder = root / f"{arm}{pair}" / compositor
                            folder.mkdir(parents=True, exist_ok=True)
                            (folder / f"{phase}.json").write_text(json.dumps(row(value, file_value)))
            output = root / "summary.json"
            report = batch_summary.summarize(root, report_path=output)
            self.assertTrue(report["complete"])
            self.assertFalse(report["pass"])
            self.assertTrue(report["one_time_27_mib_report_used"])
            self.assertEqual(len(report["phases"]), 8)
            self.assertIn("one-time allowance", report["phases"]["niri/opened-once"]["error"])
            self.assertTrue(output.is_file())
            self.assertEqual(json.loads(output.read_text())["phases"], report["phases"])

    def test_batch_arm_rail_allows_median_evaluation_but_preserves_quiet_and_close_cap(self):
        total = lambda row: row["pss_plus_drm_mib"] + row["memory"]["swap_pss_mib"]
        bar = {"pss_plus_drm_mib": 24, "memory": {"swap_pss_mib": 0, "pss_anon_mib": 5}}
        row = {"pss_plus_drm_mib": 26, "memory": {"swap_pss_mib": 0, "pss_anon_mib": 5.5,
                "pss_file_mib": 15, "pss_shmem_mib": 1}}
        external = {"processes": [{"memory": {"swap_pss_mib": 0}}]}
        residency.assert_capture_rails(row, bar, external, total, batch_medians=True)
        close_bar = {"pss_plus_drm_mib": 20, "memory": {"swap_pss_mib": 0, "pss_anon_mib": 5}}
        with self.assertRaisesRegex(AssertionError, r"bar\+5"):
            residency.assert_capture_rails(row, close_bar, external, total, batch_medians=True, close=True)
        with self.assertRaisesRegex(AssertionError, "UI swap"):
            residency.assert_capture_rails({**row, "memory": {**row["memory"], "swap_pss_mib": 1}},
                                            bar, external, total, batch_medians=True)


class CaptureSafetyTests(unittest.TestCase):
    def _run_mock_guard(self, temporary, cgroup_snapshots, monotonic, scope_poll_values=None, env=None,
                        child_group="/scope", watchdog_starts=None, child_start_ticks=456,
                        manager_readbacks=None, start_ticks_readbacks=None, group_readbacks=None,
                        cgroup_pids=None, events=None,
                        release_observations=None, scope_read_states=None):
        env = {} if env is None else env
        cgroup_pids = {321} if cgroup_pids is None else cgroup_pids
        baseline_peak = (capture_safety.parse_baseline_peak_bytes(env[capture_safety.BASELINE_SCOPE_PEAK_ENV])
                         if capture_safety.BASELINE_SCOPE_PEAK_ENV in env else None)
        expected_memory_max = capture_safety.scope_memory_max(baseline_peak)
        cgroup_snapshots = [
            {"memory.max": str(expected_memory_max), "memory.swap.max": "0", **snapshot}
            for snapshot in cgroup_snapshots
        ]
        remaining_snapshots = list(cgroup_snapshots)

        def next_cgroup_snapshot(_control_group):
            scope_alive = scope.finish_release_file is None or not scope.finish_release_file.exists()
            if scope_read_states is not None:
                scope_read_states.append(scope_alive)
            if events is not None:
                events.append("cgroup-read-while-scope-live" if scope_alive else "cgroup-read-after-scope-exit")
            if not scope_alive:
                return {"memory.events": "unavailable: scope removed"}
            if len(remaining_snapshots) > 1:
                return remaining_snapshots.pop(0)
            return remaining_snapshots[0]
        class FakeScope:
            def __init__(self):
                self.poll_values = list(scope_poll_values or [])
                self.returncode = None
                self.finish_release_file = None

            def poll(self):
                if self.poll_values:
                    value = self.poll_values.pop(0)
                    if value is not None:
                        self.returncode = value
                if (self.returncode is None and self.finish_release_file is not None
                        and self.finish_release_file.exists()):
                    self.returncode = 0
                return self.returncode

            def wait(self, timeout=None):
                self.returncode = 0
                record("scope-released")
                return 0

            def terminate(self):
                self.returncode = -15

        class FakeWatchdog:
            def poll(self):
                return None

            def wait(self, timeout=None):
                return 0

        scope, watchdog = FakeScope(), FakeWatchdog()
        commands = []
        readback_counts = {}
        start_read_count = 0
        group_read_count = 0

        def record(event):
            if events is not None:
                events.append(event)

        def readback(prop, default):
            values = (manager_readbacks or {}).get(prop)
            if values is None:
                return default
            index = readback_counts.get(prop, 0)
            readback_counts[prop] = index + 1
            return values[min(index, len(values) - 1)]

        def fake_popen(command, **kwargs):
            if command[0] == "systemd-run":
                record("child-started-and-held")
                ready = pathlib.Path(command[command.index("--ready-file") + 1])
                release = pathlib.Path(command[command.index("--release-file") + 1])
                finished = pathlib.Path(command[command.index("--finished-file") + 1])
                scope.finish_release_file = pathlib.Path(
                    command[command.index("--finish-release-file") + 1])
                scope.finished_file = finished
                scope.release_file = release
                ready.write_text(json.dumps({"pid": 321, "start_ticks": child_start_ticks,
                                             "cgroup": child_group}))
                return scope
            record("watchdog-started")
            if watchdog_starts is not None:
                watchdog_starts.append(command)
            ready = pathlib.Path(command[command.index("--ready-file") + 1])
            heartbeat = pathlib.Path(command[command.index("--heartbeat-file") + 1])
            ready.write_text(json.dumps({"available_kib": capture_safety.START_MIN_KIB,
                                         "monotonic": 100.0}))
            heartbeat.write_text(json.dumps({"monotonic": 100.0}))
            return watchdog

        def fake_run(command, **kwargs):
            commands.append(command)
            prop = next((part.split("=", 1)[1] for part in command if part.startswith("--property=")), "")
            record(f"manager-read:{prop}")
            output = {"MemoryMax": str(expected_memory_max),
                      "MemorySwapMax": "0", "ControlGroup": "/scope"}.get(prop, "")
            output = readback(prop, output)
            return mock.Mock(returncode=0, stdout=output, stderr="")

        def start_ticks_reader(pid):
            nonlocal start_read_count
            record("child-start-ticks-read")
            if pid != 321:
                return 999
            index = start_read_count
            start_read_count += 1
            if start_ticks_readbacks:
                return start_ticks_readbacks[min(index, len(start_ticks_readbacks) - 1)]
            return child_start_ticks

        def group_reader(pid):
            nonlocal group_read_count
            record("child-cgroup-read")
            if pid != 321:
                return child_group
            index = group_read_count
            group_read_count += 1
            if group_readbacks:
                return group_readbacks[min(index, len(group_readbacks) - 1)]
            return child_group

        real_release_after = capture_safety.release_after_watchdog_ready

        def observed_release(**kwargs):
            pre_release_check = kwargs["pre_release_check"]

            def tracked_check():
                result = pre_release_check()
                record("pre-release-check-passed")
                return result

            kwargs["pre_release_check"] = tracked_check
            try:
                result = real_release_after(**kwargs)
            except BaseException:
                marker_exists = pathlib.Path(kwargs["release_file"]).exists()
                if release_observations is not None:
                    release_observations.append(marker_exists)
                record(f"release-failed-marker:{marker_exists}")
                raise
            marker_exists = pathlib.Path(kwargs["release_file"]).exists()
            if release_observations is not None:
                release_observations.append(marker_exists)
            record(f"release-marker:{marker_exists}")
            pathlib.Path(scope.finished_file).write_text(json.dumps({"returncode": 0}) + "\n")
            record("capture-finished-scope-held")
            return result

        with (mock.patch.object(capture_safety, "cgroup_memory_snapshot", side_effect=next_cgroup_snapshot),
              mock.patch.object(capture_safety, "release_after_watchdog_ready", side_effect=observed_release),
              mock.patch.object(capture_safety, "process_start_ticks", side_effect=start_ticks_reader),
              mock.patch.object(capture_safety, "process_cgroup", side_effect=group_reader),
              mock.patch.object(capture_safety, "read_cgroup_processes", return_value=cgroup_pids)):
            try:
                result = capture_safety.run_guarded(
                    ["python3", "measure.py"], report_path=pathlib.Path(temporary) / "safety.json",
                    env=env, memory_reader=lambda: capture_safety.START_MIN_KIB,
                    popen=fake_popen, run=fake_run, sleeper=lambda _: None,
                    monotonic=monotonic, unit_id="012345abcdef",
                )
                error = None
            except BaseException as caught:
                result, error = None, caught
        report = json.loads((pathlib.Path(temporary) / "safety.json").read_text())
        return result, error, commands, report

    def test_memavailable_parser_requires_kernel_field(self):
        self.assertEqual(capture_safety.parse_memavailable_kib("MemTotal: 9000 kB\nMemAvailable: 4194304 kB\n"), 4194304)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "missing or malformed"):
            capture_safety.parse_memavailable_kib("MemTotal: 9000 kB\n")
        for contents in ("MemAvailable: 0 kB\n", "MemAvailable: nope kB\n"):
            with self.subTest(contents=contents), self.assertRaises(capture_safety.CaptureSafetyError):
                capture_safety.parse_memavailable_kib(contents)

    def test_scope_is_exact_limit_and_contains_harness(self):
        capture_safety.validate_scope(
            "bead-capture-012345abcdef.scope", str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0",
            "/user.slice/user-1000.slice/bead-capture-012345abcdef.scope",
            "/user.slice/user-1000.slice/bead-capture-012345abcdef.scope",
            str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0", capture_safety.SCOPE_MEMORY_MAX_MIN,
        )
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "MemoryMax"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope", "0", "0", "/scope", "/scope",
                                          "0", "0", capture_safety.SCOPE_MEMORY_MAX_MIN)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "MemorySwapMax"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "1", "/scope", "/scope",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0",
                                          capture_safety.SCOPE_MEMORY_MAX_MIN)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "memory.swap.max"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0", "/scope", "/scope",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "max",
                                          capture_safety.SCOPE_MEMORY_MAX_MIN)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "outside"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0", "/scope/a", "/scope-b",
                                          str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0",
                                          capture_safety.SCOPE_MEMORY_MAX_MIN)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "unrecognized"):
            capture_safety.validate_scope("other.scope", str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0",
                                          "/scope", "/scope", str(capture_safety.SCOPE_MEMORY_MAX_MIN), "0",
                                          capture_safety.SCOPE_MEMORY_MAX_MIN)

    def test_scope_limit_uses_minimum_or_one_and_a_half_times_recorded_peak(self):
        self.assertEqual(capture_safety.scope_memory_max(), 12 * 1024 ** 3)
        self.assertEqual(capture_safety.parse_baseline_peak_bytes("8589934592"), 8 * 1024 ** 3)
        self.assertEqual(capture_safety.scope_memory_max(8 * 1024 ** 3), 12 * 1024 ** 3)
        self.assertEqual(capture_safety.scope_memory_max(10 * 1024 ** 3), 15 * 1024 ** 3)
        self.assertEqual(capture_safety.scope_memory_max(1), 12 * 1024 ** 3)
        self.assertEqual(capture_safety.scope_memory_max(12 * 1024 ** 3 + 1), 18 * 1024 ** 3 + 2)
        for value in ("", "0", "-1", "+1", "1.5", "nope"):
            with self.subTest(value=value), self.assertRaises(capture_safety.CaptureSafetyError):
                capture_safety.parse_baseline_peak_bytes(value)
        for value in (True, 0, -1, 1.5, "12"):
            with self.subTest(value=value), self.assertRaises(capture_safety.CaptureSafetyError):
                capture_safety.scope_memory_max(value)

    def test_guard_binds_derived_cap_to_manager_and_cgroup(self):
        peak = 10 * 1024 ** 3
        expected = 15 * 1024 ** 3
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": str(peak)}
        with tempfile.TemporaryDirectory() as temporary:
            result, error, _commands, report = self._run_mock_guard(
                temporary, [stable, stable], lambda: 100.0, env={capture_safety.BASELINE_SCOPE_PEAK_ENV: str(peak)})
        self.assertIsNone(error)
        self.assertEqual(result, 0)
        scope_command = capture_safety.scope_command("bead-capture-012345abcdef.scope", ["python3"], expected)
        self.assertIn(f"--property=MemoryMax={expected}", scope_command)
        self.assertIn("--property=MemorySwapMax=0", scope_command)
        self.assertEqual(report["memory_max_bytes"], expected)
        self.assertEqual(report["baseline_scope_peak_bytes"], peak)

    def test_guard_rechecks_held_child_and_scope_before_release_marker(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        events, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, _commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0, events=events,
                release_observations=release_observations)
        self.assertIsNone(error)
        self.assertEqual(result, 0)
        self.assertEqual(release_observations, [True])
        self.assertEqual(report["child_start_ticks"], 456)
        self.assertTrue(report["child_membership_verified"])
        self.assertEqual(report["pre_release_verification"]["child_pid"], 321)
        self.assertEqual(report["pre_release_verification"]["child_start_ticks"], 456)
        self.assertTrue(report["pre_release_verification"]["child_membership_verified"])
        self.assertLess(events.index("child-started-and-held"), events.index("watchdog-started"))
        self.assertLess(events.index("watchdog-started"), events.index("pre-release-check-passed"))
        self.assertLess(events.index("pre-release-check-passed"), events.index("release-marker:True"))
        marker = events.index("release-marker:True")
        self.assertGreaterEqual(events[:marker].count("child-start-ticks-read"), 5)
        for prop in ("MemoryMax", "MemorySwapMax", "ControlGroup"):
            reads = [index for index, item in enumerate(events[:marker]) if item == f"manager-read:{prop}"]
            self.assertGreaterEqual(len(reads), 3, prop)

    def test_final_oom_read_is_held_before_teardown_and_reproduces_old_race(self):
        # Old ordering: systemd --collect removes the cgroup before the final
        # memory.events read, so the required OOM counters become unavailable.
        old_snapshot = {"memory.events": "unavailable: cgroup removed"}
        self.assertIsNone(capture_safety.parse_oom_counters(old_snapshot))
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "unavailable after teardown"):
            if capture_safety.parse_oom_counters(old_snapshot) is None:
                raise capture_safety.CaptureSafetyError("cgroup OOM counters unavailable after teardown")

        stable = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096"}
        events, scope_reads = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, _commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0, events=events,
                scope_read_states=scope_reads)
        self.assertIsNone(error)
        self.assertEqual(result, 0)
        self.assertTrue(report["final_cgroup_read_while_scope_held"])
        self.assertEqual(report["oom_delta"], {"oom": 0, "oom_kill": 0})
        self.assertTrue(scope_reads)
        self.assertTrue(all(scope_reads))
        final_live_read = max(index for index, event in enumerate(events)
                              if event == "cgroup-read-while-scope-live")
        self.assertLess(final_live_read, events.index("scope-released"))

    def test_final_missing_oom_counters_while_scope_held_still_invalidates(self):
        stable = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096"}
        unavailable = {"memory.events": "unavailable: simulated read failure", "memory.peak": "4096"}
        scope_reads = []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, _commands, report = self._run_mock_guard(
                temporary, [stable, stable, stable, unavailable], lambda: 100.0,
                scope_read_states=scope_reads)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("unavailable while capture scope was held", str(error))
        self.assertTrue(report["final_cgroup_read_while_scope_held"])
        self.assertEqual(report["status"], "aborted")
        self.assertTrue(all(scope_reads))

    def test_guard_rejects_manager_readback_mismatch_before_watchdog(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0,
                manager_readbacks={"MemorySwapMax": ["1024"]},
                watchdog_starts=watchdog_starts, release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("MemorySwapMax", str(error))
        self.assertEqual(watchdog_starts, [])
        self.assertEqual(release_observations, [])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_rejects_reused_child_pid_before_starting_watchdog(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0, child_start_ticks=456,
                start_ticks_readbacks=[789], watchdog_starts=watchdog_starts,
                release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("start ticks mismatch", str(error))
        self.assertEqual(watchdog_starts, [])
        self.assertEqual(release_observations, [])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_rechecks_memory_limit_immediately_before_release(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        expected = capture_safety.SCOPE_MEMORY_MAX_MIN
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0,
                manager_readbacks={"MemoryMax": [str(expected), str(expected), str(expected + 1)]},
                watchdog_starts=watchdog_starts, release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("MemoryMax", str(error))
        self.assertEqual(len(watchdog_starts), 1)
        self.assertEqual(release_observations, [False])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_rejects_child_start_time_change_immediately_before_release(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0,
                start_ticks_readbacks=[456, 456, 456, 999],
                watchdog_starts=watchdog_starts, release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("identity changed", str(error))
        self.assertEqual(len(watchdog_starts), 1)
        self.assertEqual(release_observations, [False])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_rejects_child_moved_immediately_before_release(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0,
                group_readbacks=["/scope", "/scope", "/outside"],
                watchdog_starts=watchdog_starts, release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("moved", str(error))
        self.assertEqual(len(watchdog_starts), 1)
        self.assertEqual(release_observations, [False])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_rejects_child_missing_from_cgroup_procs_before_watchdog(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        watchdog_starts, release_observations = [], []
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0, cgroup_pids=set(),
                watchdog_starts=watchdog_starts, release_observations=release_observations)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("absent from cgroup.procs", str(error))
        self.assertEqual(watchdog_starts, [])
        self.assertEqual(release_observations, [])
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_held_child_identifiers_must_be_strict_positive_integers(self):
        for pid, ticks in ((True, 456), (0, 456), (321, True), (321, 0), ("321", 456), (321, "456")):
            with self.subTest(pid=pid, ticks=ticks), self.assertRaises(capture_safety.CaptureSafetyError):
                capture_safety.validate_held_child(
                    pid, ticks, "/scope", "/scope", start_reader=lambda _pid: 456,
                    group_reader=lambda _pid: "/scope", membership_reader=lambda _group: {321})

    def test_guard_stops_owned_scope_if_effective_cgroup_swap_is_not_zero(self):
        unsafe = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096",
                  "memory.swap.max": "max"}
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [unsafe], lambda: 100.0)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("memory.swap.max", str(error))
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])

    def test_guard_stops_scope_before_watchdog_when_child_escapes_containment(self):
        stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "4096"}
        with tempfile.TemporaryDirectory() as temporary:
            watchdog_starts = []
            result, error, commands, report = self._run_mock_guard(
                temporary, [stable], lambda: 100.0, child_group="/scope-escape",
                watchdog_starts=watchdog_starts)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("outside", str(error))
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", report["scope"]])
        self.assertEqual(watchdog_starts, [])

    def test_scope_command_sets_the_fixed_memory_cap(self):
        command = capture_safety.scope_command(
            "bead-capture-012345abcdef.scope", ["python3", "measure.py"])
        self.assertIn("--property=MemoryMax=12884901888", command)
        self.assertIn("--property=MemorySwapMax=0", command)
        self.assertIn("--scope", command)
        self.assertNotIn("--wait", command)
        self.assertEqual(command[-2:], ["python3", "measure.py"])

    def test_start_below_four_gib_fails_before_scope_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(capture_safety.shutil, "which", return_value="unused"):
                with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "4-GiB start rail"):
                    capture_safety.run_guarded(
                        ["unused"], report_path=pathlib.Path(temporary) / "safety.json",
                        memory_reader=lambda: capture_safety.START_MIN_KIB - 1,
                        popen=mock.Mock(side_effect=AssertionError("scope must not start")),
                    )
                report = json.loads((pathlib.Path(temporary) / "safety.json").read_text())
                self.assertEqual(report["status"], "refused-low-memory")

    def test_release_is_not_touched_before_watchdog_ready(self):
        class FakeClock:
            now = 100.0

            def monotonic(self):
                return self.now

            def sleep(self, seconds):
                self.now += seconds

        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            release_file = root / "release"
            clock = FakeClock()
            watchdog = mock.Mock(poll=mock.Mock(return_value=None))
            with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "timed out waiting for watchdog READY"):
                capture_safety.release_after_watchdog_ready(
                    ready_file=root / "ready.json", heartbeat_file=root / "heartbeat.json",
                    release_file=release_file, watchdog=watchdog,
                    memory_reader=lambda: capture_safety.START_MIN_KIB,
                    sleeper=clock.sleep, monotonic=clock.monotonic, timeout=0.2,
                )
            self.assertFalse(release_file.exists())
            self.assertGreater(watchdog.poll.call_count, 0)

    def test_stale_heartbeat_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            heartbeat = pathlib.Path(temporary) / "heartbeat.json"
            heartbeat.write_text(json.dumps({"monotonic": 1.0}))
            with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "heartbeat is stale"):
                capture_safety.check_heartbeat(heartbeat, now=10.0, stale_after=3.5)

    def test_release_rechecks_four_gib_after_watchdog_ready(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            ready = root / "ready.json"
            heartbeat = root / "heartbeat.json"
            release = root / "release"
            ready.write_text(json.dumps({"available_kib": capture_safety.START_MIN_KIB,
                                         "monotonic": 100.0}))
            heartbeat.write_text(json.dumps({"monotonic": 100.0}))
            with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "4-GiB release rail"):
                capture_safety.release_after_watchdog_ready(
                    ready_file=ready, heartbeat_file=heartbeat, release_file=release,
                    watchdog=mock.Mock(poll=lambda: None),
                    memory_reader=lambda: capture_safety.START_MIN_KIB - 1,
                    monotonic=lambda: 100.0,
                )
            self.assertFalse(release.exists())

    def test_watchdog_ready_follows_first_valid_sample(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "armed").touch()
            stopped = root / "stop"
            stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "100"}
            with mock.patch.object(capture_safety, "cgroup_memory_snapshot", return_value=stable):
                result = capture_safety.watchdog_loop(
                    owner_pid=123, owner_start=4, scope_name="bead-capture-012345abcdef.scope",
                    stop_file=stopped, armed_file=root / "armed", ready_file=root / "ready.json",
                    heartbeat_file=root / "heartbeat.json", events_file=root / "events.jsonl",
                    control_group="/private/scope", initial_oom={"oom": 0, "oom_kill": 0},
                    memory_reader=lambda: capture_safety.START_MIN_KIB,
                    sleeper=lambda _: stopped.touch(), monotonic=lambda: 100.0,
                    run=lambda *_args, **_kwargs: mock.Mock(returncode=0),
                    owner_alive=lambda: True,
                )
            self.assertEqual(result, 0)
            self.assertEqual(json.loads((root / "ready.json").read_text())["available_kib"], capture_safety.START_MIN_KIB)
            self.assertEqual(json.loads((root / "heartbeat.json").read_text())["monotonic"], 100.0)

    def test_oom_counters_must_be_present_and_unchanged(self):
        initial = {"memory.events": "low 0\noom 2\noom_kill 1\n"}
        self.assertEqual(capture_safety.oom_counter_delta(
            initial, {"memory.events": "oom 2\noom_kill 1\n"}), {"oom": 0, "oom_kill": 0})
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "counter increased"):
            capture_safety.oom_counter_delta(initial, {"memory.events": "oom 3\noom_kill 1\n"})
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "unavailable"):
            capture_safety.oom_counter_delta(initial, {"memory.events": "unavailable: removed"})

    def test_stale_heartbeat_invalidates_and_stops_owned_scope(self):
        stable_events = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096"}
        ticks = iter([100.0, 100.0, 100.0, 100.0, 100.0, 104.0])
        with tempfile.TemporaryDirectory() as temporary:
            _, error, commands, report = self._run_mock_guard(
                temporary, [stable_events], lambda: next(ticks), [None, None, 0])
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("heartbeat is stale", str(error))
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", "bead-capture-012345abcdef.scope"])
        self.assertEqual(report["status"], "aborted")

    def test_oom_delta_invalidates_even_zero_exit_harness(self):
        baseline = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096"}
        oom = {"memory.events": "low 0\noom 1\noom_kill 0\n", "memory.peak": "4096"}
        with tempfile.TemporaryDirectory() as temporary:
            result, error, commands, report = self._run_mock_guard(
                temporary, [baseline, oom], lambda: 100.0)
        self.assertIsNone(result)
        self.assertIsInstance(error, capture_safety.CaptureSafetyError)
        self.assertIn("counter increased", str(error))
        self.assertEqual(report["oom_initial"], {"oom": 0, "oom_kill": 0})
        self.assertEqual(report["oom_final_or_last_available"], {"oom": 1, "oom_kill": 0})
        self.assertEqual(report["oom_delta"], {"oom": 1, "oom_kill": 0})
        self.assertEqual(commands[-1], ["systemctl", "--user", "stop", "bead-capture-012345abcdef.scope"])

    def test_guard_verifies_scope_before_releasing_child_and_records_report(self):
        class FakeProcess:
            def __init__(self, returncode=None):
                self.returncode = returncode
                self.poll_count = 0
                self.finish_release_file = None

            def poll(self):
                self.poll_count += 1
                if (self.returncode is None and self.finish_release_file is not None
                        and self.finish_release_file.exists()):
                    self.returncode = 0
                return self.returncode

            def wait(self, timeout=None):
                self.returncode = 0
                return 0

        with tempfile.TemporaryDirectory() as temporary:
            scope = FakeProcess()

            class FakeWatchdog:
                def poll(self):
                    return None

                def wait(self, timeout=None):
                    return 0

            watcher = FakeWatchdog()
            popen_calls = []

            def fake_popen(command, **kwargs):
                popen_calls.append(command)
                if command[0] == "systemd-run":
                    ready_path = pathlib.Path(command[command.index("--ready-file") + 1])
                    ready_path.write_text(json.dumps({"pid": 321, "start_ticks": 456, "cgroup": "/scope"}))
                    pathlib.Path(command[command.index("--finished-file") + 1]).write_text(
                        json.dumps({"returncode": 0}))
                    scope.finish_release_file = pathlib.Path(
                        command[command.index("--finish-release-file") + 1])
                    return scope
                watchdog_ready = pathlib.Path(command[command.index("--ready-file") + 1])
                heartbeat = pathlib.Path(command[command.index("--heartbeat-file") + 1])
                watchdog_ready.write_text(json.dumps({"available_kib": capture_safety.START_MIN_KIB,
                                                      "monotonic": 100.0}))
                heartbeat.write_text(json.dumps({"monotonic": 100.0}))
                return watcher

            def fake_run(command, **kwargs):
                prop = next((part.split("=", 1)[1] for part in command if part.startswith("--property=")), "")
                return mock.Mock(returncode=0, stdout={
                    "MemoryMax": str(capture_safety.SCOPE_MEMORY_MAX_MIN),
                    "MemorySwapMax": "0", "ControlGroup": "/scope"}.get(prop, ""))

            stable_events = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096",
                             "memory.max": str(capture_safety.SCOPE_MEMORY_MAX_MIN), "memory.swap.max": "0"}
            with (mock.patch.object(capture_safety, "cgroup_memory_snapshot", return_value=stable_events),
                  mock.patch.object(capture_safety, "process_start_ticks", return_value=456),
                  mock.patch.object(capture_safety, "process_cgroup", return_value="/scope"),
                  mock.patch.object(capture_safety, "read_cgroup_processes", return_value={321})):
                result = capture_safety.run_guarded(
                    ["python3", "measure.py"], report_path=pathlib.Path(temporary) / "safety.json",
                    env={}, memory_reader=lambda: capture_safety.START_MIN_KIB,
                    popen=fake_popen, run=fake_run, sleeper=lambda _: None,
                    monotonic=lambda: 100.0, unit_id="012345abcdef",
                )
            self.assertEqual(result, 0)
            self.assertIn("--property=MemoryMax=12884901888", popen_calls[0])
            self.assertIn("--property=MemorySwapMax=0", popen_calls[0])
            report = json.loads((pathlib.Path(temporary) / "safety.json").read_text())
            self.assertEqual(report["status"], "complete")
            self.assertEqual(report["effective_memory_max"], str(capture_safety.SCOPE_MEMORY_MAX_MIN))
            self.assertEqual(report["effective_memory_swap_max"], "0")
            self.assertEqual(report["child_cgroup"], "/scope")
            self.assertEqual(report["oom_delta"], {"oom": 0, "oom_kill": 0})
            self.assertTrue(report["final_cgroup_read_while_scope_held"])

    def test_interrupt_stops_the_scope_before_disarming_watchdog(self):
        class RunningProcess:
            returncode = None

            def poll(self):
                return self.returncode

            def wait(self, timeout=None):
                self.returncode = 0
                return 0

        with tempfile.TemporaryDirectory() as temporary:
            scope, watcher = RunningProcess(), RunningProcess()
            commands = []

            def fake_popen(command, **kwargs):
                if command[0] == "systemd-run":
                    ready_path = pathlib.Path(command[command.index("--ready-file") + 1])
                    ready_path.write_text(json.dumps({"pid": 321, "start_ticks": 456, "cgroup": "/scope"}))
                    return scope
                pathlib.Path(command[command.index("--ready-file") + 1]).write_text(
                    json.dumps({"available_kib": capture_safety.START_MIN_KIB, "monotonic": 100.0}))
                pathlib.Path(command[command.index("--heartbeat-file") + 1]).write_text(
                    json.dumps({"monotonic": 100.0}))
                return watcher

            def fake_run(command, **kwargs):
                commands.append(command)
                prop = next((part.split("=", 1)[1] for part in command if part.startswith("--property=")), "")
                return mock.Mock(returncode=0, stdout={
                    "MemoryMax": str(capture_safety.SCOPE_MEMORY_MAX_MIN),
                    "MemorySwapMax": "0", "ControlGroup": "/scope"}.get(prop, ""))

            stable_events = {"memory.events": "low 0\noom 0\noom_kill 0\n", "memory.peak": "4096",
                             "memory.max": str(capture_safety.SCOPE_MEMORY_MAX_MIN), "memory.swap.max": "0"}
            with (mock.patch.object(capture_safety, "cgroup_memory_snapshot", return_value=stable_events),
                  mock.patch.object(capture_safety, "process_start_ticks", return_value=456),
                  mock.patch.object(capture_safety, "process_cgroup", return_value="/scope"),
                  mock.patch.object(capture_safety, "read_cgroup_processes", return_value={321})):
                with self.assertRaises(KeyboardInterrupt):
                    capture_safety.run_guarded(
                        ["python3", "measure.py"], report_path=pathlib.Path(temporary) / "safety.json",
                        env={}, memory_reader=lambda: capture_safety.START_MIN_KIB,
                        popen=fake_popen, run=fake_run,
                        sleeper=lambda _: (_ for _ in ()).throw(KeyboardInterrupt()),
                        monotonic=lambda: 100.0, unit_id="012345abcdef",
                    )
            self.assertEqual(commands[-1], ["systemctl", "--user", "stop", "bead-capture-012345abcdef.scope"])
            self.assertTrue(all(command[3] == "bead-capture-012345abcdef.scope" for command in commands))
            report = json.loads((pathlib.Path(temporary) / "safety.json").read_text())
            self.assertEqual(report["status"], "aborted")

    def test_low_memory_aborts_only_the_recorded_owned_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            stop_file = pathlib.Path(temporary) / "stop"
            armed_file = pathlib.Path(temporary) / "armed"
            events_file = pathlib.Path(temporary) / "events.jsonl"
            armed_file.touch()
            stopped = []
            stable = {"memory.events": "oom 0\noom_kill 0\n", "memory.peak": "100"}
            with mock.patch.object(capture_safety, "cgroup_memory_snapshot", return_value=stable):
                result = capture_safety.watchdog_loop(
                    owner_pid=123, owner_start=4, scope_name="bead-capture-012345abcdef.scope",
                    stop_file=stop_file, armed_file=armed_file, events_file=events_file,
                    ready_file=pathlib.Path(temporary) / "ready.json",
                    heartbeat_file=pathlib.Path(temporary) / "heartbeat.json",
                    control_group="/private/scope", initial_oom={"oom": 0, "oom_kill": 0},
                    memory_reader=lambda: capture_safety.ABORT_BELOW_KIB - 1,
                    sleeper=lambda _: self.fail("watchdog should stop at low memory"),
                    run=lambda command, **_: (stopped.append(command) or mock.Mock(returncode=0)),
                    owner_alive=lambda: True,
                )
            self.assertEqual(result, 3)
            self.assertEqual(stopped, [["systemctl", "--user", "stop", "bead-capture-012345abcdef.scope"]])
            events = [json.loads(line) for line in events_file.read_text().splitlines()]
            self.assertEqual(events[-1]["reason"], "MemAvailable-below-2GiB")

    def test_monitor_failure_aborts_and_owned_scope_cleanup_is_narrow(self):
        with tempfile.TemporaryDirectory() as temporary:
            armed_file = pathlib.Path(temporary) / "armed"
            armed_file.touch()
            stopped = []
            result = capture_safety.watchdog_loop(
                owner_pid=123, owner_start=4, scope_name="bead-capture-fedcba987654.scope",
                stop_file=pathlib.Path(temporary) / "stop", armed_file=armed_file,
                ready_file=pathlib.Path(temporary) / "ready.json",
                heartbeat_file=pathlib.Path(temporary) / "heartbeat.json",
                events_file=pathlib.Path(temporary) / "events.jsonl", control_group="/private/scope",
                initial_oom={"oom": 0, "oom_kill": 0},
                memory_reader=lambda: (_ for _ in ()).throw(OSError("meminfo disappeared")),
                sleeper=lambda _: self.fail("watchdog should stop after read failure"),
                run=lambda command, **_: (stopped.append(command) or mock.Mock(returncode=0)),
                owner_alive=lambda: True,
            )
            self.assertEqual(result, 4)
            self.assertEqual(stopped, [["systemctl", "--user", "stop", "bead-capture-fedcba987654.scope"]])
            with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "not created by this task"):
                capture_safety.stop_owned_scope("user.slice", lambda *_args, **_kwargs: self.fail("must not stop another unit"))

    def test_zero_or_malformed_watchdog_samples_stop_scope(self):
        for value in (0, "bad"):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as temporary:
                root = pathlib.Path(temporary)
                (root / "armed").touch()
                stopped = []
                result = capture_safety.watchdog_loop(
                    owner_pid=123, owner_start=4, scope_name="bead-capture-012345abcdef.scope",
                    stop_file=root / "stop", armed_file=root / "armed", ready_file=root / "ready",
                    heartbeat_file=root / "heartbeat", events_file=root / "events.jsonl",
                    control_group="/private/scope", initial_oom={"oom": 0, "oom_kill": 0},
                    memory_reader=lambda: value, sleeper=lambda _: self.fail("invalid sample must abort"),
                    run=lambda command, **_: (stopped.append(command) or mock.Mock(returncode=0)),
                    owner_alive=lambda: True,
                )
                self.assertEqual(result, 4)
                self.assertEqual(stopped, [["systemctl", "--user", "stop", "bead-capture-012345abcdef.scope"]])
                self.assertFalse((root / "ready").exists())

    def test_missing_wayland_info_is_a_prerequisite_not_a_probe_substitution(self):
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "wayland-info"):
            capture_safety.require_wayland_info(lambda _: None)


if __name__ == "__main__":
    unittest.main()
