import importlib.util
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


capture = load("laptop_reference_capture", ROOT / "spikes/laptop-reference-capture.py")
measure = load("measure_for_laptop_capture", ROOT / "spikes/measure.py")
arm = load("b0002_arm_for_laptop_capture", ROOT / "spikes/b0002-arm.py")
launcher_proof = load("launcher_proof_for_laptop_capture", ROOT / "spikes/launcher-proof.py")


def sample(total, anon=5.0, drm=None):
    drm = None if drm is False else ({"i915@0000:00:02.0/system": total - 20.0} if drm is None else drm)
    return {"memory": {"pss_mib": 20.0, "swap_pss_mib": 0.0,
                        "pss_anon_mib": anon, "pss_file_mib": 14.0,
                        "pss_shmem_mib": 1.0},
            "drm_mib": drm, "pss_plus_drm_mib": total, "seconds": 60.0}


class LaptopReferenceCaptureTests(unittest.TestCase):
    def test_fixture_lane_uses_empty_external_observer_and_engine_lane_requires_one(self):
        fixture_observer = launcher_proof.default_external_observer(None, None)
        self.assertEqual(fixture_observer(), {
            "processes": [], "pss_plus_drm_swap_mib": 0.0,
            "source": "UI-only menu fixture; no external process"})
        with self.assertRaisesRegex(RuntimeError, "requires an external-process observer"):
            launcher_proof.default_external_observer(None, "/run/menu-engine.sock")

    def test_manifest_pins_binaries_frozen_bundle_and_shared_fixtures(self):
        with tempfile.TemporaryDirectory(prefix="bead-reference-manifest-") as temp:
            root = pathlib.Path(temp)
            artifacts = root / "artifacts"
            artifacts.mkdir()
            control = artifacts / "control"
            candidate = artifacts / "candidate"
            control.write_bytes(b"frozen-control")
            candidate.write_bytes(b"main-candidate")
            bundle = artifacts / "control-bundle"
            (bundle / "shell/ui").mkdir(parents=True)
            (bundle / "shell/tests").mkdir()
            (bundle / "shell/palette.json").write_text("{}\n")
            (bundle / "shell/ui/main.luau").write_text("return {}\n")
            (bundle / "shell/tests/icon.svg").write_text("<svg/>\n")
            fixture_root = ROOT / "shell/tests/fixtures/dashboard"
            shutil.copytree(fixture_root, bundle / "shell/tests/fixtures/dashboard")
            expected_control = capture.sha256(control)
            value = {
                "schema_version": 1,
                "control": {"path": "control", "sha256": expected_control,
                            "exact_original_binary_identity": True},
                "candidate": {"path": "candidate", "sha256": capture.sha256(candidate),
                              "shell_git_tree": subprocess.check_output(
                                  ["git", "-C", str(ROOT), "rev-parse", "HEAD:shell"], text=True).strip()},
                "control_bundle_root": "control-bundle",
                "control_bundle_sha256": capture.canonical_tree_hash(bundle),
                "dashboard_fixtures_sha256": capture.canonical_tree_hash(fixture_root),
                "dashboard_fixtures_git_tree": capture.FROZEN_FIXTURE_GIT_TREE,
            }
            manifest = artifacts / "manifest.json"
            manifest.write_text(json.dumps(value))
            with mock.patch.object(capture, "FROZEN_CONTROL_SHA256", expected_control):
                loaded = capture.load_manifest(manifest)
            self.assertEqual(loaded["control_bundle_root"], str(bundle.resolve()))
            self.assertEqual(loaded["dashboard_fixtures"], str(fixture_root.resolve()))

    def test_candidate_built_from_a_different_shell_tree_is_refused(self):
        with self.assertRaisesRegex(RuntimeError, "rebuild the candidate"):
            capture.check_candidate_shell_tree({"source_revision": "old", "shell_git_tree": "0" * 40})
        with self.assertRaisesRegex(RuntimeError, "rebuild the candidate"):
            capture.check_candidate_shell_tree({"source_revision": "old"})

    def test_ui_error_aborts_the_bar_wait_instead_of_timing_out(self):
        with self.assertRaisesRegex(RuntimeError, "UI_ERROR"):
            launcher_proof.fail_on_ui_error("READY\nUI_ERROR bar/main.luau:14:1: nil value\n")
        self.assertFalse(launcher_proof.fail_on_ui_error("READY output=1\n"))

    def test_control_and_candidate_use_their_matched_ui_bundle_roots(self):
        with tempfile.TemporaryDirectory(prefix="bead-reference-bundles-") as temp:
            control_bundle = pathlib.Path(temp) / "control-bundle"
            control_bundle.mkdir()
            control_binary = pathlib.Path(temp) / "control"
            candidate_binary = pathlib.Path(temp) / "candidate"
            helper = pathlib.Path(temp) / "unused-helper"
            with mock.patch.multiple(arm, CAPTURE_PROFILE="rehearsal", CONTROL_BUNDLE_ROOT=control_bundle,
                                    CONTROL=control_binary, FIXED=candidate_binary, HELPER=helper):
                control_env = arm.inherited_capture_environment(
                    {"PATH": "/usr/bin", "XDG_RUNTIME_DIR": "/run/user/1000"},
                    "A", "niri", pathlib.Path(temp) / "raw-a")
                candidate_env = arm.inherited_capture_environment(
                    {"PATH": "/usr/bin", "XDG_RUNTIME_DIR": "/run/user/1000"},
                    "B", "niri", pathlib.Path(temp) / "raw-b")
            self.assertEqual(control_env["BEAD_HARDENING_BUNDLE_ROOT"], str(control_bundle))
            self.assertEqual(control_env["BEAD_DASHBOARD_FIXTURES"],
                             str(control_bundle / "shell/tests/fixtures/dashboard"))
            self.assertEqual(candidate_env["BEAD_HARDENING_BUNDLE_ROOT"], str(ROOT))
            self.assertEqual(candidate_env["BEAD_DASHBOARD_FIXTURES"],
                             str(ROOT / "shell/tests/fixtures/dashboard"))

    def make_batch(self, root, drm=True):
        for compositor in ("niri",):
            for side in ("A", "B"):
                for pair in range(1, 4):
                    arm_root = root / compositor / f"{side}{pair}"
                    arm = root / compositor / f"{side}{pair}" / compositor
                    arm.mkdir(parents=True)
                    (arm_root / "host-swapin-delta.txt").write_text("host_swapin_delta_bytes=0\n")
                    (arm_root / f"{compositor}-capture-safety.json").write_text(json.dumps({
                        "status": "complete", "capture_returncode": 0,
                        "scope_runner_returncode": 0, "final_cgroup_read_while_scope_held": True,
                        "oom_delta": {"oom": 0, "oom_kill": 0},
                        "effective_memory_max": 12 * capture.GIB,
                        "effective_memory_swap_max": 0}))
                    (arm / "output-geometry.json").write_text(json.dumps({
                        "current_mode": 0, "modes": [{"width": 2880, "height": 1800,
                                                           "refresh_rate": 60000}],
                        "logical": {"width": 1920, "height": 1200, "scale": 1.5}}))
                    (arm / "session-isolation.json").write_text(json.dumps({
                        "runtime_is_task_private": True,
                        "runtime_has_no_group_or_other_access": True,
                        "wayland_socket_inside_private_runtime": True}))
                    value = 22.0 if side == "A" else 22.2
                    row = sample(value, drm={"i915@0000:00:02.0/system": value - 20.0} if drm else False)
                    backend = {"requested": "production", "main_bar": "cpu",
                               "opened_panel": "cpu", "gpu_libraries": [],
                               "evidence_source": "test measured PID evidence"}
                    row["backend"] = backend
                    residency = {"bar": row, "closed": [row, row], "warm": [{"backend": backend}], "valid": True,
                                 "batch_medians": True, "opened_once_only": False}
                    for file_phase, phase in capture.PHASES.items():
                        residency[phase] = {"memory": row["memory"], "drm_mib": row["drm_mib"],
                                            "backend": backend}
                        (arm / f"{file_phase}.json").write_text(json.dumps(row))
                    (arm / "dashboard-residency.json").write_text(json.dumps(residency))

    def test_three_alternating_pairs_report_absolute_and_relative_deltas(self):
        with tempfile.TemporaryDirectory(prefix="bead-laptop-summary-") as temp:
            root = pathlib.Path(temp)
            self.make_batch(root)
            report = capture.summarize(root, ["niri"], rehearsal=False)
            self.assertEqual(report["absolute_verdict"], "PASS")
            self.assertEqual(len(report["relative_deltas_mib"]), 4)
            phase = report["relative_deltas_mib"]["niri/never"]
            for delta in phase["pairwise_total_deltas_mib"]:
                self.assertAlmostEqual(delta, 0.2)
            self.assertTrue(all(item["pass"] for item in phase["close_checks"]))

    def test_unknown_drm_never_passes_reference_absolute_gate(self):
        with tempfile.TemporaryDirectory(prefix="bead-laptop-drm-unknown-") as temp:
            root = pathlib.Path(temp)
            self.make_batch(root, drm=False)
            report = capture.summarize(root, ["niri"], rehearsal=False)
            self.assertEqual(report["absolute_verdict"], "INCOMPLETE / STOP")
            self.assertIn("DRM fdinfo unavailable", report["failures"][0])

    def test_unknown_backend_invalidates_arm_and_absolute_verdict(self):
        with tempfile.TemporaryDirectory(prefix="bead-laptop-backend-unknown-") as temp:
            root = pathlib.Path(temp)
            self.make_batch(root)
            path = root / "niri" / "B1" / "niri" / "dashboard-residency.json"
            residency = json.loads(path.read_text())
            residency["warm"][0]["backend"]["opened_panel"] = "unknown"
            path.write_text(json.dumps(residency))
            report = capture.summarize(root, ["niri"], rehearsal=False)
            self.assertEqual(report["arm_validity"]["niri/B1"]["valid"], False)
            self.assertEqual(report["absolute_verdict"], "INCOMPLETE / STOP")

    def test_a1_closed_phase_rows_inactive_panel_is_valid_with_warm_cpu_rows(self):
        # Reproduces laptop run 3 A1: settled phase rows record opened_panel "inactive".
        with tempfile.TemporaryDirectory(prefix="bead-laptop-a1-") as temp:
            root = pathlib.Path(temp)
            self.make_batch(root)
            path = root / "niri" / "B1" / "niri" / "dashboard-residency.json"
            residency = json.loads(path.read_text())
            for phase in ("once", "several", "combined"):
                residency[phase]["backend"]["opened_panel"] = "inactive"
            for row in residency["warm"]:
                row["backend"]["opened_panel"] = "cpu (configured dashboard renderer)"
            path.write_text(json.dumps(residency))
            report = capture.summarize(root, ["niri"], rehearsal=False)
            self.assertTrue(report["arm_validity"]["niri/B1"]["valid"])
            residency["warm"] = []
            path.write_text(json.dumps(residency))
            report = capture.summarize(root, ["niri"], rehearsal=False)
            self.assertFalse(report["arm_validity"]["niri/B1"]["valid"])

    def test_control_without_markers_is_affirmed_cpu_from_argv_and_maps(self):
        argv = ["nacre-shell", "--bar", "--core-views", "--renderer", "auto"]
        cpu = launcher_proof.classify_renderer(argv, "7f00 r-xp /usr/lib/libc.so.6\n", "", "production", True)
        self.assertEqual((cpu["main_bar"], cpu["opened_panel"]), ("cpu", "cpu (configured dashboard renderer)"))
        closed = launcher_proof.classify_renderer(argv, "", "", "production", False)
        self.assertEqual(closed["opened_panel"], "inactive")

    def test_gl_libraries_without_marker_stay_unknown(self):
        argv = ["nacre-shell", "--bar", "--core-views", "--renderer", "auto"]
        maps = "7f00 r-xp 0 00:00 0 /usr/lib/libEGL.so.1\n"
        row = launcher_proof.classify_renderer(argv, maps, "", "production", True)
        self.assertEqual(row["opened_panel"], "unknown")
        self.assertIn("libEGL.so.1", row["gpu_libraries"])

    def test_runtime_estimate_covers_default_and_optional_cpu_batch(self):
        self.assertEqual(capture.estimate_runtime(["niri"], include_cpu=False), (45, 41.0))
        self.assertEqual(capture.estimate_runtime(["niri"], include_cpu=True), (90, 82.0))

    def test_process_without_drm_device_is_known_zero_not_unknown(self):
        import os
        with mock.patch.object(measure, "drm_device_fds", return_value=0):
            self.assertEqual(measure.drm_memory(os.getpid()), {})
        with mock.patch.object(measure, "drm_device_fds", return_value=1), \
                mock.patch.object(measure, "parse_drm_fdinfo", return_value=None):
            self.assertIsNone(measure.drm_memory(os.getpid()))

    def test_resident_and_legacy_fdinfo_are_deduplicated_and_normalized(self):
        fdinfo = [
            "drm-driver: i915\ndrm-pdev: 0000:00:02.0\ndrm-client-id: 7\n"
            "drm-memory-system: 8192 KiB\ndrm-resident-system: 10 MiB\n"
            "drm-total-system: 14 MiB\n",
            "drm-driver: i915\ndrm-pdev: 0000:00:02.0\ndrm-client-id: 7\n"
            "drm-total-system: 14 MiB\n",
            "drm-driver: i915\ndrm-pdev: 0000:00:02.0\ndrm-client-id: 8\n"
            "drm-resident-system: 3 MiB\n",
        ]
        self.assertEqual(measure.parse_drm_fdinfo(fdinfo), {"i915@0000:00:02.0/system": 17.0})


if __name__ == "__main__":
    unittest.main()
