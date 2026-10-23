"""Staging and launcher tests with stub binaries; no build, display or daemon."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STUB = '#!/bin/sh\nprintf "%s\\n" "$PWD" "$@" > "$STUB_LOG"\n'


class StageTests(unittest.TestCase):
    def stage(self, prefix):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        base = Path(temp.name)
        release = base / "release"
        release.mkdir()
        for name in ("nacre-shell", "nacre-wallpaper-decode", "bead-wallpaper-video"):
            (release / name).write_text(STUB)
            (release / name).chmod(0o755)
        dest = base / "dest"
        subprocess.run([ROOT / "packaging/stage.sh", dest, prefix, release], check=True)
        return base, dest, dest / prefix.lstrip("/")

    def test_layout_excludes_tests_and_stays_under_prefix(self):
        base, dest, installed = self.stage("/usr")
        self.assertTrue((installed / "bin/bead-shell").stat().st_mode & 0o111)
        self.assertTrue((installed / "lib/bead/nacre-wallpaper-decode").is_file())
        self.assertTrue((installed / "lib/bead/bead-wallpaper-video").is_file())
        self.assertTrue((installed / "share/bead/shell/bar/main.luau").is_file())
        self.assertFalse((installed / "share/bead/shell/tests").exists())
        self.assertFalse((installed / "share/bead/shell/i18n").exists())
        self.assertTrue((installed / "share/licenses/bead-shell/LICENSE-GPL-3.0").is_file())
        self.assertEqual({p.name for p in dest.iterdir()}, {"usr"})

    def run_launcher(self, installed, base, *args, **env):
        return subprocess.run(
            [installed / "bin/bead-shell", *args],
            env={"PATH": os.environ["PATH"], "STUB_LOG": str(base / "log"), **env},
            capture_output=True, text=True,
        )

    def test_check_passes_and_detects_a_missing_piece(self):
        base, _, installed = self.stage("/opt/bead")  # relocatable, nothing refers to /usr
        self.assertEqual(self.run_launcher(installed, base, "check").returncode, 0)
        (installed / "share/bead/shell/bar/main.luau").unlink()
        result = self.run_launcher(installed, base, "check")
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing bundle entry bar", result.stderr)

    def test_start_uses_daemon_sockets_and_bundle_directory(self):
        base, _, installed = self.stage("/usr")
        self.assertEqual(self.run_launcher(installed, base, "start", XDG_RUNTIME_DIR="/run/user/1").returncode, 0)
        cwd, *argv = (base / "log").read_text().split("\n")[:-1]
        self.assertEqual(cwd, str(installed / "share/bead"))
        self.assertEqual(argv[:3], ["--bar", "--core-views", "--ui-provider"])
        self.assertIn("/run/user/1/nacre/daemon.sock", argv)
        self.assertIn("/run/user/1/nacre/bead-command.sock", argv)

    def test_start_needs_a_session_and_unknown_commands_fail(self):
        base, _, installed = self.stage("/usr")
        self.assertNotEqual(self.run_launcher(installed, base, "start").returncode, 0)
        self.assertEqual(self.run_launcher(installed, base, "bogus").returncode, 2)


if __name__ == "__main__":
    unittest.main()
