# SPDX-License-Identifier: GPL-3.0-only
import importlib.util
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
