# SPDX-License-Identifier: GPL-3.0-only
import importlib.util
from pathlib import Path
import os
import socket
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("foundation_preview", Path(__file__).resolve().parents[1] / "foundation-preview.py")
PREVIEW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREVIEW)


class DisplayWatch(unittest.TestCase):
    def test_a_closed_compositor_socket_ends_the_preview_loop(self):
        with tempfile.TemporaryDirectory(prefix="bd-t-") as directory:
            path = Path(directory) / "wayland-1"
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(path))
            listener.listen(1)
            self.assertTrue(PREVIEW.display_alive(path))
            listener.close()  # the compositor exited: the file stays, nothing listens
            self.assertFalse(PREVIEW.display_alive(path))
            path.unlink()
            self.assertFalse(PREVIEW.display_alive(path))


class PrivateCall(unittest.TestCase):
    def fake(self, directory, body):
        path = Path(directory) / "nacrectl"
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(0o700)
        return str(path)

    def test_a_hung_nacrectl_is_a_retryable_runtime_error_not_a_crash(self):
        with tempfile.TemporaryDirectory(prefix="bd-t-") as directory:
            fake = self.fake(directory, "sleep 5\n")
            saved, PREVIEW.CALL_PROCESS_LIMIT = PREVIEW.CALL_PROCESS_LIMIT, 0.3
            try:
                with self.assertRaisesRegex(RuntimeError, "did not finish"):
                    PREVIEW.private_call(fake, dict(os.environ), "ui.dispatch", {})
            finally:
                PREVIEW.CALL_PROCESS_LIMIT = saved

    def test_failure_carries_stderr_and_success_parses_json(self):
        with tempfile.TemporaryDirectory(prefix="bd-t-") as directory:
            failing = self.fake(directory, "echo 'UI provider is unavailable' >&2\nexit 1\n")
            with self.assertRaisesRegex(RuntimeError, "UI provider is unavailable"):
                PREVIEW.private_call(failing, dict(os.environ), "ui.dispatch", {})
            working = self.fake(directory, "echo '{\"ok\":1}'\n")
            self.assertEqual(PREVIEW.private_call(working, dict(os.environ), "daemon.status", {}), {"ok": 1})


if __name__ == "__main__":
    unittest.main()
