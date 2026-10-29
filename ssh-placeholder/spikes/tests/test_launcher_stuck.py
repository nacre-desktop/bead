#!/usr/bin/env python3
"""Private app-catalog setup used by the B0160 nested launcher proof."""
import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "menu_engine_proof", ROOT / "menu-engine-proof.py"
)
menu_engine_proof = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(menu_engine_proof)
launcher_proof = menu_engine_proof.load(
    "launcher_proof", ROOT / "launcher-proof.py"
)


class PrivateAppDataTests(unittest.TestCase):
    def test_copies_user_and_system_entries_without_rewriting_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            (source / "user").mkdir(parents=True)
            (source / "system").mkdir()
            user_text = "[Desktop Entry]\nName=Private User App\nExec=never-run\n"
            system_text = "[Desktop Entry]\nName=System App\nExec=never-run\n"
            (source / "user/user.desktop").write_text(user_text)
            (source / "system/system.desktop").write_text(system_text)

            count = menu_engine_proof.populate_private_app_data(root / "private", source)

            self.assertEqual(count, 2)
            self.assertEqual(
                (root / "private/data/applications/user.desktop").read_text(), user_text
            )
            self.assertEqual(
                (root / "private/system/applications/system.desktop").read_text(),
                system_text,
            )

    def test_rejects_an_empty_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "source/user").mkdir(parents=True)
            (root / "source/system").mkdir()
            with self.assertRaisesRegex(RuntimeError, "contains no desktop entries"):
                menu_engine_proof.populate_private_app_data(root / "private", root / "source")

    def test_synthetic_default_remains_a_320_entry_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertEqual(menu_engine_proof.populate_private_app_data(Path(temporary)), 320)

    def test_open_evidence_joins_latency_and_presentation_markers(self):
        start_ns = 1_000_000_000
        text = (
            "PANEL_OPEN_LATENCY name=launcher ms_to_first_content_frame=12.500\n"
            "PANEL_PRESENTED monotonic_ns=1010000000\n"
            "PANEL_PRESENTED monotonic_ns=1020000000\n"
        )
        evidence = launcher_proof.launcher_open_evidence(text, 0, start_ns)
        self.assertEqual(evidence["content_latency_ms"], 12.5)
        self.assertEqual(evidence["first_presented_ms"], 10.0)
        self.assertEqual(evidence["presented_frames"], 2)


if __name__ == "__main__":
    unittest.main()
