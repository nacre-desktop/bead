#!/usr/bin/env python3
"""Edit only a temporary UI copy inside the existing nested preview.

SPDX-License-Identifier: MIT OR Apache-2.0
"""
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    compositor = sys.argv[1]
    commands = {
        'niri': ['niri', '--config', str(ROOT/'examples/niri.kdl')],
        'hyprland': ['Hyprland', '--config', str(ROOT/'examples/hyprland.lua')],
    }
    if compositor not in commands:
        raise SystemExit('Choose niri or hyprland')
    def interrupt(*_):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupt)
    with tempfile.TemporaryDirectory(prefix='bead-reload-demo-') as directory:
        root = pathlib.Path(directory)
        shutil.copytree(ROOT/'shell', root/'shell')
        env = os.environ.copy()
        preview_log = root/'preview.log'
        preview_log.touch()
        env['BEAD_PREVIEW_LOG'] = str(preview_log)
        if compositor == 'hyprland':
            env['AQ_DRM_DEVICES'] = '/dev/null'
        child = subprocess.Popen(commands[compositor], cwd=root, env=env, start_new_session=True)
        try:
            content = root/'shell/ui/main.luau'
            original = content.read_text()
            behavior = root/'shell/ui/behavior.luau'
            print('DEMO: type in Search and scroll now. Every eight seconds: valid edit, broken file, recovery. Only a temporary copy changes.', flush=True)
            for stage in ['valid', 'broken', 'recovered']:
                for _ in range(80):
                    if child.poll() is not None:
                        return child.returncode
                    time.sleep(.1)
                if stage == 'valid':
                    content.write_text(original.replace('Applications', 'Applications · reloaded'))
                    behavior.write_text(behavior.read_text().replace('["launcher.search.text"] = ""', '["launcher.search.text"] = "new default"'))
                elif stage == 'broken':
                    content.write_text('return { broken file')
                else:
                    content.write_text(original)
                print('DEMO: '+stage+'; your input, scroll and Home state should survive.', flush=True)
                time.sleep(.5)
                for line in preview_log.read_text().splitlines():
                    if line.startswith(('UI_COMMIT', 'UI_ERROR')):
                        print(line, flush=True)
            print('DEMO complete. Close the nested window or press Ctrl+C.', flush=True)
            return child.wait()
        except KeyboardInterrupt:
            return 0
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=5)


if __name__ == '__main__':
    sys.exit(main())
