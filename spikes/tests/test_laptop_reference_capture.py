import importlib.util
import json
import pathlib
import shutil
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
                "candidate": {"path": "candidate", "sha256": capture.sha256(candidate)},
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
                    residency = {"bar": row, "closed": [row, row], "valid": True,
                                 "batch_medians": True, "opened_once_only": False}
                    for file_phase, phase in capture.PHASES.items():
                        residency[phase] = {"memory": row["memory"], "drm_mib": row["drm_mib"]}
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
