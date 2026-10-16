#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run Bead in an isolated nested window, using an explicit standalone daemon."""
import argparse
import importlib.util
import signal
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--daemon', type=Path, required=True, help='built nacre-helper; never an installed control script')
    parser.add_argument('--bead', type=Path, default=Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'release/nacre-shell')
    parser.add_argument('--shell', type=Path, default=ROOT / 'shell', help='matching shell source bundle for the selected Bead binary')
    parser.add_argument('--compositor', choices=['niri', 'hyprland'], default='niri')
    parser.add_argument('--headless-smoke', action='store_true', help='private compositor test, closes automatically')
    args = parser.parse_args()
    if args.compositor=='hyprland' and not args.headless_smoke:
        parser.error('The interactive window uses niri; Hyprland supports the private headless smoke here')
    if os.getpriority(os.PRIO_PROCESS, 0) < 10:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
    for path in [args.daemon, args.bead]:
        if not path.is_file() or not os.access(path, os.X_OK):
            parser.error('Provide explicit built executable paths; this preview installs and builds nothing')
    if not (args.shell / 'ui/main.luau').is_file() or not (args.shell / 'bar/main.luau').is_file():
        parser.error('--shell needs a matching Bead shell bundle')
    env = {key:value for key,value in os.environ.items() if not key.startswith('BEAD_') or key in ['BEAD_RESEARCH_TOOLS','BEAD_RAW_DIR']}
    display = env.get('WAYLAND_DISPLAY', '')
    if display and not display.startswith('/'):
        display = str(Path(env['XDG_RUNTIME_DIR']) / display)
    if not args.headless_smoke and not display:
        parser.error('Run from a Wayland desktop terminal to open the nested window')
    with tempfile.TemporaryDirectory(prefix='bead-preview-') as directory:
        private = Path(directory)
        (private / 'home').mkdir(mode=0o700)
        env.update(HOME=str(private / 'home'), WAYLAND_DISPLAY=display,
                   BEAD_HARDENING='foundation-preview',
                   BEAD_PREVIEW_DAEMON=str(args.daemon.resolve()),
                   BEAD_PREVIEW_SHELL=str(args.shell.resolve()),
                   BEAD_HARDENING_BINARY=str(args.bead.resolve()),
                   BEAD_PREVIEW_SMOKE='1' if args.headless_smoke else '0',
                   BEAD_RAW_DIR=env.get('BEAD_RAW_DIR', str(private / 'logs')),
                   DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(private / 'no-system-bus'))
        for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'NACRE_DAEMON_SOCKET',
                    'NACRE_COMPOSITOR', 'DBUS_SESSION_BUS_ADDRESS', 'DBUS_SESSION_BUS_PID',
                    'BEAD_PHYSICAL_NIRI_APPROVED', 'BEAD_BAR_MATRIX']:
            env.pop(key, None)
        command = ['python3', str(ROOT / 'spikes/measure.py'), '--compositor', args.compositor,
                   '--parent', 'headless' if args.headless_smoke else 'desktop',
                   '--bar-ui', '--modes', 'bead', '--runs', '1']
        if args.headless_smoke and args.compositor=='hyprland':
            command += ['--virtual120']
        print('Private nested preview: click the bar launcher icon/lip, type, Escape to close. Close the window or Ctrl+C to stop.', flush=True)
        print('Launcher queries and UI controls use the daemon. Notifications are hosted by the transitional Bead service on a private bus; daemon notification hosting/history is still pending.', flush=True)
        # Reuse the established private-session harness and its owned-process reaper.
        spec = importlib.util.spec_from_file_location('preview_owned', ROOT / 'spikes/process-clock.py')
        clocks = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(clocks)
        child = subprocess.Popen(command, cwd=ROOT, env=env, start_new_session=True)
        def interrupt(*_):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM, interrupt)
        def stop_preview(process):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
        try:
            return child.wait()
        except KeyboardInterrupt:
            return 0
        finally:
            clocks.stop_owned_tree(child, stop_preview)


if __name__ == '__main__':
    raise SystemExit(main())
