#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run Bead in an isolated nested window, using an explicit standalone daemon.

Default: the whole shell exactly as `bead-shell start` runs it (bar, frame and lips, launcher, dashboard,
session menu, settings window, AI sidebar, notices/OSD, wallpaper), one process, against a private daemon.
Everything lives in a private niri, D-Bus, runtime dir and HOME; the live session is never touched."""
import argparse
import importlib.util
import json
import signal
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent

CTL = 'NACRE_DAEMON_SOCKET="$XDG_RUNTIME_DIR/nacre/daemon.sock" exec {bin}/nacrectl ui '
# Alt is the nested niri's Mod key; the outer compositor keeps Super.
BINDS = [('Space', 'nacre launcher ""', 'launcher'), ('O', 'drawers toggle dashboard', 'dashboard'),
         ('I', 'nacre settings', 'settings window'), ('A', 'nacre left', 'AI sidebar'),
         ('X', 'nacre session', 'session menu'), ('V', 'nacre clipboard', 'clipboard'),
         ('W', 'nacre launcher ">wallpaper "', 'wallpaper picker'), ('Period', 'nacre menu actions.emoji ""', 'emoji'),
         ('K', 'nacre menu learn.shortcuts ""', 'shortcut list'), ('Z', 'nacre hide', 'hide / show the shell')]
CHEAT_SHEET = 'Bead preview (private nested window). Keys, with the nested compositor Mod = Alt:\n' + '\n'.join(
    f'  Alt+{key:<7} {label}' for key, _, label in BINDS) + (
    '\n  Alt+N       send a test notification\n  Alt+Shift+Q  quit the preview (or close the window / Ctrl+C here)\n'
    'Also: hover the screen edges for the frame lips, click bar icons for popouts, Esc closes a panel.\n'
    'Standalone daemon with fixture data: no real network, audio, battery or accounts; your session is untouched.')


def write_niri_config(private, shell):
    ctl = CTL.format(bin=private / 'bin')
    lines = ['input {', '    keyboard {', '        xkb {', '            layout "us"', '        }', '    }', '}',
             'prefer-no-csd', 'hotkey-overlay {', '    skip-at-startup', '}', 'binds {']
    for key, command, _ in BINDS:
        lines.append(f'    Alt+{key} repeat=false {{ spawn-sh {json.dumps(ctl + command)}; }}')
    notify = ("gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications "
              "--method org.freedesktop.Notifications.Notify 'Bead preview' 0 '' 'Test notification' "
              "\"Sent at $(date +%T)\" '[]' '{}' 8000")
    lines.append(f'    Alt+N repeat=false {{ spawn-sh {json.dumps(notify)}; }}')
    lines.append('    Alt+Shift+Q { quit skip-confirmation=true; }')
    lines.append('}')
    lines.append((shell / 'niri-pearls.kdl').read_text())
    path = private / 'niri.kdl'
    path.write_text('\n'.join(lines) + '\n')
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--daemon', type=Path, required=True, help='built nacre-helper; never an installed control script')
    parser.add_argument('--bead', type=Path, default=Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')) / 'release/nacre-shell')
    parser.add_argument('--shell', type=Path, default=ROOT / 'shell', help='matching shell source bundle for the selected Bead binary')
    parser.add_argument('--compositor', choices=['niri', 'hyprland'], default='niri')
    parser.add_argument('--tools', type=Path, help='Nacre nacre/shell-tools directory matching --daemon (notification policy, menus)')
    parser.add_argument('--real-wallpaper', action='store_true', help='read-only: copy your presentation.json record so your wallpaper and palette show; the image is only read')
    parser.add_argument('--cpu', action='store_true', help='force the CPU renderer (use if the GL renderer misbehaves in the nested window)')
    parser.add_argument('--screenshots', type=Path, help='tour the views in an offscreen private parent and save PNGs here with grim, then exit')
    parser.add_argument('--legacy', action='store_true', help='old foundation preview: separate notice process, no wallpaper/frame/settings')
    parser.add_argument('--dashboard-fixtures', action='store_true', help='show clearly labeled artificial dashboard data; no dashboard backend calls')
    parser.add_argument('--headless-smoke', action='store_true', help='private compositor test, closes automatically')
    args = parser.parse_args()
    if args.compositor=='hyprland' and not args.headless_smoke:
        parser.error('The interactive window uses niri; Hyprland supports the private headless smoke here')
    if os.getpriority(os.PRIO_PROCESS, 0) < 10:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
    for path in [args.daemon, args.bead]:
        if not path.is_file() or not os.access(path, os.X_OK):
            parser.error('Provide explicit built executable paths; this preview installs and builds nothing')
    full = not (args.legacy or args.headless_smoke)
    entries = ['ui/main.luau', 'bar/main.luau', 'popouts/main.luau', 'sound/main.luau', 'status/main.luau']
    if any(not (args.shell / entry).is_file() for entry in entries):
        parser.error('--shell needs a matching Bead core-views bundle')
    if args.dashboard_fixtures and not (args.shell / 'tests/fixtures/dashboard/weather.json').is_file():
        parser.error('--dashboard-fixtures needs the matching dashboard bundle')
    env = {key:value for key,value in os.environ.items() if not key.startswith('BEAD_') or key in ['BEAD_RESEARCH_TOOLS','BEAD_RAW_DIR']}
    display = env.get('WAYLAND_DISPLAY', '')
    if display and not display.startswith('/'):
        display = str(Path(env['XDG_RUNTIME_DIR']) / display)
    headless = args.headless_smoke or bool(args.screenshots)  # a screenshot tour needs no visible window
    if not headless and not display:
        parser.error('Run from a Wayland desktop terminal to open the nested window')
    with tempfile.TemporaryDirectory(prefix='bead-preview-') as directory:
        private = Path(directory)
        (private / 'home').mkdir(mode=0o700)
        if full:
            (private / 'bin').mkdir(mode=0o700)
            (private / 'bin/nacrectl').symlink_to(args.daemon.resolve())
            env['BEAD_PREVIEW_FULL'] = '1'
            env['BEAD_PREVIEW_NACRECTL'] = str(private / 'bin/nacrectl')
            env['BEAD_PREVIEW_NIRI_CONFIG'] = str(write_niri_config(private, args.shell))
            env['BEAD_PREVIEW_CPU'] = '1' if args.cpu else '0'
            if args.tools:
                env['BEAD_PREVIEW_TOOLS'] = str(args.tools.resolve())
            if args.screenshots:
                env['BEAD_PREVIEW_SHOTS'] = str(args.screenshots.resolve())
            record = Path.home() / '.local/state/nacre/presentation.json'
            if args.real_wallpaper and record.is_file():
                env['BEAD_PREVIEW_PRESENTATION'] = str(record)
            elif args.real_wallpaper:
                print('No presentation.json under ~/.local/state/nacre; showing no wallpaper.', flush=True)
        env.update(HOME=str(private / 'home'), WAYLAND_DISPLAY=display,
                   BEAD_HARDENING='foundation-preview',
                   BEAD_PREVIEW_DAEMON=str(args.daemon.resolve()),
                   BEAD_PREVIEW_SHELL=str(args.shell.resolve()),
                   BEAD_HARDENING_BINARY=str(args.bead.resolve()),
                   BEAD_PREVIEW_SMOKE='1' if args.headless_smoke else '0',
                   BEAD_PREVIEW_DASHBOARD_FIXTURES='1' if args.dashboard_fixtures else '0',
                   BEAD_RAW_DIR=env.get('BEAD_RAW_DIR', str(private / 'logs')),
                   DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(private / 'no-system-bus'))
        for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'NACRE_DAEMON_SOCKET',
                    'NACRE_COMPOSITOR', 'DBUS_SESSION_BUS_ADDRESS', 'DBUS_SESSION_BUS_PID',
                    'BEAD_PHYSICAL_NIRI_APPROVED', 'BEAD_BAR_MATRIX']:
            env.pop(key, None)
        command = ['python3', str(ROOT / 'spikes/measure.py'), '--compositor', args.compositor,
                   '--parent', 'headless' if headless else 'desktop',
                   '--bar-ui', '--modes', 'bead', '--runs', '1']
        if args.headless_smoke and args.compositor=='hyprland':
            command += ['--virtual120']
        if full:
            print(CHEAT_SHEET, flush=True)
        else:
            print('Private nested preview (legacy): click the bar launcher icon/lip, type, Escape to close. Close the window or Ctrl+C to stop.', flush=True)
        if args.dashboard_fixtures:
            print('Dashboard values are artificial preview fixtures; player controls and live service hosting remain unavailable.', flush=True)
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
