import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "b0002-arm.py"
SPEC = importlib.util.spec_from_file_location("b0002_arm", SCRIPT)
arm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(arm)


class B0002ArmLauncherTests(unittest.TestCase):
    def test_side_configuration_is_explicit_and_hash_pinned(self):
        base = {
            "PATH": os.environ["PATH"], "HOME": "/tmp/private-home", "USER": "tester",
            "XDG_RUNTIME_DIR": "/tmp/runtime", "WAYLAND_DISPLAY": "wayland-test",
            "HYPRLAND_INSTANCE_SIGNATURE": "nested-test", "NIRI_SOCKET": "/bad/live/socket",
            "BEAD_MENU_ENGINE_BINARY": "/wrong/helper", "BEAD_DASHBOARD_OPENED_ONCE_ONLY": "1",
            "NACRE_DAEMON_SOCKET": "/wrong/socket", "API_TOKEN": "must-not-record",
        }
        for side, artifact in (("A", "control"), ("B", "fixed")):
            env = arm.inherited_capture_environment(base, side, "hyprland", pathlib.Path("/tmp/raw"))
            self.assertEqual(env["BEAD_HARDENING_BINARY"], str(arm.CONTROL if side == "A" else arm.FIXED))
            selected = arm.CONTROL if side == "A" else arm.FIXED
            self.assertEqual(arm.sha256(selected), arm.EXPECTED_HASHES[artifact])
            self.assertEqual(env["BEAD_MENU_ENGINE_BINARY"], str(arm.HELPER))
            self.assertNotIn("BEAD_BAR_MATRIX", env)
            self.assertEqual(env["BEAD_DASHBOARD_BATCH_MEDIANS"], "1")
            self.assertEqual(env["BEAD_DASHBOARD_WARMUP_SECONDS"], "600")
            self.assertNotIn("BEAD_DASHBOARD_OPENED_ONCE_ONLY", env)
            self.assertNotIn("NIRI_SOCKET", env)
            self.assertEqual(arm.artifact_hashes()["control"]["sha256"], arm.EXPECTED_HASHES["control"])
            self.assertEqual(arm.artifact_hashes()["fixed"]["sha256"], arm.EXPECTED_HASHES["fixed"])
            self.assertEqual(arm.artifact_hashes()["helper"]["sha256"], arm.EXPECTED_HASHES["helper"])
        niri_env = arm.inherited_capture_environment(base, "A", "niri", pathlib.Path("/tmp/raw"))
        self.assertEqual(niri_env["BEAD_BAR_MATRIX"], "1")

    def test_order_and_compositor_arguments_are_fixed(self):
        self.assertEqual(arm.previous_for("A1"), "START")
        self.assertEqual(arm.previous_for("B1"), "A1")
        self.assertEqual(arm.previous_for("A2"), "B1")
        with self.assertRaisesRegex(RuntimeError, "schedule violation"):
            arm.verify_previous(pathlib.Path("/tmp/no-data"), "A2", "A1")
        hypr = arm.capture_argv("hyprland")
        niri = arm.capture_argv("niri")
        self.assertEqual(hypr[-1], "--virtual120")
        self.assertNotIn("--virtual120", niri)
        self.assertEqual(hypr[-6:], ["--compositor", "hyprland", "--bar-ui", "--modes", "bead", "--runs", "1", "--virtual120"][-6:])

    def test_invocation_retry_rejects_baseline_and_mid_phase_samples(self):
        with tempfile.TemporaryDirectory(prefix="b0002-retry-test-") as temp:
            root = pathlib.Path(temp)
            (root / "launcher-failure.json").write_text(json.dumps({"stage": "capture", "sampling_started": False}))
            (root / "hyprland-capture-safety.json").write_text(json.dumps({
                "status": "complete", "capture_returncode": 1}))
            self.assertTrue(arm.invocation_retry_allowed(root, "hyprland"))
            measured = root / "hyprland"
            measured.mkdir()
            (measured / "dashboard-residency-baseline.json").write_text("{}")
            self.assertFalse(arm.invocation_retry_allowed(root, "hyprland"))
            (measured / "dashboard-residency-baseline.json").unlink()
            (measured / "opened-once-close-0.json").write_text("{}")
            self.assertFalse(arm.invocation_retry_allowed(root, "hyprland"))

    def test_product_result_cannot_use_environmental_replacement(self):
        with tempfile.TemporaryDirectory(prefix="b0002-product-retry-test-") as temp:
            root = pathlib.Path(temp)
            (root / "launcher-failure.json").write_text(json.dumps({"stage": "capture", "sampling_started": True}))
            (root / "hyprland-capture-safety.json").write_text(json.dumps({
                "status": "complete", "capture_returncode": 0, "oom_delta": {"oom": 0, "oom_kill": 0}}))
            measured = root / "hyprland"
            measured.mkdir()
            (measured / "dashboard-residency-sampling-start.json").write_text("{}")
            (measured / "dashboard-residency.json").write_text(json.dumps({"valid": False}))
            self.assertFalse(arm.environmental_replacement_allowed(root, "hyprland", "scope-setup"))
            (root / "host-swapin-delta.txt").write_text("host_swapin_delta_bytes=0\n")
            self.assertFalse(arm.environmental_replacement_allowed(root, "hyprland", "host-swap-in"))

    def test_only_verified_host_swap_in_can_authorize_replacement(self):
        with tempfile.TemporaryDirectory(prefix="b0002-swap-retry-test-") as temp:
            root = pathlib.Path(temp)
            (root / "launcher-failure.json").write_text(json.dumps({"sampling_started": True}))
            (root / "hyprland-capture-safety.json").write_text(json.dumps({
                "status": "complete", "capture_returncode": 0, "oom_delta": {"oom": 0, "oom_kill": 0}}))
            (root / "host-swapin-delta.txt").write_text("host_swapin_delta_bytes=1048576\n")
            self.assertTrue(arm.environmental_replacement_allowed(root, "hyprland", "host-swap-in"))

    def test_valid_invocation_retry_prevents_later_replacement(self):
        with tempfile.TemporaryDirectory(prefix="b0002-valid-retry-test-") as temp:
            root = pathlib.Path(temp)
            comp_dir = root / "hyprland"
            first = comp_dir / "A1"
            retry = comp_dir / "A1-invocation-retry1"
            first.mkdir(parents=True)
            retry.mkdir()
            (first / "launcher-failure.json").write_text(json.dumps({"sampling_started": False}))
            (retry / "hyprland-capture-safety.json").write_text(json.dumps({
                "status": "complete", "capture_returncode": 0, "scope_runner_returncode": 0}))
            residency = {key: {"seconds": 60} for key in arm.PHASES}
            residency.update({"valid": True, "batch_medians": True, "opened_once_only": False})
            measured = retry / "hyprland"
            measured.mkdir()
            (measured / "dashboard-residency.json").write_text(json.dumps(residency))
            self.assertEqual(arm.latest_failed_attempt(comp_dir, "A1"), retry)
            self.assertFalse(arm.environmental_replacement_allowed(retry, "hyprland", "scope-setup"))

    def test_dry_run_records_resolved_environment_argv_and_hashes_without_sampling(self):
        with tempfile.TemporaryDirectory(prefix="b0002-launcher-test-") as temp:
            root = pathlib.Path(temp)
            env = {
                "PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "USER": "tester",
                "XDG_RUNTIME_DIR": str(root / "runtime"), "WAYLAND_DISPLAY": "wayland-test",
                "GTK_THEME": "test-theme",
                "HYPRLAND_INSTANCE_SIGNATURE": "nested-test", "BEAD_MENU_ENGINE_BINARY": "/wrong/helper",
                "API_TOKEN": "not-for-evidence",
            }
            completed = subprocess.run(
                [sys.executable, str(SCRIPT), "--side", "A", "--compositor", "hyprland",
                 "--label", "A1", "--previous-label", "START", "--raw-root", str(root / "raw"), "--dry-run"],
                env=env, capture_output=True, text=True, check=False)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            evidence = root / "raw/hyprland/A1"
            manifest = json.loads((evidence / "capture-launch.json").read_text())
            self.assertTrue(manifest["dry_run"])
            self.assertTrue(manifest["previous_label_verified"])
            self.assertEqual(manifest["environment"]["BEAD_MENU_ENGINE_BINARY"], str(arm.HELPER))
            self.assertNotIn("API_TOKEN", manifest["environment"])
            self.assertEqual(manifest["environment"]["GTK_THEME"], "test-theme")
            self.assertNotIn("BEAD_BAR_MATRIX", manifest["environment"])
            self.assertEqual(manifest["argv"][-1], "--virtual120")
            self.assertEqual(json.loads((evidence / "artifact-hashes.json").read_text())["helper"]["sha256"],
                             arm.EXPECTED_HASHES["helper"])
            self.assertFalse((evidence / "hyprland-capture-safety.json").exists())


if __name__ == "__main__":
    unittest.main()
