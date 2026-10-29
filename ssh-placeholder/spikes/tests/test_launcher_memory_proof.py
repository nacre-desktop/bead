import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("launcher_proof", ROOT / "launcher-proof.py")
launcher_proof = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(launcher_proof)


class LauncherMemoryProofTests(unittest.TestCase):
    def test_rejects_large_expired_launcher_growth(self):
        start = {"memory": {"pss_anon_mib": 38.7}}
        expired = {"memory": {"pss_anon_mib": 501.5}}

        self.assertFalse(launcher_proof.expired_anon_within_budget(start, expired))
        self.assertAlmostEqual(
            launcher_proof.expired_anon_delta_mib(start, expired), 462.8
        )

    def test_accepts_expired_launcher_within_ten_mib(self):
        start = {"memory": {"pss_anon_mib": 38.7}}
        expired = {"memory": {"pss_anon_mib": 48.6}}

        self.assertTrue(launcher_proof.expired_anon_within_budget(start, expired))


if __name__ == "__main__":
    unittest.main()
