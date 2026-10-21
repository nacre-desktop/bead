import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "b0003-arm.py"
SPEC = importlib.util.spec_from_file_location("b0003_arm", SCRIPT)
arm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(arm)
PROOF_SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "menu-engine-proof.py"
PROOF_SPEC = importlib.util.spec_from_file_location("menu_engine_proof", PROOF_SCRIPT)
proof = importlib.util.module_from_spec(PROOF_SPEC)
PROOF_SPEC.loader.exec_module(proof)


class B0003ArmTests(unittest.TestCase):
    def test_diagnostic_worker_clocks_do_not_leak_into_serial_typing(self):
        workers = {123: object(), 456: object()}
        proof.reset_worker_tracking(workers)
        self.assertEqual(workers, {})

    def test_idle_snapshot_excludes_zombies_and_records_missing_workers(self):
        states = {3: "R", 4: "Z"}

        def read_state(pid):
            if pid not in states:
                raise ProcessLookupError(pid, "exited")
            return states[pid]

        active, exited = proof.active_process_ids(1, 2, {3, 4, 5}, read_state)
        self.assertEqual(active, {1, 2, 3})
        self.assertEqual(exited, {4, 5})

    def test_idle_memory_snapshot_retries_a_worker_that_exits_mid_read(self):
        process_sets = iter(({1, 2, 3}, {1, 2}, {1, 2}))

        def read_memory(pid):
            if pid == 3:
                raise ProcessLookupError(pid, "exited")
            return {"swap_pss_mib": 0, "pss_mib": float(pid)}

        snapshot = proof.stable_process_memory_snapshot(lambda: next(process_sets), read_memory)
        self.assertEqual(snapshot["pids"], [1, 2])
        self.assertEqual(snapshot["observation_retries"], 1)
        self.assertNotIn(3, snapshot["memory"])

    def test_idle_memory_snapshot_retries_a_changed_process_set(self):
        process_sets = iter(({1, 2}, {1}, {1}, {1}))
        snapshot = proof.stable_process_memory_snapshot(
            lambda: next(process_sets), lambda pid: {"pss_mib": float(pid)})
        self.assertEqual(snapshot["pids"], [1])
        self.assertEqual(snapshot["observation_retries"], 1)

    def test_desktop_swap_capacity_and_pointer_target_path(self):
        self.assertEqual(arm.SWAP_SIZE_BYTES, 15_569_252_352)
        self.assertEqual(arm.pointer_target_dir(pathlib.Path('/build/target/release/examples/private-pointer')),
                         pathlib.Path('/build/target'))

    def test_diagnostic_requires_stage_profile(self):
        self.assertEqual(arm.profile_for('diagnostic', '1'), '1')
        with self.assertRaisesRegex(RuntimeError, 'profile 1'):
            arm.profile_for('diagnostic', 'clocks')

    def test_w_idle_formats_include_day_units(self):
        self.assertEqual(arm.idle_seconds('5days'), 5 * 86400)
        self.assertEqual(arm.idle_seconds('1day'), 86400)
        self.assertEqual(arm.idle_seconds('0.26s'), 0.26)

    def test_compositor_geometry_profiles(self):
        self.assertTrue(arm.geometry_valid('hyprland', {
            'width': 2880, 'height': 1800, 'scale': 1.5, 'refreshRate': 120.0}))
        self.assertTrue(arm.geometry_valid('niri', {
            'modes': [{'width': 2880, 'height': 1800, 'refresh_rate': 60000}],
            'current_mode': 0, 'logical': {'width': 1920, 'height': 1200, 'scale': 1.5}}))
        self.assertFalse(arm.geometry_valid('niri', {'modes': [], 'logical': {}}))

    def test_trace_validator_ignores_unrelated_presentations(self):
        with tempfile.TemporaryDirectory() as tmp:
            trace = pathlib.Path(tmp) / "engine-typing.log"
            lines = [
                "MENU_RESULT ok=true catalog_rev=8 count=160",
                "UI_COMMIT generation=1 update=1 source_error=false",
                "INPUT_PROFILE stage=markup_eval wall_ms=1 cpu_ms=0.8",
            ]
            lines.extend(f"INPUT_RECEIVED monotonic_ns={i + 1} cpu_ns={i + 1}" for i in range(200))
            lines.extend(f"UI_PRESENTED generation={i} update={i} cpu_ns={i}" for i in range(203))
            trace.write_text("\n".join(lines) + "\n")
            valid, _ = arm.trace_valid(trace, "1")
            self.assertTrue(valid)
            missing_input = next(line for line in lines if line.startswith("INPUT_RECEIVED "))
            lines.remove(missing_input)
            trace.write_text("\n".join(lines) + "\n")
            valid, reason = arm.trace_valid(trace, "1")
            self.assertFalse(valid)
            self.assertIn("received keys", reason)

    def test_invocation_and_profile_differ_only_by_profile_flag(self):
        hypr = arm.capture_argv("hyprland")
        niri = arm.capture_argv("niri")
        self.assertIn("--virtual120", hypr)
        self.assertNotIn("--virtual120", niri)
        self.assertEqual(hypr[hypr.index("--runs") + 1], "1")
        self.assertIn("--bar-ui", hypr)
        self.assertIn("bead", hypr)

    def test_exact_sha_pin_rejects_other_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "binary"
            path.write_bytes(b"frozen")
            digest = arm.sha256(path)
            self.assertEqual(arm.verify_artifact(path, digest)["sha256"], digest)
            with self.assertRaisesRegex(RuntimeError, "mismatch"):
                arm.verify_artifact(path, "0" * 64)

    def test_b3_validator_requires_complete_safety_and_typing_proof(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            comp = "niri"
            (root / f"{comp}-capture-safety.json").write_text(
                '{"status":"complete","capture_returncode":0,"scope_runner_returncode":0,'
                '"final_cgroup_read_while_scope_held":true,"oom_delta":{"oom":0,"oom_kill":0}}')
            (root / "host-swapin-delta.txt").write_text("host_swapin_delta_bytes=0\n")
            measured = root / comp
            measured.mkdir()
            trace = "MENU_RESULT ok=true catalog_rev=8 count=320\nUI_COMMIT generation=1 update=1 source_error=false\n" + "".join(
                f"UI_PRESENTED generation={i} update={i} cpu_ns=100\n" for i in range(200))
            (measured / "engine-typing.log").write_text(trace)
            (measured / "menu-engine-proof.json").write_text(__import__("json").dumps({
                "valid": True, "profile_mode": "clocks", "keys": 200,
                "rows": [{"update": i + 1, "result_count": 320} for i in range(200)],
                "memory": {"ui": {"swap_pss_mib": 0}, "daemon": {"swap_pss_mib": 0},
                           "workers": [{"swap_pss_mib": 0}]},
                "idle_memory_screen": [{"elapsed_seconds": float(i), "ui": {"swap_pss_mib": 0},
                    "daemon": {"swap_pss_mib": 0}, "workers": [{"swap_pss_mib": 0}],
                    "swap_pss_mib": 0} for i in range(61)]}))
            (measured / "output-geometry.json").write_text('{"modes":[{"width":2880,"height":1800,"refresh_rate":60000}],"current_mode":0,"logical":{"width":1920,"height":1200,"scale":1.5}}')
            result = arm.validate_arm(root, comp, "clocks")
            self.assertTrue(result["valid"])
            proof_path = measured / "menu-engine-proof.json"
            proof = __import__("json").loads(proof_path.read_text())
            proof["memory"]["workers"][0]["swap_pss_mib"] = 0.01
            proof_path.write_text(__import__("json").dumps(proof))
            with self.assertRaisesRegex(RuntimeError, "SwapPss"):
                arm.validate_arm(root, comp, "clocks")
            proof["memory"]["workers"][0]["swap_pss_mib"] = 0
            proof_path.write_text(__import__("json").dumps(proof))
            safety = __import__("json").loads((root / f"{comp}-capture-safety.json").read_text())
            safety["final_cgroup_read_while_scope_held"] = False
            (root / f"{comp}-capture-safety.json").write_text(__import__("json").dumps(safety))
            with self.assertRaisesRegex(RuntimeError, "incomplete"):
                arm.validate_arm(root, comp, "clocks")


if __name__ == "__main__":
    unittest.main()
