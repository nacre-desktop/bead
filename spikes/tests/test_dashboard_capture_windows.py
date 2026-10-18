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
launcher = load_module("launcher_proof", ROOT / "launcher-proof.py")
capture_safety = load_module("capture_safety", ROOT / "capture_safety.py")


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
        row, timing = residency.acceptance_baseline(22, clock.idle, 600, clock.monotonic, clock.sleep)
        self.assertEqual(clock.events, [("sleep", 600), ("idle", 22, 60)])
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


class CaptureSafetyTests(unittest.TestCase):
    def test_memavailable_parser_requires_kernel_field(self):
        self.assertEqual(capture_safety.parse_memavailable_kib("MemTotal: 9000 kB\nMemAvailable: 4194304 kB\n"), 4194304)
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "missing or malformed"):
            capture_safety.parse_memavailable_kib("MemTotal: 9000 kB\n")

    def test_scope_is_exact_limit_and_contains_harness(self):
        capture_safety.validate_scope(
            "bead-capture-012345abcdef.scope", "2147483648",
            "/user.slice/user-1000.slice/bead-capture-012345abcdef.scope",
            "/user.slice/user-1000.slice/bead-capture-012345abcdef.scope",
        )
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "MemoryMax"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope", "0", "/scope", "/scope")
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "outside"):
            capture_safety.validate_scope("bead-capture-012345abcdef.scope", "2147483648", "/scope/a", "/scope-b")
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "unrecognized"):
            capture_safety.validate_scope("other.scope", "2147483648", "/scope", "/scope")

    def test_scope_command_sets_the_fixed_memory_cap(self):
        command = capture_safety.scope_command(
            "bead-capture-012345abcdef.scope", ["python3", "measure.py"])
        self.assertIn("--property=MemoryMax=2147483648", command)
        self.assertIn("--scope", command)
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

    def test_guard_verifies_scope_before_releasing_child_and_records_report(self):
        class FakeProcess:
            def __init__(self, returncode=None):
                self.returncode = returncode
                self.poll_count = 0

            def poll(self):
                self.poll_count += 1
                if self.returncode is None and self.poll_count > 1:
                    self.returncode = 0
                return self.returncode

            def wait(self, timeout=None):
                self.returncode = 0
                return 0

        with tempfile.TemporaryDirectory() as temporary:
            scope = FakeProcess()
            watcher = FakeProcess()
            popen_calls = []

            def fake_popen(command, **kwargs):
                popen_calls.append(command)
                if command[0] == "systemd-run":
                    ready_path = pathlib.Path(command[command.index("--capture-guard-ready-file") + 1])
                    ready_path.write_text(json.dumps({"pid": 321, "cgroup": "/scope"}))
                    return scope
                return watcher

            def fake_run(command, **kwargs):
                prop = next((part.split("=", 1)[1] for part in command if part.startswith("--property=")), "")
                return mock.Mock(returncode=0, stdout={"MemoryMax": "2147483648", "ControlGroup": "/scope"}.get(prop, ""))

            result = capture_safety.run_guarded(
                ["python3", "measure.py"], report_path=pathlib.Path(temporary) / "safety.json",
                env={}, memory_reader=lambda: capture_safety.START_MIN_KIB,
                popen=fake_popen, run=fake_run, sleeper=lambda _: None,
                monotonic=iter([0, 0, 0, 0, 0, 0, 0, 0]).__next__, unit_id="012345abcdef",
            )
            self.assertEqual(result, 0)
            self.assertIn("--property=MemoryMax=2147483648", popen_calls[0])
            report = json.loads((pathlib.Path(temporary) / "safety.json").read_text())
            self.assertEqual(report["status"], "complete")
            self.assertEqual(report["effective_memory_max"], "2147483648")
            self.assertEqual(report["child_cgroup"], "/scope")

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
                    ready_path = pathlib.Path(command[command.index("--capture-guard-ready-file") + 1])
                    ready_path.write_text(json.dumps({"pid": 321, "cgroup": "/scope"}))
                    return scope
                return watcher

            def fake_run(command, **kwargs):
                commands.append(command)
                prop = next((part.split("=", 1)[1] for part in command if part.startswith("--property=")), "")
                return mock.Mock(returncode=0, stdout={"MemoryMax": "2147483648", "ControlGroup": "/scope"}.get(prop, ""))

            with self.assertRaises(KeyboardInterrupt):
                capture_safety.run_guarded(
                    ["python3", "measure.py"], report_path=pathlib.Path(temporary) / "safety.json",
                    env={}, memory_reader=lambda: capture_safety.START_MIN_KIB,
                    popen=fake_popen, run=fake_run,
                    sleeper=lambda _: (_ for _ in ()).throw(KeyboardInterrupt()),
                    monotonic=iter([0, 0, 0, 0, 0]).__next__, unit_id="012345abcdef",
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
            result = capture_safety.watchdog_loop(
                owner_pid=123, owner_start=4, scope_name="bead-capture-012345abcdef.scope",
                stop_file=stop_file, armed_file=armed_file, events_file=events_file,
                control_group="/private/scope", memory_reader=lambda: capture_safety.ABORT_BELOW_KIB - 1,
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
                events_file=pathlib.Path(temporary) / "events.jsonl", control_group="/private/scope",
                memory_reader=lambda: (_ for _ in ()).throw(OSError("meminfo disappeared")),
                sleeper=lambda _: self.fail("watchdog should stop after read failure"),
                run=lambda command, **_: (stopped.append(command) or mock.Mock(returncode=0)),
                owner_alive=lambda: True,
            )
            self.assertEqual(result, 4)
            self.assertEqual(stopped, [["systemctl", "--user", "stop", "bead-capture-fedcba987654.scope"]])
            with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "not created by this task"):
                capture_safety.stop_owned_scope("user.slice", lambda *_args, **_kwargs: self.fail("must not stop another unit"))

    def test_missing_wayland_info_is_a_prerequisite_not_a_probe_substitution(self):
        with self.assertRaisesRegex(capture_safety.CaptureSafetyError, "wayland-info"):
            capture_safety.require_wayland_info(lambda _: None)


if __name__ == "__main__":
    unittest.main()
