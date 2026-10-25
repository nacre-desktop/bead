#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0056 side-by-side parity harness: Nacre's QML shell (A) against Bead (B), same states, same session.

Run it ON THE DESKTOP (never the laptop), under `flock ~/nacre-lead/desktop-heavy.lock`:

    python3 spikes/parity-compare.py --nacre DIR --bead-bin DIR --bead-shell DIR --out DIR [--states a,b] [--arms qml,bead]

  --nacre       a Nacre checkout or `git archive` of origin/main holding nacre/shell and nacre/shell-tools
  --bead-bin    a directory with the built `nacre-shell` and `nacre-helper` (the Nacre daemon, same revision as --nacre)
  --bead-shell  Bead's shell/ bundle that matches that binary
  --out         gallery directory; open <out>/index.html (copy it to the laptop with scp/rsync)

Each arm gets its own throwaway everything: a headless labwc parent, a nested niri (2880x1800 at scale 1.5 by
default, the owner's laptop geometry), a private D-Bus session, XDG dirs and HOME, and the SAME standalone
nacre-helper daemon, wallpaper/palette record and settings. States are driven through the daemon's `ui.dispatch`
(the vocabulary both shells register), so one script drives both. Screenshots are `grim`; open/close
transitions are short grim frame sequences with timestamps. The gallery shows QML | Bead | diff per state with
numeric differences, and --baseline PREVIOUS/results.json adds before/after columns.
Nothing here touches a live session: private sockets only, and only exact PIDs started here are stopped."""
import argparse
import html
import json
import math
import os
os.environ.setdefault('BEAD_VERBOSE_LOG', '1')
import re
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fake_system  # noqa: E402
import capture_safety  # noqa: E402

# state name -> (steps that open it, steps that close it, sequence seconds).  Steps are tuples:
#   ('ui', target, method, *args)   daemon ui.dispatch, identical for both shells
#   ('notify', summary, body)       a freedesktop notification on the private bus
#   ('sleep', seconds)
#   ('shell', target, method, *args) a call on Bead's own command socket (views the daemon vocabulary lacks)
#   ('app_close', name)             close Settings (an app window in both shells)
#   ('qml_settings', page)          QML only: `quickshell ipc call settingsApp open PAGE` (Bead: ('shell', 'nacre', 'settings', PAGE))
#   ('qml_app', app, method, *args) QML only: `quickshell ipc call APP METHOD ARGS` (Bead arm: a ('ui', APP, METHOD, ARGS) step)
#   ('type', text)                  B0092: wtype into the focused panel of the nested session (same keystrokes for both arms)
#   ('only', 'qml'|'bead', step)    a step for one arm (the shells' preview/trigger routes differ)
#   ('level', 'sink'|'source', 0..1) a fake audio level the daemon's osd service reads (Bead shows it; the QML has no daemon osd)
#   ('pointer', x, y) / ('press', x, y) / ('release',)   B0067: a virtual pointer on the private parent compositor (logical px; needs --pointer-tool)
#   ('journal', 'on'|'off')         B0089: write/remove the controller's display-pending journal (a pending change; nothing is applied)
#   ('bluez', key, value)           the fake org.bluez on the private bus (spikes/fake-bluez.py): power on|off, discovering on|off, reset
#   ('nm', key, value)              the fake NetworkManager (spikes/fake-nm.py, needs --fake-nm): connect|disconnect work|home, reset
# Logical pointer targets (2880x1800 at 1.5), per arm. Bluetooth is available
# in both arms through the private BlueZ fixture.
PTR = {'control-tile': {'qml': (1790, 290), 'bead': (1800, 290)}, 'launcher-quick': (1350, 745),
       'vpn-tile': {'qml': (1793, 220), 'bead': (1793, 220)}, 'vpn-row': {'qml': (1650, 242), 'bead': (1650, 324)},
       # B0093: the third row of the selector prompt (panel x 410, y 610; list at 20,54; 58 px pitch)
       'prompt-row': {'qml': (930, 807), 'bead': (930, 807)}}
# B0093: one fixed selector request for both shells (`nacre-menu pick --json` / `nacre-menu input`).
PROMPT_CHOICES = json.dumps([
    {'label': 'Home Wi-Fi', 'value': 'home', 'description': 'WPA2 · 92%', 'icon': 'wifi'},
    {'label': 'Guest', 'value': 'guest', 'description': 'Open · 71%', 'icon': 'wifi'},
    {'label': 'Workshop', 'value': 'workshop', 'description': 'WPA3 · 48%', 'icon': 'wifi'},
    {'label': 'Office (out of range)', 'value': 'office', 'description': 'Saved', 'icon': 'wifi_off', 'disabled': True},
    {'label': 'Neighbour', 'value': 'neighbour', 'description': 'WPA2 · 33%', 'icon': 'wifi'},
    {'label': 'Cafe', 'value': 'cafe', 'description': 'Open · 21%', 'icon': 'wifi'}])
# A normal press is sampled before QML's long-press detail timer can fire.
STATE_SETTLE = {'osd-brightness': 0.35, 'osd-keyboard': 0.35, 'control-pressed': 0.05, 'control-vpn-pressed': 0.05}
STATE_SETTLE.update({'bar-title-hover': .18, 'bar-title-open': .8})
# QML audio inherits the preceding popup's width. Establish the same explicit
# 352 px content-width precondition for both builds, independent of state order.
STATE_PREPARE = {'popout-audio': [('ui', 'nacre', 'popout', 'network', '1560'), ('sleep', 0.4),
                                  ('ui', 'nacre', 'close'), ('sleep', 0.5)]}
STATE_CLEANUP = {}
# B0093: QML creates its selector service (and registers `menuPrompt`) with the launcher, so open and close it once first.
for _prompt_state in ('menu-pick', 'menu-pick-hover', 'menu-input'):
    STATE_PREPARE[_prompt_state] = [('ui', 'nacre', 'launcher', ''), ('sleep', 2.0), ('ui', 'nacre', 'close'), ('sleep', 1.0)]
STATES = {
    'idle': ([], [], 0),
    'bar-idle': ([], [], 0),
    'bar-workspace': ([('bar-window',)], [('bar-window-close',)], 0),
    'bar-title-hover': ([('pointer-fast', 960, 25)], [('pointer', 960, 600)], 0),
    'bar-title-open': ([('pointer', 960, 25)], [('pointer', 960, 600), ('ui', 'nacre', 'close')], 0),
    'overview': ([('ui', 'nacre', 'mode', 'overview')], [('ui', 'nacre', 'close')], 1.0),
    'palette': ([('ui', 'nacre', 'mode', 'palette')], [('ui', 'nacre', 'close')], 1.0),
    'palette-typed': ([('ui', 'nacre', 'mode', 'palette'), ('sleep', 2.5), ('type', 'set')], [('ui', 'nacre', 'close')], 0),
    'palette-brain': ([('ui', 'nacre', 'mode', 'palette'), ('sleep', 2.5), ('type', '> brain: hello')], [('ui', 'nacre', 'close')], 0),
    'keybindings': ([('ui', 'nacre', 'mode', 'keys')], [('ui', 'nacre', 'close')], 1.0),
    'launcher': ([('ui', 'nacre', 'launcher', '')], [('ui', 'nacre', 'close')], 1.4),
    'launcher-hover': ([('ui', 'nacre', 'launcher', ''), ('sleep', 1.4), ('pointer', *PTR['launcher-quick'])],
                       [('ui', 'nacre', 'close')], 0),
    'launcher-pressed': ([('ui', 'nacre', 'launcher', ''), ('sleep', 1.4), ('press', *PTR['launcher-quick'])],
                         [('release',), ('ui', 'nacre', 'close')], 0),
    'launcher-typed': ([('ui', 'nacre', 'launcher', 'fi')], [('ui', 'nacre', 'close')], 0),
    'launcher-wallpapers': ([('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 0),
    # B0100: the same library and picker preferences, in each isolated HOME.
    'wallpaper-carousel': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-hover': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 2), ('pointer', 960, 1020)], [('ui', 'nacre', 'close')], 0),
    'wallpaper-pressed': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 2), ('press', 960, 1020)], [('release',), ('ui', 'nacre', 'close')], 0),
    'wallpaper-search': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 2), ('press', 960, 944), ('release',), ('type', 'generated-1')], [('ui', 'nacre', 'close')], 0),
    'wallpaper-spotlight': ([('wallpaper-pref', 'static', 'spotlight'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-hexagons': ([('wallpaper-pref', 'static', 'hexagons'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-dynamic-spotlight': ([('wallpaper-pref', 'dynamic', 'spotlight'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-dynamic-hexagons': ([('wallpaper-pref', 'dynamic', 'hexagons'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-empty': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 2), ('press', 960, 944), ('release',), ('type', 'zzzz-no-matches')], [('ui', 'nacre', 'close')], 0),
    'wallpaper-video-activated': ([('wallpaper-pref', 'dynamic', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 3), ('press', 960, 944), ('release',), ('type', 'motion-1'), ('key', 'Right'), ('key', 'Return'), ('sleep', 6)], [('ui', 'nacre', 'close')], 0),
    'wallpaper-dynamic': ([('wallpaper-pref', 'dynamic', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper ')], [('ui', 'nacre', 'close')], 1.0),
    'wallpaper-activated': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 3), ('key', 'Right'), ('key', 'Return'), ('sleep', 6)], [('ui', 'nacre', 'close')], 0),
    'wallpaper-settled': ([('wallpaper-pref', 'static', 'carousel'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 3), ('ui', 'nacre', 'close'), ('sleep', 7)], [], 0),
    'wallpaper-spotlight-settled': ([('wallpaper-pref', 'static', 'spotlight'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 3), ('ui', 'nacre', 'close'), ('sleep', 7)], [], 0),
    'wallpaper-hexagons-settled': ([('wallpaper-pref', 'static', 'hexagons'), ('ui', 'nacre', 'launcher', '>wallpaper '), ('sleep', 3), ('ui', 'nacre', 'close'), ('sleep', 7)], [], 0),
    'menu-power': ([('ui', 'nacre', 'menu', 'power', '')], [('ui', 'nacre', 'close')], 0),
    'dashboard-0': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '0')], [('ui', 'nacre', 'close')], 1.4),
    # B0067: the same open, measured after one open/close (the view is built and warm), to tell a cold first frame from the motion
    'dashboard-warm': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '0')], [('ui', 'nacre', 'close')], 1.4),
    'dashboard-1': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '1')], [('ui', 'nacre', 'close')], 0),
    'dashboard-2': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '2')], [('ui', 'nacre', 'close')], 0),
    'dashboard-3': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '3')], [('ui', 'nacre', 'close')], 0),
    'dashboard-4': ([('ui', 'drawers', 'toggle', 'dashboard'), ('ui', 'nacre', 'tab', '4')], [('ui', 'nacre', 'close')], 0),
    'popout-calendar': ([('ui', 'nacre', 'popout', 'calendar', '1700')], [('ui', 'nacre', 'close')], 1.0),
    'popout-network': ([('ui', 'nacre', 'popout', 'network', '1560')], [('ui', 'nacre', 'close')], 1.0),
    'popout-bluetooth': ([('ui', 'nacre', 'popout', 'bluetooth', '1600')], [('ui', 'nacre', 'close')], 0),
    'popout-bluetooth-scanning': ([('bluez', 'discovering', 'on'), ('sleep', 2.5), ('ui', 'nacre', 'popout', 'bluetooth', '1600')],
                                  [('ui', 'nacre', 'close'), ('bluez', 'reset', 'now')], 0),
    'popout-bluetooth-off': ([('bluez', 'power', 'off'), ('sleep', 1.0), ('ui', 'nacre', 'popout', 'bluetooth', '1600')],
                             [('ui', 'nacre', 'close'), ('bluez', 'reset', 'now')], 0),
    'popout-battery': ([('ui', 'nacre', 'popout', 'battery', '1640')], [('ui', 'nacre', 'close')], 0),
    'popout-audio': ([('ui', 'nacre', 'popout', 'audio', '1520')], [('ui', 'nacre', 'close')], 0),
    'control-center': ([('ui', 'nacre', 'controls', 'home')], [('ui', 'nacre', 'close')], 1.2),
    'control-battery': ([('ui', 'nacre', 'controls', 'battery')], [('ui', 'nacre', 'close')], 0),
    'control-audio': ([('ui', 'nacre', 'controls', 'audio')], [('ui', 'nacre', 'close')], 0),
    'control-network': ([('ui', 'nacre', 'controls', 'network')], [('ui', 'nacre', 'close')], 0),
    # B0067: pointer states (need --pointer-tool): same semantic Bluetooth toggle per arm.
    'control-hover': ([('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2), ('pointer-target', 'control-tile')],
                      [('release',), ('ui', 'nacre', 'close')], 0),
    'control-pressed': ([('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2), ('press-target', 'control-tile')],
                        [('release',), ('ui', 'nacre', 'close')], 0),
    'lip-hover-right': ([('pointer', 1919, 600), ('sleep', 1.0)], [('pointer', 960, 600)], 0),
    'control-bluetooth': ([('ui', 'nacre', 'controls', 'bluetooth')], [('ui', 'nacre', 'close')], 0),
    # B0087 (need --fake-nm): the VPN tile and page, idle and with one profile connected.
    # QML refuses `controls vpn`: the page opens from the tile (a virtual-pointer click, so --pointer-tool is needed).
    'control-vpn': ([('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2), ('press-target', 'vpn-tile'), ('release',), ('sleep', 1.0)],
                    [('ui', 'nacre', 'close')], 0),
    'control-vpn-connected': ([('nm', 'connect', 'home'), ('sleep', 1.5), ('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2),
                               ('press-target', 'vpn-tile'), ('release',), ('sleep', 1.0)],
                              [('ui', 'nacre', 'close'), ('nm', 'reset', 'now')], 0),
    'control-vpn-hover': ([('nm', 'connect', 'home'), ('sleep', 1.5), ('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2),
                           ('press-target', 'vpn-tile'), ('release',), ('sleep', 1.0), ('pointer-target', 'vpn-row')],
                          [('release',), ('ui', 'nacre', 'close'), ('nm', 'reset', 'now')], 0),
    'control-vpn-pressed': ([('nm', 'connect', 'home'), ('sleep', 1.5), ('ui', 'nacre', 'controls', 'home'), ('sleep', 1.2),
                             ('press-target', 'vpn-tile'), ('release',), ('sleep', 1.0), ('press-target', 'vpn-row')],
                            [('release',), ('ui', 'nacre', 'close'), ('nm', 'reset', 'now')], 0),
    'control-center-vpn': ([('nm', 'connect', 'home'), ('sleep', 1.5), ('ui', 'nacre', 'controls', 'home')],
                           [('ui', 'nacre', 'close'), ('nm', 'reset', 'now')], 1.2),
    'notification': ([('notify', 'Parity check', 'The same notification in both shells.'), ('sleep', 1.5)], [('ui', 'nacre', 'close')], 1.2),
    'osd-volume': ([('only', 'qml', ('ui', 'nacre', 'previewFeedback', 'volume')), ('only', 'bead', ('level', 'sink', 0.62))],
                   [('ui', 'nacre', 'close')], 0),
    'osd-microphone': ([('only', 'qml', ('ui', 'nacre', 'previewFeedback', 'microphone')), ('only', 'bead', ('level', 'source', 0.45))],
                       [('ui', 'nacre', 'close')], 0),
    'osd-brightness': ([('only', 'qml', ('qml-osd-preview', 'display')),
                        ('only', 'bead', ('bead-osd-preview', 'brightness', 'display'))],
                       [('ui', 'nacre', 'close')], 0),
    'osd-keyboard': ([('only', 'qml', ('qml-osd-preview', 'keyboard')),
                      ('only', 'bead', ('bead-osd-preview', 'keyboard', 'keyboard'))],
                     [('ui', 'nacre', 'close')], 0),
    'session': ([('ui', 'nacre', 'session')],
                [('ui', 'nacre', 'close')], 1.0),
    'settings-scroll': ([('ui', 'nacre', 'settings'), ('sleep', 1.5), ('scroll-proof',)], [('app_close', 'settings')], 0),
    'settings': ([('ui', 'nacre', 'settings'), ('sleep', 1.5)], [('app_close', 'settings')], 0),
    # B0064: the Colors app, the Welcome colours page and the app-theming targets (generated wallpapers, see --wallpapers)
    'colors-app': ([('only', 'qml', ('qml_app', 'colorsApp', 'open', '')), ('only', 'bead', ('shell', 'colorsApp', 'open')), ('sleep', 6.0)],
    [('app_close', 'colors')], 0),
    # B0133: every Welcome page shares the same private palette and compositor geometry.
    'welcome-hero': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'home')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'home'))], [('app_close', 'welcome')], 0),
    'welcome-shortcuts': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'shortcuts')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'shortcuts'))], [('app_close', 'welcome')], 0),
    'welcome-apps': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'apps')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'apps'))], [('app_close', 'welcome')], 0),
    'welcome-ready': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'ready')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'ready'))], [('app_close', 'welcome')], 0),
    'welcome-help': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'help')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'help'))], [('app_close', 'welcome')], 0),
    'welcome-colors': ([('only', 'qml', ('qml_app', 'welcomeApp', 'open', 'colors')), ('only', 'bead', ('ui', 'welcomeApp', 'open', 'colors')), ('sleep', 6.0)],
    [('app_close', 'welcome')], 0),
    'settings-colors-app-theming': ([('only', 'qml', ('qml_settings', 'colors/app-theming')), ('only', 'bead', ('shell', 'nacre', 'settings', 'colors/app-theming')), ('sleep', 3.0)],
    [('app_close', 'settings')], 0),
    # B0063: one state per Settings page, opened by deep link in both shells
    'settings-date-time-language': ([('only', 'qml', ('qml_settings', 'date-time-language')), ('only', 'bead', ('shell', 'nacre', 'settings', 'date-time-language')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-notifications': ([('only', 'qml', ('qml_settings', 'notifications')), ('only', 'bead', ('shell', 'nacre', 'settings', 'notifications')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-windows-workspaces-workflows': ([('only', 'qml', ('qml_settings', 'windows-workspaces/workflows')), ('only', 'bead', ('shell', 'nacre', 'settings', 'windows-workspaces/workflows')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-lock-login': ([('only', 'qml', ('qml_settings', 'lock-login')), ('only', 'bead', ('shell', 'nacre', 'settings', 'lock-login')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-privacy': ([('only', 'qml', ('qml_settings', 'privacy')), ('only', 'bead', ('shell', 'nacre', 'settings', 'privacy')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-keyboard-mouse': ([('only', 'qml', ('qml_settings', 'keyboard-mouse')), ('only', 'bead', ('shell', 'nacre', 'settings', 'keyboard-mouse')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-power': ([('only', 'qml', ('qml_settings', 'power')), ('only', 'bead', ('shell', 'nacre', 'settings', 'power')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-system': ([('only', 'qml', ('qml_settings', 'system')), ('only', 'bead', ('shell', 'nacre', 'settings', 'system')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-nacre-ai': ([('only', 'qml', ('qml_settings', 'nacre-ai')), ('only', 'bead', ('shell', 'nacre', 'settings', 'nacre-ai')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-addons': ([('only', 'qml', ('qml_settings', 'addons')), ('only', 'bead', ('shell', 'nacre', 'settings', 'addons')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-sound': ([('only', 'qml', ('qml_settings', 'sound')), ('only', 'bead', ('shell', 'nacre', 'settings', 'sound')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-network': ([('only', 'qml', ('qml_settings', 'network')), ('only', 'bead', ('shell', 'nacre', 'settings', 'network')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'settings-bluetooth': ([('only', 'qml', ('qml_settings', 'bluetooth')), ('only', 'bead', ('shell', 'nacre', 'settings', 'bluetooth')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    # B0089: the Displays page, idle and with a display change awaiting Keep (a journal file only; no output is touched)
    'settings-displays': ([('only', 'qml', ('qml_settings', 'displays')), ('only', 'bead', ('shell', 'nacre', 'settings', 'displays')), ('sleep', 2.5)],
    [('app_close', 'settings')], 0),
    'settings-displays-keep': ([('journal', 'on'), ('only', 'qml', ('qml_settings', 'displays')), ('only', 'bead', ('shell', 'nacre', 'settings', 'displays')), ('sleep', 2.5)],
    [('app_close', 'settings'), ('journal', 'off')], 0),
    'settings-appearance': ([('only', 'qml', ('qml_settings', 'appearance')), ('only', 'bead', ('shell', 'nacre', 'settings', 'appearance')), ('sleep', 2.0)],
    [('app_close', 'settings')], 0),
    'sidebar-hover': ([('ui', 'nacre', 'left'), ('sleep', 0.8), ('pointer', 80, 342)], [('release',), ('ui', 'nacre', 'left')], 0),
    'sidebar-pressed': ([('ui', 'nacre', 'left'), ('sleep', 0.8), ('press', 80, 342)], [('release',), ('ui', 'nacre', 'left')], 0),
    'sidebar-conversation': ([('ui', 'nacre', 'left'), ('chat-seed',), ('sleep', 1.0)], [('ui', 'nacre', 'left')], 0),
    'sidebar-pinned': ([('ui', 'nacre', 'left'), ('chat-seed',), ('sleep', 1.0), ('pin',), ('click', 1600, 600), ('sleep', 0.3)], [('pin',), ('click', 1600, 600), ('sleep', 0.6)], 0),
    'sidebar': ([('ui', 'nacre', 'left'), ('sleep', 1.0)], [('only', 'qml', ('ui', 'nacre', 'close')), ('only', 'bead', ('ui', 'nacre', 'left'))], 1.2),
    # B0093: the selector prompt (`nacre-menu pick|input`), the same request through the same helper in both shells
    'menu-pick': ([('prompt', 'pick'), ('sleep', 0.2)], [('prompt_end',)], 1.0),
    'menu-pick-hover': ([('prompt', 'pick'), ('sleep', 1.5), ('pointer-target', 'prompt-row')], [('prompt_end',)], 0),
    'menu-input': ([('prompt', 'input'), ('sleep', 0.2)], [('prompt_end',)], 1.0),
    'pearl-detached': ([('ui', 'nacre', 'launcher', ''), ('sleep', 1.2), ('ui', 'pearls', 'detach')], [('ui', 'nacre', 'close')], 0),
}
# B0095: the microphone muted (the meter is off in both shells and QML shows no peak-monitor card of its own), and Bead's meter at 60 %
# (QML's cannot follow a level in the private PipeWire, see fake_system.PEAK).
_MUTE_CLOSE = [('ui', 'nacre', 'close'), ('mute', 'source', False), ('level', 'peak', 0.0)]
STATES.update({
    'popout-audio-muted': ([('mute', 'source', True), ('sleep', 1.2), ('ui', 'nacre', 'popout', 'audio', '1520')], _MUTE_CLOSE, 0),
    'control-audio-muted': ([('mute', 'source', True), ('sleep', 1.2), ('ui', 'nacre', 'controls', 'audio')], _MUTE_CLOSE, 0),
    'popout-audio-peak': ([('only', 'bead', ('level', 'peak', 0.6)), ('sleep', 1.2), ('ui', 'nacre', 'popout', 'audio', '1520'), ('sleep', 0.6)],
                          _MUTE_CLOSE, 0),
})
STATE_PREPARE['popout-audio-muted'] = STATE_PREPARE['popout-audio-peak'] = STATE_PREPARE['popout-audio']
# B0095 (need --fake-wifi, and --pointer-tool for the scan clicks): the Wi-Fi list idle, while a scan runs (the fake takes 2 s) and
# after it, in the bar popout and the docked page. The scan button is clicked in both shells; two more networks appear.
PTR['wifi-scan'] = {'qml': (1621, 82), 'bead': (1621, 82)}
PTR['wifi-switch-docked'] = {'qml': (1869, 131), 'bead': (1869, 131)}
PTR['wifi-scan-docked'] = {'qml': (1567, 131), 'bead': (1567, 131)}
_SCAN_CLOSE = [('ui', 'nacre', 'close'), ('nm', 'reset', 'now')]
STATES.update({
    'popout-network-wifi': ([('ui', 'nacre', 'popout', 'network', '1560')], [('ui', 'nacre', 'close')], 0),
    'popout-network-scanning': ([('ui', 'nacre', 'popout', 'network', '1560'), ('sleep', 1.2), ('pointer-target', 'wifi-scan'), ('sleep', 0.5), ('press-target', 'wifi-scan'), ('sleep', 0.15), ('release',)],
                                _SCAN_CLOSE, 0),
    # The scan is asked of the fake NetworkManager directly (a click on the button is not delivered in the nested session, see
    # the scanning states): both shells then show the two networks that appear.
    'popout-network-scanned': ([('nm', 'scan', 'now'), ('sleep', 3.0), ('ui', 'nacre', 'popout', 'network', '1560')], _SCAN_CLOSE, 0),
    'control-network-wifi': ([('ui', 'nacre', 'controls', 'network')], [('ui', 'nacre', 'close')], 0),
    'control-network-ptrtest': ([('ui', 'nacre', 'controls', 'network'), ('sleep', 1.2), ('press-target', 'wifi-scan-docked'), ('sleep', 0.3)],
                                [('release',), ('ui', 'nacre', 'close')], 0),
    'control-network-probe': ([('nmcli-probe',), ('ui', 'nacre', 'controls', 'network'), ('sleep', 1.5)], [('nm', 'reset', 'now')], 0),
    'control-network-scanning': ([('ui', 'nacre', 'controls', 'network'), ('sleep', 1.2), ('pointer-target', 'wifi-scan-docked'), ('sleep', 0.5), ('press-target', 'wifi-scan-docked'), ('sleep', 0.15), ('release',)],
                                 _SCAN_CLOSE, 0),
    'control-network-scanned': ([('nm', 'scan', 'now'), ('sleep', 3.0), ('ui', 'nacre', 'controls', 'network')], _SCAN_CLOSE, 0),
})
STATE_SETTLE.update({'popout-network-scanning': 0.6, 'control-network-scanning': 0.6})
for _bar_cycle in range(1, 4):
    # Identical forced opens in control/candidate; title hover intentionally differs before the fix.
    STATES[f'bar-memory-{_bar_cycle}'] = ([('ui', 'drawers', 'toggle', 'dashboard')],
                                        [('ui', 'nacre', 'close'), ('sleep', 1.5)], 0)
# Preference helper work belongs before the animation trigger, so frame sequences
# measure the panel opening rather than waiting for a preferences write.
for _picker_state, (_opens, _closes, _seconds) in list(STATES.items()):
    if _picker_state.startswith('wallpaper-') and _opens and _opens[0][0] == 'wallpaper-pref':
        STATE_PREPARE[_picker_state] = [_opens[0]]
        STATES[_picker_state] = (_opens[1:], _closes, _seconds)

# Where each transition happens, in logical pixels ("x,y wxh"): a grim region is much faster than the whole output, which
# is what gives a usable frame rate for the animation timing.
REGIONS = {
    'launcher': '340,520 1240x680', 'dashboard-0': '300,0 1320x760', 'dashboard-warm': '300,0 1320x760', 'popout-calendar': '1250,0 670x520',
    'popout-network': '1250,0 670x520', 'control-center': '1360,0 560x760', 'notification': '1300,0 620x420',
    'session': '560,260 800x680', 'sidebar': '0,0 760x1200', 'menu-pick': '340,520 1240x680', 'menu-input': '340,520 1240x680',
    'wallpaper-carousel': '250,830 1420x370', 'wallpaper-dynamic': '250,830 1420x370',
}
NIRI_CONFIG = '''input {
    keyboard {
        xkb {
            layout "us"
        }
    }
}
prefer-no-csd
hotkey-overlay {
    skip-at-startup
}
animations {
    off
}
'''


# B0077: close controls exercise the same semantic hover/press state at each
# shell's bottom attachment (B0079).
for _name in ("overview", "palette", "keybindings"):
    _overview = _name == "overview"
    _target = "extras-close-" + _name
    PTR[_target] = {"qml": (1400 if _overview else 1320, 419 if _overview else 659),
                    "bead": (1400 if _overview else 1320, 419 if _overview else 659)}
    _open, _close, _seconds = STATES[_name]
    STATES[_name + "-hover"] = (_open + [("sleep", 1.0), ("pointer-target", _target)], _close, 0)
    STATES[_name + "-pressed"] = (_open + [("sleep", 1.0), ("press-target", _target)], [("release",)] + _close, 0)
    if _overview:
        for _state in (_name, _name + "-hover", _name + "-pressed"):
            STATE_PREPARE[_state] = [("ui", "nacre", "settings"), ("sleep", 2.0)]
            STATE_CLEANUP[_state] = [("app_close", "settings")]


def log(message):
    print(time.strftime('%H:%M:%S ') + message, flush=True)


def stop(process, grace=8):
    """Stop one process this script started (exact PID, its own group), politely first."""
    if process is None or process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=grace)
    except (subprocess.TimeoutExpired, ProcessLookupError):
        try:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            pass


def wait_until(check, timeout, what):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise RuntimeError('timeout waiting for ' + what)


# ---------------------------------------------------------------- one arm (runs inside dbus-run-session)

class Arm:
    def __init__(self, args, name, root):
        self.args, self.name, self.root = args, name, Path(root)
        self.procs = []
        self.out = Path(args.out).resolve() / name
        self.out.mkdir(parents=True, exist_ok=True)
        self.logs = self.root / 'logs'
        self.logs.mkdir(exist_ok=True)
        self.runtime = self.root / 'r'
        self.env = None

    def spawn(self, command, name, env=None, cwd=None):
        process = subprocess.Popen(command, env=env or self.env, cwd=cwd or self.root,
                                   stdout=(self.logs / (name + '.log')).open('w'), stderr=subprocess.STDOUT,
                                   start_new_session=True)
        self.procs.append(process)
        return process

    def start_compositor(self):
        a = self.args
        # The parent compositor also gets a private HOME/XDG tree, before it starts.
        for name in ['home', 'data', 'config', 'state/nacre', 'cache']:
            (self.root / name).mkdir(mode=0o700, parents=True, exist_ok=True)
        self.env.update(HOME=str(self.root / 'home'), XDG_DATA_HOME=str(self.root / 'data'),
                        XDG_CONFIG_HOME=str(self.root / 'config'), XDG_STATE_HOME=str(self.root / 'state'),
                        XDG_CACHE_HOME=str(self.root / 'cache'))
        parent_runtime = self.root / 'pr'
        parent_runtime.mkdir(mode=0o700)
        config = self.root / 'labwc'
        config.mkdir()
        (config / 'autostart').write_text('')
        width, height = a.size
        (config / 'rc.xml').write_text(
            '<labwc_config><windowRules><windowRule identifier="*" serverDecoration="no">'
            f'<action name="ResizeTo" width="{width}" height="{height}"/><action name="MoveTo" x="0" y="0"/>'
            '</windowRule></windowRules></labwc_config>')
        parent_env = dict(self.env, XDG_RUNTIME_DIR=str(parent_runtime), WLR_BACKENDS='headless',
                          WLR_HEADLESS_OUTPUTS='1', WLR_RENDERER='gles2')
        parent_env.pop('WAYLAND_DISPLAY', None)
        parent = self.spawn(['labwc', '--config-dir', str(config)], 'labwc', parent_env)
        socket = wait_until(lambda: next((f for f in parent_runtime.glob('wayland-*')
                                          if stat.S_ISSOCK(f.stat().st_mode)), None), 20, 'labwc socket')
        if a.pointer_tool:
            if not a.parent_output_tool:
                raise RuntimeError('pointer captures require --parent-output-tool (wlr-randr)')
            parent_control = dict(parent_env, WAYLAND_DISPLAY=str(socket))
            outputs = json.loads(self.run([a.parent_output_tool, '--json'], env=parent_control).stdout)
            parent_name = outputs[0]['name']
            self.run([a.parent_output_tool, '--output', parent_name, '--custom-mode',
                      f'{width}x{height}@60Hz'], env=parent_control)
            outputs = json.loads(self.run([a.parent_output_tool, '--json'], env=parent_control).stdout)
            mode = next(mode for mode in outputs[0]['modes'] if mode.get('current'))
            if (mode['width'], mode['height']) != (width, height) or outputs[0]['scale'] != 1:
                raise RuntimeError('private parent output does not match pointer coordinates')
            (self.out / 'parent-geometry.json').write_text(json.dumps(outputs))
        niri_config = self.root / 'niri.kdl'
        # Use the shipped floating app rules in both private arms, including Bead app IDs.
        rules = Path(a.nacre).resolve() / 'niri/window-rules.kdl'
        config_text = NIRI_CONFIG + ('include ' + json.dumps(str(rules)) + '\n' if rules.is_file() else '')
        if getattr(a, "shortcut_fixture", False):
            # Real private bindings, read by BOTH desktop-extras keys and shortcuts.refresh.
            # Their actions are harmless even if a key is accidentally sent during capture.
            bindings = self.root / 'fixture-bindings.kdl'
            bindings.write_text('// Generated from compositor-contract.json; private parity fixture.\nbinds {\n'
                                '    Super+Tab repeat=false hotkey-overlay-title="Nacre:overview" { spawn-sh "true"; }\n'
                                '    Super+Ctrl+Space repeat=false hotkey-overlay-title="Nacre:palette" { spawn-sh "true"; }\n'
                                '    Super+K repeat=false hotkey-overlay-title="Nacre:shortcuts" { spawn-sh "true"; }\n}\n')
            config_text += 'include ' + json.dumps(str(bindings)) + '\n'
        niri_config.write_text(config_text)
        self.env['NACRE_NIRI_CONFIG'] = str(niri_config)
        env = dict(self.env, WAYLAND_DISPLAY=str(socket))
        for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY']:
            env.pop(key, None)
        self.spawn(['niri', '--config', str(niri_config)], 'niri', env)
        display = wait_until(lambda: next((f for f in self.runtime.glob('wayland-*')
                                           if stat.S_ISSOCK(f.stat().st_mode)), None), 20, 'niri display')
        niri_socket = wait_until(lambda: next(iter(self.runtime.glob('niri.*.sock')), None), 20, 'niri socket')
        self.env.update(WAYLAND_DISPLAY=str(display), NIRI_SOCKET=str(niri_socket), BEAD_PARENT=str(socket))
        self.env.pop('DISPLAY', None)
        outputs = json.loads(self.run(['niri', 'msg', '-j', 'outputs']).stdout)
        name = next(iter(outputs))
        self.run(['niri', 'msg', 'output', name, 'scale', str(a.scale)])
        time.sleep(1)
        outputs = json.loads(self.run(['niri', 'msg', '-j', 'outputs']).stdout)
        output = outputs[name]
        mode = output['modes'][output['current_mode']]
        self.geometry = {'name': name, 'mode': [mode['width'], mode['height']], 'scale': output['logical']['scale'],
                         'logical': [output['logical']['width'], output['logical']['height']]}
        (self.out / 'geometry.json').write_text(json.dumps(self.geometry))
        log(f'{self.name}: output {self.geometry}')

    def run(self, command, timeout=15, check=True, env=None):
        return subprocess.run(command, env=env or self.env, capture_output=True, text=True, timeout=timeout, check=check)

    def call(self, method, params, timeout=3):
        reply = subprocess.run([self.nacrectl, 'call', '--timeout', str(timeout * 1000), method, json.dumps(params)],
                               env=self.env, capture_output=True, text=True, timeout=timeout + 2)
        if reply.returncode:
            raise RuntimeError(f'daemon call failed: {method} {reply.stderr.strip()[:200]}')
        return json.loads(reply.stdout) if reply.stdout.strip() else None

    def install_chat_fixture(self):
        # Reuse Nacre's synthetic provider, without copying a production helper or making network calls.
        source = (Path(self.args.nacre) / 'crates/nacre-helper/tests/sidebar_chat.rs').read_text()
        fake = re.search(r'const FAKE_CODEX: &str = r##"(.*?)"##;', source, re.S).group(1)
        folder = self.root / 'home/.local/bin'
        folder.mkdir(parents=True, exist_ok=True)
        for name, body in {'codex': fake, 'nacre-ai': '#!/bin/sh\nexit 0\n', 'nacre-ai-chat': '#!/bin/sh\nexit 0\n'}.items():
            path = folder / name
            path.write_text(body)
            path.chmod(0o700)
        config = self.root / 'config/nacre/ai'
        config.mkdir(parents=True, exist_ok=True)
        (config / 'settings.json').write_text('{"default_assistant":"codex","version":1}')
        (config.parent / 'profile.json').write_text('{"version":1,"workspaceCount":6,"components":["configs","shell","ai"]}')
        for name in ('PROVIDER_LOG', 'TURN_STATE', 'PROVIDER_COUNT'):
            self.env['FAKE_' + name] = str(self.root / name.lower())
        self.env['FAKE_SCENARIO'] = 'workspace'

    def start_daemon(self):
        a = self.args
        home = self.root / 'home'
        for name in ['home', 'data', 'config', 'state/nacre', 'cache']:
            (self.root / name).mkdir(mode=0o700, parents=True, exist_ok=True)
        (self.runtime / 'nacre').mkdir(mode=0o700, exist_ok=True)
        for part in ['shell', 'shell/notifications', 'shell/notifications/images']:
            (self.root / 'state/nacre' / part).mkdir(mode=0o700, exist_ok=True)
        self.endpoint = self.runtime / 'nacre/daemon.sock'
        self.nacrectl = str(self.root / 'nacrectl')
        os.symlink(Path(a.bead_bin).resolve() / 'nacre-helper', self.nacrectl)
        self.env.update(HOME=str(home), XDG_DATA_HOME=str(self.root / 'data'), XDG_CONFIG_HOME=str(self.root / 'config'),
                        XDG_STATE_HOME=str(self.root / 'state'), XDG_CACHE_HOME=str(self.root / 'cache'),
                        NACRE_DAEMON_SOCKET=str(self.endpoint), NACRE_COMPOSITOR='niri', NACRE_NIRI_HOVER_STREAM='0',
                        NACRE_HELPER_TOOLS=str(Path(a.nacre).resolve() / 'nacre/shell-tools'),
                        NACRE_HELPER_BINARY=str(Path(a.bead_bin).resolve() / 'nacre-helper'),
                        DBUS_SYSTEM_BUS_ADDRESS=self.env['DBUS_SESSION_BUS_ADDRESS'],
                        BEAD_PRIVATE_INPUT_PROOF='1', PIPEWIRE_REMOTE='bead-private-unavailable', PULSE_SERVER='unix:' + str(self.root / 'no-pulse'),
                        GIO_USE_VFS='local', GVFS_DISABLE_FUSE='1', QT_NO_XDG_DESKTOP_PORTAL='1', GTK_USE_PORTAL='0')
        if any(name.startswith('sidebar') for name in a.states):
            self.install_chat_fixture()
        record = Path(a.presentation).expanduser() if a.presentation else None
        if record and record.is_file() and not a.wallpaper_fixture:  # read-only copy of the owner's record: same wallpaper and palette on both arms
            shutil.copyfile(record, self.root / 'state/nacre/presentation.json')
            (self.root / 'state/nacre/presentation.json').chmod(0o600)
        if a.wallpapers:  # generated gradients: the library both arms list in the Colors app
            from PIL import Image
            folder = home / 'Pictures/Wallpapers'
            folder.mkdir(parents=True, exist_ok=True)
            for index in range(a.wallpapers):
                width, height = 1600, 900
                image = Image.new('RGB', (width, height))
                pixels = image.load()
                for y in range(height):
                    for x in range(width):
                        pixels[x, y] = ((x * 255 // width + index * 70) % 256, (y * 255 // height * (index + 1)) % 256,
                                        ((x + y) * 255 // (width + height) + index * 40) % 256)
                image.save(folder / f'generated-{index + 1}.png')
        if a.wallpaper_videos:
            folder = home / 'Pictures/Wallpapers/Dynamic'
            folder.mkdir(parents=True, exist_ok=True)
            for index in range(a.wallpaper_videos):
                self.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
                          f'testsrc2=size=640x360:rate=12:duration=2', '-an', '-c:v', 'libx264',
                          '-threads', '1', '-pix_fmt', 'yuv420p', '-y', str(folder / f'motion-{index + 1}.mp4')], timeout=30)
        if a.wallpaper_fixture:
            helper = str(Path(a.bead_bin).resolve() / 'nacre-helper')
            fixture_image = home / 'Pictures/Wallpapers/generated-1.png'
            from PIL import Image
            with Image.open(fixture_image) as image:
                image.verify()
            log(f'fixture image verified: {fixture_image.name}, {fixture_image.stat().st_size} bytes')
            # Bootstrap before the private daemon starts. An explicit absent
            # endpoint disables the helper's native private-compositor fallback.
            fixture_env = dict(self.env)
            fixture_env.pop('NACRE_DAEMON_SOCKET', None)
            fixture_env.pop('NACRE_DAEMON_CLIENT', None)
            try:
                self.run([helper, 'wallpaper-media', 'select', str(fixture_image)], timeout=45, env=fixture_env)
            except subprocess.CalledProcessError as error:
                raise RuntimeError('fixture wallpaper select failed: ' + (error.stdout + '\n' + error.stderr).strip()[-3000:]) from error
            if not (self.root / 'state/nacre/presentation.json').is_file():
                raise RuntimeError('fixture wallpaper did not produce a private presentation')
        # No audio server here: stand-ins make the daemon see the fixed machine of spikes/fake_system.py (2 sinks, 1 source,
        # 45 %); the files hold the default levels the `level` step changes.
        fake = self.root / 'fakeaudio'
        fake.mkdir()
        (fake / 'events').write_text('')
        (fake / 'sink').write_text(f'{fake_system.DEFAULT_LEVEL}\n')
        (fake / 'source').write_text(f'{fake_system.DEFAULT_LEVEL}\n')
        (fake / 'pactl').write_text(f'#!/bin/sh\nFAKE_AUDIO_DIR={fake} exec {sys.executable} {HERE / "fake_system.py"} "$@"\n')
        (fake / 'wpctl').write_text(
            f'#!/bin/sh\ncase "$1" in\nget-volume) case "$2" in *SOURCE*) n=source;; *) n=sink;; esac; echo "Volume: $(cat {fake}/$n)";;\n'
            'inspect) case "$2" in *SOURCE*) echo "  node.name = \\"fixture-microphone\\"";; *) echo "  node.name = \\"fixture-speakers\\"";; esac;;\nesac\n')
        # B0095: the daemon's `audio.peak` capture (parec) plays silence (or the `level peak` step's level) instead of opening the microphone.
        (fake / 'parec').write_text(f'#!/bin/sh\nFAKE_AUDIO_DIR={fake} exec {sys.executable} {HERE / "fake_system.py"} --parec\n')
        for name in ['pactl', 'wpctl', 'parec']:
            (fake / name).chmod(0o755)
        self.env['PATH'] = f'{fake}:' + self.env['PATH']
        # The fixed battery and power profiles (UPower, power-profiles-daemon) on the same private bus.
        self.spawn([sys.executable, str(HERE / 'fake-power.py')], 'fake-power')
        wait_until(lambda: self._try(lambda: self.run(['gdbus', 'call', '--session', '--dest', 'org.freedesktop.UPower', '--object-path',
                                                       '/org/freedesktop/UPower', '--method', 'org.freedesktop.UPower.GetDisplayDevice'])),
                   20, 'the fake UPower')
        # A fake org.bluez on the private bus (the "system" bus of both shells): the same adapter and devices on both arms.
        self.spawn([sys.executable, str(HERE / 'fake-bluez.py')], 'fake-bluez')
        wait_until(lambda: self._try(lambda: self.run(['gdbus', 'call', '--session', '--dest', 'org.bluez', '--object-path', '/',
                                                       '--method', 'org.freedesktop.DBus.ObjectManager.GetManagedObjects'])),
                   20, 'the fake org.bluez')
        self.start_pipewire()  # QML reads it directly; the Nacre daemon only offers audio.set_volume while its socket exists (B0091)
        if a.fake_nm:
            # B0087: the owner's saved file holds the complete default tile list (QML NacreControlPreferences.defaults, VPN
            # included); a fresh private profile would otherwise give Bead the registry's shorter list. Both arms read this file.
            saved = self.root / 'config/nacre'
            saved.mkdir(parents=True, exist_ok=True)
            (saved / 'control-center.json').write_text(json.dumps({'version': 2, 'defaultsRevision': 2, 'tiles': [
                'volume', 'mic-volume', 'brightness', 'keyboard', 'network', 'bluetooth', 'airplane', 'vpn', 'battery', 'awake', 'dnd',
                'mic', 'night', 'appearance', 'screenshot', 'record', 'picker', 'clipboard', 'updates', 'displays', 'wallpaper',
                'next-wallpaper', 'notifications']}))
            # B0087: a fake NetworkManager on the same private bus; the QML arm's nmcli is a stub that talks to it.
            self.spawn([sys.executable, str(HERE / 'fake-nm.py')] + (['--wifi'] if a.fake_wifi else []), 'fake-nm')
            wait_until(lambda: self._try(lambda: self.run(['gdbus', 'call', '--session', '--dest', 'org.freedesktop.NetworkManager',
                                                           '--object-path', '/org/freedesktop/NetworkManager/Settings',
                                                           '--method', 'org.freedesktop.NetworkManager.Settings.ListConnections'])),
                       20, 'the fake NetworkManager')
            stub = self.root / 'fakenm'
            stub.mkdir()
            (stub / 'nmcli').symlink_to(HERE / 'fake-nmcli.py')
            self.env['PATH'] = f'{stub}:' + self.env['PATH']
        command = [str(Path(a.bead_bin).resolve() / 'nacre-helper'), 'daemon']
        if self.name == 'bead':
            command.append('--notifications=daemon')
        else:
            self.env['NACRE_DAEMON_CLIENT'] = '1'
        self.daemon = self.spawn(command, 'daemon')
        wait_until(lambda: self._try(lambda: self.call('daemon.status', {})), 30, 'the private daemon')
        if any(state.startswith('bar-') for state in a.states):
            self.spawn([sys.executable, str(HERE / 'fake-sni.py')], 'fake-sni')
            wait_until(lambda: 'registered two' in (self.logs / 'fake-sni.log').read_text(), 20, 'the two private SNI items')

    def start_pipewire(self):
        """QML reads audio through PipeWire, not pactl (the daemon reads pactl but needs the socket to offer volume writes): each arm gets a private server with the same nodes (spikes/fake_system.py),
        the same default sink/source and the same level (PipeWire stores volume cubed, as pactl's percentage does)."""
        conf = self.root / 'config/pipewire/pipewire.conf.d'
        conf.mkdir(parents=True, exist_ok=True)
        (conf / '10-fixture.conf').write_text(fake_system.pipewire_conf())
        self.env['PIPEWIRE_REMOTE'] = 'pipewire-0'
        self.spawn(['pipewire'], 'pipewire')
        wait_until(lambda: self._try(lambda: self.run(['pw-metadata', '-n', 'default'])), 20, 'the private PipeWire')
        for key, value in fake_system.default_metadata():
            self.run(['pw-metadata', '-n', 'default', '0', key, value, 'Spa:String:JSON'])
        nodes = {o['info']['props'].get('node.name'): o['id'] for o in json.loads(self.run(['pw-dump']).stdout)
                 if o.get('type') == 'PipeWire:Interface:Node'}
        cubed = fake_system.DEFAULT_LEVEL ** 3
        for name in [n for _, n, _ in fake_system.SINKS + fake_system.SOURCES]:
            channels = '[ %s ]' % ' '.join([f'{cubed:.6f}'] * (1 if 'microphone' in name else 2))
            self.run(['pw-cli', 's', str(nodes[name]), 'Props', '{ mute: false, channelVolumes: %s }' % channels])
        # B0095: the application streams (spikes/fake_system.py STREAMS) as unlinked pw-cat nodes with a name, an icon and a volume.
        # One at a time, so the node ids (and with them the order Quickshell lists them in) are the same in every run.
        commands = fake_system.stream_commands()
        for count, (name, command, volume) in enumerate(commands, 1):
            self.spawn(command, name)
            wait_until(lambda: len([o for o in json.loads(self.run(['pw-dump']).stdout) if o.get('type') == 'PipeWire:Interface:Node'
                                    and str(o['info']['props'].get('node.name', '')).startswith('fixture-stream-')]) == count,
                       20, 'the stream node ' + name)
        # pw-cat resets a stream's volume while it connects: set it again until pw-dump shows it.
        def volumes_set():
            dump = json.loads(self.run(['pw-dump']).stdout)
            streams = {o['info']['props']['node.name']: o for o in dump if o.get('type') == 'PipeWire:Interface:Node'
                       and str(o['info']['props'].get('node.name', '')).startswith('fixture-stream-')}
            done = True
            for name, _, volume in commands:
                node = streams['fixture-stream-' + name.split('-')[1]]
                have = [p.get('channelVolumes') for p in node['info'].get('params', {}).get('Props', [])]
                if not any(h and abs(h[0] - volume) < 1e-4 for h in have):
                    done = False
                    self.run(['pw-cli', 's', str(node['id']), 'Props', '{ mute: false, channelVolumes: [ %.6f, %.6f ] }' % (volume, volume)])
            return done
        wait_until(lambda: time.sleep(0.5) or volumes_set(), 20, 'the stream volumes')

    @staticmethod
    def _try(function):
        try:
            return function() or True
        except Exception:
            return None

    def start_shell(self):
        a = self.args
        if self.name == 'qml':
            nacre_shell = Path(a.nacre).resolve() / 'nacre/shell'
            # The QML shell finds its helper tools and assets under the installed layout.
            base = Path(self.env['HOME']) / '.local/share/nacre/shell'
            base.mkdir(parents=True)
            (base / 'source').symlink_to(nacre_shell)
            tools = base / 'tools'
            tools.mkdir()
            for entry in (Path(a.nacre).resolve() / 'nacre/shell-tools').iterdir():
                (tools / entry.name).symlink_to(entry)
            for name in ['nacre-helper', 'nacre-daemon']:
                (tools / name).symlink_to(Path(a.bead_bin).resolve() / 'nacre-helper')
            # OSD-only descriptors must not invent controls in unrelated dock captures.
            if any(state in a.states for state in ('osd-brightness', 'osd-keyboard')):
                self.install_fake_light_devices(tools / 'nacre-helper')
            (base / 'bin').mkdir()
            (base / 'bin/qs').symlink_to(shutil.which('quickshell'))
            local_bin = Path(self.env['HOME']) / '.local/bin'
            local_bin.mkdir(parents=True, exist_ok=True)
            for name in ['nacrectl', 'nacre-menu']:
                (local_bin / name).symlink_to(Path(a.bead_bin).resolve() / 'nacre-helper')
            self.env['PATH'] = f'{local_bin}:{base / "bin"}:' + self.env['PATH']
            self.qml_entry = nacre_shell / 'shell.qml'
            if 'settings-scroll' in a.states:
                probe = self.root / 'qml-probe'
                probe.mkdir()
                for entry in nacre_shell.iterdir():
                    if entry.name != 'shell.qml':
                        (probe / entry.name).symlink_to(entry)
                handler = r'''IpcHandler {
                    target: "b0098Probe"
                    function position(): string {
                        function find(item) {
                            if (!item) return null;
                            if (item.objectName === "registryScroll") return item;
                            for (const child of item.children || []) { const value = find(child); if (value) return value; }
                            return null;
                        }
                        const scroll = find(NacreSettingsApp.window?.view);
                        return JSON.stringify({position: scroll?.contentY, extent: scroll?.contentHeight, height: scroll?.height});
                    }
                }'''
                self.qml_entry = probe / 'shell.qml'
                self.qml_entry.write_text('import Quickshell.Io\n' + (nacre_shell / 'shell.qml').read_text().replace('ShellRoot {', 'ShellRoot {\n' + handler, 1))
            command = ['quickshell', '--path', str(self.qml_entry)]
            self.shell = self.spawn(command, 'shell', cwd=nacre_shell)
        else:
            root = self.root / 'bead'
            shutil.copytree(a.bead_shell, root / 'shell')
            # Pin the same read-only presentation inputs as QML even when this
            # private daemon does not advertise wallpaper.active.
            presentation = self.root / 'state/nacre/presentation.json'
            palette = root / 'palette.json'
            if presentation.is_file():
                record = json.loads(presentation.read_text())
                palette.write_text(json.dumps({'formatVersion': 1, 'mode': record['mode'], 'colours': record['colours']}))
                poster = record.get('poster')
                if poster and Path(poster).is_file():
                    background_tool = shutil.which('swaybg') or str(Path(a.bead_bin).resolve() / 'swaybg')
                    self.spawn([background_tool, '--output', self.geometry['name'], '--image', poster, '--mode', 'fill'], 'matched-wallpaper')
            else:
                palette = root / 'shell/palette.json'
            commands = self.runtime / 'bead-command.sock'
            self.env['NACRE_HELPER'] = str(Path(a.bead_bin).resolve() / 'nacre-helper')  # B0093: the prompt app answers through it
            command = [str(Path(a.bead_bin).resolve() / 'nacre-shell'), '--bar', '--core-views', '--ui-provider',
                       '--notices', '--daemon-socket', str(self.endpoint), '--command-socket', str(commands), '--palette', str(palette)]
            if a.cpu:
                command += ['--renderer', 'cpu', '--panel-renderer', 'cpu']
            self.shell = self.spawn(command, 'shell', cwd=root)
        if self.name == 'bead':
            wait_until(lambda: 'READY output=' in (self.logs / 'shell.log').read_text(errors='replace'), 40, 'Bead READY')

        def provider():
            if self.shell.poll() is not None:
                raise RuntimeError('the shell exited; see shell.log')
            return self._try(lambda: self.dispatch('nacre', 'state'))
        wait_until(provider, 60, 'the UI provider')
        if any(state.startswith('bar-') for state in a.states):
            self.spawn([self.nacrectl, 'watch', 'tray'], 'tray-snapshot')
        time.sleep(self.args.boot_settle)

    def install_fake_light_devices(self, path):
        """Supply read-only DDC-shaped descriptors so QML previews work without host backlights."""
        real = str(Path(self.args.bead_bin).resolve() / 'nacre-helper')
        wrapper = '''#!/usr/bin/env python3
import json, os, sys
real = {real!r}
args = sys.argv[1:]
if args[:2] == ["light-devices", "discover"]:
    screens = json.loads(args[2])
    screen = screens[0] if screens else "bead-parity"
    print(json.dumps({{"backlight": None,
        "keyboard": {{"kind": "ddc", "device": "fixture-keyboard", "screen": screen,
            "maximum": 100, "current": 42}},
        "ddc": [{{"kind": "ddc", "screen": screen, "bus": 0,
            "connector": "fixture", "identity": "bead-parity-fixture",
            "current": 68, "maximum": 100}}]}}))
    raise SystemExit(0)
if args[:1] == ["light-devices"]:
    if args[1:2] == ["read-ddc"]:
        descriptor = json.loads(args[2])
        print(json.dumps({{"current": descriptor["current"], "maximum": descriptor["maximum"]}}))
    else:
        print(json.dumps({{"error": "private parity light fixture is read-only"}}))
    raise SystemExit(0)
os.execv(real, [real, *args])
'''.format(real=real)
        Path(path).unlink()
        Path(path).write_text(wrapper)
        Path(path).chmod(0o700)

    def dispatch(self, target, method, *args):
        begin = time.monotonic()
        reply = self.call('ui.dispatch', {'target': target, 'method': method, 'args': list(args)})
        if hasattr(self, 'dispatch_trace'):
            self.dispatch_trace.append({'target': target, 'method': method, 'args': list(args),
                                        'start_ms': round((begin - self.state_start) * 1000, 3),
                                        'ack_ms': round((time.monotonic() - self.state_start) * 1000, 3)})
        if isinstance(reply, dict) and reply.get('err'):
            raise RuntimeError(str(reply['err'].get('message') or reply['err'])[:160])
        return reply

    @staticmethod
    def socket_call(path, target, method, args):
        """Newline-delimited JSON on a Bead command socket (the shell's CLI only knows catalogue routes)."""
        import socket
        request = json.dumps({'version': 1, 'id': 1, 'target': target, 'method': method, 'args': args}) + '\n'
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(3)
            client.connect(str(path))
            client.sendall(request.encode())
            data = b''
            while b'\n' not in data:
                part = client.recv(65536)
                if not part:
                    break
                data += part
            return json.loads(data)

    def app_close(self, name):
        if self.name == 'qml':
            self.run(['quickshell', 'ipc', '--path', str(getattr(self, 'qml_entry', Path(self.args.nacre).resolve() / 'nacre/shell/shell.qml')),
                      'call', name + 'App', 'close'], check=False)
        else:  # the app process listens on app-NAME.sock beside the shell's command socket
            try:
                self.socket_call(self.runtime / f'app-{name}.sock', 'bead.window', 'close', [])
            except OSError as error:
                self.errors.append(f'closing {name}: {error}')

    def welcome_ready(self, name):
        """A cold app must be mapped on the requested page before its parity shot."""
        page = 'welcome' if name == 'welcome-hero' else name.removeprefix('welcome-')
        def ready():
            try:
                state = self.socket_call(self.runtime / 'app-welcome.sock', 'bead.window', 'state', [])['result']
                windows = json.loads(self.run(['niri', 'msg', '-j', 'windows']).stdout)
                mapped = any(window.get('app_id') == 'nacre-welcome' for window in windows)
                return state if mapped and (state.get('stores') or {}).get('welcome.step') == page else None
            except (OSError, ValueError, KeyError, TypeError):
                return None
        state = wait_until(ready, 20, 'mapped Welcome page ' + page)
        time.sleep(0.2)  # allow the committed buffer to reach the nested compositor
        return {'size': state['size'], 'page': state['stores']['welcome.step'],
                'scroll': state['stores']['welcome.scroll']}

    def prompt_start(self, mode):
        """B0093: ask for a selection or a text through `menu-cli`, as `nacre-menu pick|input` does. The helper stays
        running (it waits for the answer) until `prompt_end` ends this exact PID."""
        helper = str(Path(self.args.bead_bin).resolve() / 'nacre-helper')
        tools = self.root / 'promptbin'
        tools.mkdir(exist_ok=True)
        if not (tools / 'nacrectl').exists():
            (tools / 'nacrectl').symlink_to(helper)
        env = dict(self.env, PATH=f'{tools}:' + self.env['PATH'])
        if mode == 'pick':
            command = [helper, 'menu-cli', 'pick', '--json', '--title', 'Choose a network', '--timeout', '120']
        else:
            command = [helper, 'menu-cli', 'input', '--title', 'Name this workspace', '--initial', 'Work', '--timeout', '120']
        self.prompt_proc = subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=subprocess.PIPE, text=True)
        if mode == 'pick':
            self.prompt_proc.stdin.write(PROMPT_CHOICES)
        self.prompt_proc.stdin.close()

    def prompt_end(self):
        """Stop the helper this harness started (SIGTERM: it cancels its own prompt and exits 1)."""
        process = getattr(self, 'prompt_proc', None)
        if process is None:
            return
        self.prompt_proc = None
        if process.poll() is not None:
            self.errors.append(f'the selector helper had already exited ({process.returncode}): {process.stderr.read()[:160]}')
            return
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
            self.errors.append('the selector helper ignored SIGTERM')
        process.stdout.close()
        process.stderr.close()

    def set_mute(self, node, muted):
        """B0095: mute or unmute the default sink/source in both views of the fixed machine (the daemon reads the pactl stand-in, Quickshell PipeWire)."""
        mark = self.root / f'fakeaudio/{node}-mute'
        mark.write_text('1\n') if muted else mark.unlink(missing_ok=True)
        with (self.root / 'fakeaudio/events').open('a') as events:
            events.write("Event 'change' on %s #0\n" % node)
        name = (fake_system.SINKS if node == 'sink' else fake_system.SOURCES)[0][1]
        nodes = {o['info']['props'].get('node.name'): o['id'] for o in json.loads(self.run(['pw-dump']).stdout)
                 if o.get('type') == 'PipeWire:Interface:Node'}
        channels = '[ %s ]' % ' '.join([f'{fake_system.DEFAULT_LEVEL ** 3:.6f}'] * (2 if node == 'sink' else 1))
        self.run(['pw-cli', 's', str(nodes[name]), 'Props', '{ mute: %s, channelVolumes: %s }' % ('true' if muted else 'false', channels)])

    def set_level(self, node, value):
        if node == 'peak':  # B0095: the level the daemon's parec stand-in plays (Bead's input meter; the QML meter cannot follow, see fake_system.PEAK)
            (self.root / 'fakeaudio/peak').write_text(f'{value}\n')
            return
        """The daemon's osd service polls `wpctl get-volume` after each `pactl subscribe` line (fake tools, see start_daemon)."""
        (self.root / f'fakeaudio/{node}').write_text(f'{value}\n')
        with (self.root / 'fakeaudio/events').open('a') as events:
            events.write(f"Event 'change' on {node} #1\n")

    def qml_osd_preview(self, channel):
        """Show the QML preview and read the same live level that NacreOsdModel uses."""
        shell = Path(self.args.nacre).resolve() / 'nacre/shell/shell.qml'
        def ipc(target, method):
            output = self.run(['quickshell', 'ipc', '--path', str(shell), 'call', target, method]).stdout.strip()
            value = json.loads(output)
            return json.loads(value) if isinstance(value, str) else value
        self.run(['quickshell', 'ipc', '--path', str(shell), 'call', 'nacre', 'previewFeedback', channel])
        time.sleep(0.2)
        if channel == 'display':
            state = ipc('brightness', 'state')
            monitor = next((item for item in state.get('monitors', []) if item.get('name') == self.geometry['name']), None)
            if monitor is None:
                monitor = next(iter(state.get('monitors', [])), {})
            value = monitor.get('brightness', 0.0)
            available = monitor.get('available', False)
            device = ''  # The nested parity session has one output, so QML detail is empty.
        else:
            state = ipc('keyboardLight', 'state')
            value = state.get('brightness', 0.0)
            available = state.get('available', False)
            device = ''
        if not available:
            raise RuntimeError(f'QML {channel} OSD preview unavailable despite private light fixture')
        levels_path = Path(self.args.out).resolve() / 'osd-preview-levels.json'
        levels = json.loads(levels_path.read_text()) if levels_path.exists() else {}
        levels[channel] = {'value': float(value), 'available': bool(available), 'device': device}
        levels_path.write_text(json.dumps(levels, indent=1) + '\n')

    def bead_osd_preview(self, kind, channel):
        levels_path = Path(self.args.out).resolve() / 'osd-preview-levels.json'
        levels = json.loads(levels_path.read_text())
        level = levels[channel]
        self.socket_call(self.runtime / 'bead-command.sock', 'bead.osd', 'preview',
                         [kind, level['value'], False, '', level['device']])

    def pointer(self, x, y, mode=None, settle=None):
        """B0067: hover or press at a logical position through spikes/virtual-pointer.c (wlr-virtual-pointer on the private
        nested niri; older compositors fall back to the private parent labwc). `press` holds the button until release_pointer."""
        tool = self.args.pointer_tool
        if not tool or not Path(tool).is_file():
            self.errors.append('no --pointer-tool: pointer states are skipped')
            return
        self.release_pointer()
        mode = mode or 'hover'
        width, height = self.args.size
        scale = self.geometry['scale']
        command = [tool, str(round(x * scale)), str(round(y * scale)), str(width), str(height)] + ([mode] if mode else [])
        if mode in ('press', 'hover'):
            self.held = subprocess.Popen(command, env=self.env, start_new_session=True)
            self.procs.append(self.held)
            time.sleep(settle if settle is not None else 0.04 if mode == 'press' else 0.4)
            if self.held.poll() not in (None, 0):
                self.held = subprocess.Popen(command, env=dict(self.env, WAYLAND_DISPLAY=self.env['BEAD_PARENT']), start_new_session=True)
                self.procs.append(self.held)
                time.sleep(settle if settle is not None else 0.04 if mode == 'press' else 0.4)
        else:
            result = subprocess.run(command, env=self.env, timeout=10, check=False)
            if result.returncode:
                subprocess.run(command, env=dict(self.env, WAYLAND_DISPLAY=self.env['BEAD_PARENT']), timeout=10, check=False)
            time.sleep(0.3)

    def scroll_proof(self):
        def position():
            if self.name == 'qml':
                reply = self.run(['quickshell', 'ipc', '--path', str(self.qml_entry), 'call', 'b0098Probe', 'position'])
                return json.loads(reply.stdout)['position']
            reply = self.socket_call(self.runtime / 'app-settings.sock', 'bead.window', 'state', [])
            return reply['result']['stores']['settings.scroll']
        self.pointer(1100, 600)
        before = position()
        measurements = {'before': before}
        for kind, delta in [('pixel', 10), ('wheel', 1), ('wheel-angle', 1)]:
            self.release_pointer()
            scale = self.geometry['scale']
            command = [self.args.pointer_tool, str(round(1100*scale)), str(round(600*scale)),
                       str(self.args.size[0]), str(self.args.size[1]), kind, str(delta)]
            if kind == 'wheel-angle':
                # A pure discrete notch exercises Qt's angle path without a pixel payload.
                reset = command[:-2] + ['pixel', '-10000']
                result = self.run(reset, env=self.env, check=False)
                if result.returncode:
                    self.run(reset, env=dict(self.env, WAYLAND_DISPLAY=self.env['BEAD_PARENT']))
                time.sleep(.2)
                before = position()
            # Prefer the nested compositor itself; winit may turn parent wheel notches into pixels.
            result = self.run(command, env=self.env, check=False)
            path = 'direct'
            if result.returncode:
                self.run(command, env=dict(self.env, WAYLAND_DISPLAY=self.env['BEAD_PARENT']))
                path = 'parent-pixel-proxy'
            time.sleep(.4)
            after = position()
            measurements[kind] = {'input': delta, 'path': path, 'before': before, 'after': after, 'moved': after-before}
            before = after
        (self.out / 'scroll.json').write_text(json.dumps(measurements, indent=2) + '\n')

    def release_pointer(self):
        held, self.held = getattr(self, 'held', None), None
        if held is not None and held.poll() is None:
            held.terminate()
            try:
                held.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stop(held, 2)

    def notify(self, summary, body):
        self.run(['gdbus', 'call', '--session', '--dest', 'org.freedesktop.Notifications', '--object-path',
                  '/org/freedesktop/Notifications', '--method', 'org.freedesktop.Notifications.Notify', 'parity', '0', '',
                  summary, body, '[]', '{}', '8000'], check=False)

    def steps(self, steps):
        for step in steps:
            kind = step[0]
            if kind == 'ui':
                try:
                    self.dispatch(*step[1:])
                except Exception as error:
                    self.errors.append(f'{" ".join(step[1:])}: {error}')
            elif kind == 'key':
                self.run(['wtype', '-k', step[1]])
                time.sleep(0.3)
            elif kind == 'wallpaper-pref':
                helper = str(Path(self.args.bead_bin).resolve() / 'nacre-helper')
                result = subprocess.run([helper, 'wallpaper-media', 'preferences'], input=json.dumps({'kind': step[1], 'layout': step[2]}),
                                        env=self.env, capture_output=True, text=True, timeout=30)
                result.check_returncode()
                # Prepare identical fixture media before measuring the UI transition.
                catalogue = self.call('wallpaper.catalogue', {}, timeout=30)
                if self.args.wallpapers or self.args.wallpaper_videos:
                    kinds = [row['kind'] for row in catalogue['entries']]
                    if kinds.count('image') != self.args.wallpapers or kinds.count('video') != self.args.wallpaper_videos:
                        raise RuntimeError('wallpaper fixture catalogue does not contain the requested images/videos')
                time.sleep(1)
            elif kind == 'notify':
                self.notify(step[1], step[2])
            elif kind == 'sleep':
                time.sleep(step[1])
            elif kind == 'type':
                self.run(['wtype', '-s', '40', step[1]], check=False)
                time.sleep(0.8)
            elif kind == 'shell':
                try:
                    self.socket_call(self.runtime / 'bead-command.sock', step[1], step[2], list(step[3:]))
                except OSError as error:
                    self.errors.append(f'{" ".join(step[1:])}: {error}')
            elif kind == 'qml-osd-preview':
                try:
                    self.qml_osd_preview(step[1])
                except Exception as error:
                    self.errors.append(f'QML {step[1]} OSD preview: {error}')
            elif kind == 'bead-osd-preview':
                try:
                    self.bead_osd_preview(step[1], step[2])
                except Exception as error:
                    self.errors.append(f'Bead {step[1]} OSD preview: {error}')
            elif kind == 'app_close':
                self.app_close(step[1])
            elif kind == 'bar-window':
                fixture = self.root / 'bar-window.qml'
                fixture.write_text('import QtQuick\nimport Quickshell\nFloatingWindow { visible: true; implicitWidth: 320; implicitHeight: 180; title: "Bar parity fixture"; color: "#203030" }\n')
                self.bar_window = self.spawn(['quickshell', '--path', str(fixture)], 'bar-window')
                time.sleep(1)
                if self.bar_window.poll() is not None:
                    raise RuntimeError('bar workspace fixture window exited')
            elif kind == 'bar-window-close':
                stop(self.bar_window)
            elif kind == 'prompt':
                self.prompt_start(step[1])
            elif kind == 'prompt_end':
                self.prompt_end()
            elif kind == 'scroll-proof':
                self.scroll_proof()
            elif kind == 'pin':
                if self.name == 'qml':
                    self.dispatch('drawers', 'toggle', 'leftPinned')
                else:
                    self.pointer(438, 248, 'click')
            elif kind == 'chat-seed':
                self.call('ai.chat.open', {'channel': ''})
                self.call('ai.chat.send', {'channel': '', 'text': 'Why does the release build fail on CI?', 'attachments': []})
            elif kind == 'click':
                self.pointer(step[1], step[2], 'click')
            elif kind == 'qml_settings':
                self.run(['quickshell', 'ipc', '--path', str(getattr(self, 'qml_entry', Path(self.args.nacre).resolve() / 'nacre/shell/shell.qml')),
                          'call', 'settingsApp', 'open', step[1]], check=False)
            elif kind == 'qml_app':
                self.run(['quickshell', 'ipc', '--path', str(getattr(self, 'qml_entry', Path(self.args.nacre).resolve() / 'nacre/shell/shell.qml')),
                          'call', *step[1:]], check=False)
            elif kind == 'only':
                if step[1] == self.name:
                    self.steps([step[2]])
            elif kind == 'level':
                self.set_level(step[1], step[2])
            elif kind == 'nmcli-probe':
                r = self.run(['nmcli', '--wait', '15', '--colors', 'no', '--fields', 'IN-USE', 'device', 'wifi', 'list', '--rescan', 'yes'], timeout=30, check=False)
                self.errors.append(f'PROBE nmcli rc={r.returncode} {r.stdout[:80]!r} {r.stderr[:200]!r} which={shutil.which("nmcli")}')
            elif kind == 'mute':
                self.set_mute(step[1], step[2])
            elif kind == 'pointer':
                self.pointer(step[1], step[2])
            elif kind == 'pointer-fast':
                self.pointer(step[1], step[2], settle=0.01)
            elif kind == 'press':
                self.pointer(step[1], step[2], 'press')
            elif kind in ('pointer-target', 'press-target'):
                x, y = PTR[step[1]][self.name]
                self.pointer(x, y, 'press' if kind == 'press-target' else None)
            elif kind == 'release':
                self.release_pointer()
            elif kind == 'journal':
                # The controller's pending-change journal, as left by a half-finished change: both shells then show
                # "Keep this display change?". Its monitors are the current ones and nothing is ever applied.
                journal = self.root / 'config' / 'nacre' / 'display-pending-niri.json'
                if step[1] == 'on':
                    journal.parent.mkdir(parents=True, exist_ok=True)
                    journal.write_text(json.dumps({'token': 'parity', 'previous': [], 'monitors': []}))
                else:
                    journal.unlink(missing_ok=True)
            elif kind == 'nm':
                self.run(['gdbus', 'call', '--session', '--dest', 'org.bead.FakeNm', '--object-path', '/ctl',
                          '--method', 'org.bead.FakeNm.Set', step[1], step[2]], check=False)
            elif kind == 'bluez':
                self.run(['gdbus', 'call', '--session', '--dest', 'org.bead.FakeBluez', '--object-path', '/ctl',
                          '--method', 'org.bead.FakeBluez.Set', step[1], step[2]], check=False)

    def app_rss(self):
        if self.name != 'bead':
            return {}
        import socket
        import struct
        result = {}
        for name in ('chat', 'settings'):
            try:
                with socket.socket(socket.AF_UNIX) as client:
                    client.connect(str(self.runtime / ('app-' + name + '.sock')))
                    pid, _, _ = struct.unpack('3i', client.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
                result[name] = self.rss(pid)
            except OSError:
                pass
        return result

    def rss(self, pid=None):
        """VmRSS and RssAnon (KiB) of the shell process: the relative memory gate compares two builds with this, same host and flags."""
        pid = self.shell.pid if pid is None else pid
        try:
            fields = dict(line.split(':', 1) for line in Path(f'/proc/{pid}/status').read_text().splitlines() if ':' in line)
            out = {key: int(fields[key].split()[0]) for key in ('VmRSS', 'RssAnon') if key in fields}
            rollup = Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines()
            for line in rollup:
                if ':' in line:
                    key, value = line.split(':', 1)
                    if key in ('Pss', 'Pss_Anon', 'Pss_File', 'Pss_Shmem', 'SwapPss'):
                        out[key] = int(value.split()[0])
            from measure import drm_memory
            drm = drm_memory(pid)
            out['DRM'] = None if drm is None else round(sum(drm.values()) * 1024, 3)
            return out
        except (OSError, ValueError):
            return {}

    def grab(self, path, scale=None, region=None):
        command = ['grim'] + (['-g', region] if region else []) + (['-s', str(scale)] if scale else []) + [str(path)]
        self.run(command, timeout=30)

    def sequence(self, name, trigger, seconds):
        """Frames of one transition: (milliseconds after the trigger, jpeg). B0067: grim processes are started every
        --seq-interval ms WITHOUT waiting for the previous one (a single grim is ~100 ms), so the frame rate is bounded by
        the interval, not by grim. Stamps record process spawn time relative to the trigger;
        capture scheduling adds uncertainty, so these are image-sequence proxies, not scanout timings."""
        frames_dir = self.root / 'seq'
        shutil.rmtree(frames_dir, ignore_errors=True)
        frames_dir.mkdir()
        region = REGIONS.get(name.rsplit('-', 1)[0])
        interval = self.args.seq_interval / 1000
        jobs = []
        start = time.monotonic()
        # The trigger runs beside the capture: a state opened by several dispatches (toggle, then tab) takes longer than a
        # short transition, which a sequential trigger would hide (B0067: the dashboard looked instant in Bead).
        import threading
        runner = threading.Thread(target=trigger)
        runner.start()
        index = 0
        while time.monotonic() - start < seconds:
            before = time.monotonic()
            command = ['grim'] + (['-g', region] if region else []) + ['-s', str(self.args.seq_scale), str(frames_dir / f'{index:03}.ppm')]
            jobs.append((subprocess.Popen(command, env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL), before))
            index += 1
            time.sleep(max(0.0, before + interval - time.monotonic()))
        runner.join()
        for process, _ in jobs:
            process.wait(timeout=30)
        stamps = [round((before - start) * 1000) for _, before in jobs]
        from PIL import Image
        out = self.out / 'seq' / name
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        for number in range(index):
            with Image.open(frames_dir / f'{number:03}.ppm') as image:
                image.convert('RGB').save(out / f'{number:03}.jpg', quality=82)
        (out / 'stamps.json').write_text(json.dumps(stamps))
        return stamps

    def drive(self, names):
        shots = self.out / 'shots'
        shots.mkdir(exist_ok=True)
        results = {}
        for name in names:
            opens, closes, seconds = STATES[name]
            self.errors = []
            info = {}
            self.dispatch_trace = []
            self.state_start = time.monotonic()
            try:
                self.steps([('ui', 'nacre', 'close')])
                time.sleep(0.8)
                if name.startswith('wallpaper-'):
                    # QML retains this page for 5 s; start both arms after its
                    # idle retirement so query/index state cannot leak between states.
                    time.sleep(5.5)
                self.steps(STATE_PREPARE.get(name, []))
                if name.endswith('-warm'):
                    self.steps(opens)
                    time.sleep(2.5)
                    self.steps(closes)
                    time.sleep(1.5)
                if seconds and opens:
                    info['open_ms'] = self.sequence(name + '-open', lambda: self.steps(opens), seconds)
                else:
                    self.steps(opens)
                time.sleep(STATE_SETTLE.get(name, self.args.settle))
                if name.startswith('launcher'):
                    state = self.dispatch('nacre', 'state')
                    if isinstance(state, str):
                        state = json.loads(state)
                    info['ui_state'] = state
                    if not isinstance(state, dict) or state.get('launcher') is not True:
                        raise RuntimeError('launcher capture is not an open launcher')
                if name.startswith('wallpaper-') and not name.endswith('-settled'):
                    state = self.dispatch('nacre', 'state')
                    if isinstance(state, str):
                        state = json.loads(state)
                    info['ui_state'] = state
                    if not isinstance(state, dict) or state.get('launcher') is not True or state.get('launcherMode') != 'wallpaper':
                        raise RuntimeError('wallpaper capture is not an open wallpaper picker')
                if name in ('wallpaper-activated', 'wallpaper-video-activated'):
                    catalogue = self.call('wallpaper.catalogue', {}, timeout=30)
                    active = [Path(row['id']).name for row in catalogue['entries'] if row['active']]
                    info['active_wallpaper'] = active
                    expected = 'motion-1.mp4' if name == 'wallpaper-video-activated' else 'generated-10.png'
                    if active != [expected]:
                        raise RuntimeError('keyboard selection did not activate ' + expected + ': ' + repr(active))
                # Capture the short dwell cue before /proc sampling can outlast it.
                early_hover = name == 'bar-title-hover'
                if early_hover:
                    self.grab(shots / (name + '.png'))
                if name == 'sidebar-pinned':
                    if self.name == 'qml':
                        state = self.dispatch('nacre', 'state')
                        if isinstance(state, str):
                            state = json.loads(state)
                        info['pin_state'] = state
                        if not state.get('leftPinned') or not state.get('left'):
                            raise RuntimeError('QML pin did not survive outside click')
                    else:
                        state = self.socket_call(self.runtime / 'app-chat.sock', 'bead.panel', 'state', [])['result']
                        if isinstance(state, str):
                            state = json.loads(state)
                        info['pin_state'] = state
                        if not state.get('target_open') or not state.get('stores', {}).get('chat.pinned') or state.get('dismiss_active'):
                            raise RuntimeError('Bead pin did not survive outside click')
                    info['pinned_after_outside_click'] = True
                if name.startswith('welcome-') and self.name == 'bead':
                    info['window_state'] = self.welcome_ready(name)
                if name.startswith('settings') and self.name == 'bead':
                    info['window_state'] = self.socket_call(self.runtime / 'app-settings.sock', 'bead.window', 'state', [])['result']
                info['rss_kib'] = self.rss()
                info['rss_apps_kib'] = self.app_rss()
                if info['rss_kib'].get('SwapPss') != 0:
                    raise RuntimeError('nonzero or unavailable process SwapPss')
                if not early_hover:
                    self.grab(shots / (name + '.png'))
                if name.startswith('bar-title'):
                    state = self.dispatch('nacre', 'state')
                    info['ui_state'] = json.loads(state) if isinstance(state, str) else state
                if seconds and closes:
                    info['close_ms'] = self.sequence(name + '-close', lambda: self.steps(closes), seconds)
                else:
                    self.steps(closes)
                self.steps(STATE_CLEANUP.get(name, []))
                time.sleep(self.args.closed_settle)
                info['rss_closed_kib'] = self.rss()
                info['rss_apps_closed_kib'] = self.app_rss()
                if name == 'sidebar-pinned':
                    if self.name == 'bead' and 'chat' in info['rss_apps_closed_kib']:
                        raise RuntimeError('unpinned sidebar did not close on outside click')
                    if self.name == 'qml':
                        state = self.dispatch('nacre', 'state')
                        if isinstance(state, str): state = json.loads(state)
                        if state.get('left'): raise RuntimeError('unpinned QML sidebar did not close')
                    info['unpin_then_outside_closes'] = True
                info['dispatch_trace'] = self.dispatch_trace
                if info['rss_closed_kib'].get('SwapPss') != 0:
                    raise RuntimeError('nonzero or unavailable closed process SwapPss')
            except Exception as error:
                self.errors.append(str(error)[:300])
                if self.name == 'bead':
                    # A failed sidebar state must not leave an exclusive panel over Settings.
                    try:
                        self.socket_call(self.runtime / 'app-chat.sock', 'bead.panel', 'close', [])
                        time.sleep(self.args.closed_settle)
                    except OSError:
                        pass
            info['errors'] = self.errors
            results[name] = info
            (self.out / 'drive.json').write_text(json.dumps(results, indent=1))
            log(f'{self.name}: {name} {"ERRORS " + "; ".join(self.errors) if self.errors else "ok"}')
            if (name.startswith(('wallpaper-', 'welcome-')) or name in STATE_SETTLE or name in ('control-hover', 'control-pressed', 'lip-hover-right')) and self.errors:
                raise RuntimeError(f'{name} failed: {"; ".join(self.errors)}')
        (self.out / 'drive.json').write_text(json.dumps(results, indent=1))
        try:  # B0095: which methods the private daemon offered (a missing control explains a dimmed button)
            status = self.call('daemon.status', {})
            (self.out / 'daemon-methods.json').write_text(json.dumps(status)[:6000])
        except Exception as error:
            (self.out / 'daemon-methods.json').write_text(json.dumps(str(error)))

    def shutdown(self):
        for process in reversed(self.procs):
            stop(process)
        # Keep the shell log with the captures (frame/motion traces, errors).
        for name in ('shell', 'fake-nm', 'fake-sni', 'tray-snapshot'):
            try:
                shutil.copy(self.logs / f'{name}.log', self.out / f'{name}.log')
            except OSError:
                pass


def run_arm(args):
    root = Path(args.private)
    arm = Arm(args, args.run_arm, root)
    arm.env = dict(os.environ)
    try:
        arm.start_compositor()
        arm.start_daemon()
        arm.start_shell()
        arm.drive(args.states)
        shell_log = (arm.logs / 'shell.log').read_text(errors='replace')
        if args.run_arm == 'bead' and 'UI_ERROR' in shell_log:
            raise RuntimeError('Bead reported UI_ERROR; see shell.log')
        (arm.out / 'shell.log.txt').write_text(shell_log[:6000] + '\n...\n' + shell_log[-14000:])
        (arm.out / 'daemon.log.txt').write_text((arm.logs / 'daemon.log').read_text(errors='replace')[-8000:])
        return 0
    except Exception as error:
        log(f'{args.run_arm}: FAILED {error}')
        for name in ['shell', 'daemon', 'niri', 'labwc']:
            path = arm.logs / (name + '.log')
            if path.is_file():
                (arm.out / (name + '.log.txt')).write_text(path.read_text(errors='replace')[-20000:])
        return 1
    finally:
        arm.shutdown()


# ---------------------------------------------------------------- comparison and gallery (plain PIL)

def metrics(a, b):
    from PIL import Image, ImageChops, ImageStat
    if a.size != b.size:
        b = b.resize(a.size)
    diff = ImageChops.difference(a.convert('RGB'), b.convert('RGB'))
    rms = ImageStat.Stat(diff).rms
    mask = diff.convert('L').point(lambda v: 255 if v > 16 else 0)
    width, height = a.size
    bar = ImageStat.Stat(diff.crop((0, 0, width, round(height * 0.05)))).rms
    return diff, mask, {
        'rmse': round(math.sqrt(sum(v * v for v in rms) / 3) / 255 * 100, 3),
        'changed_pct': round(ImageStat.Stat(mask).mean[0] / 255 * 100, 3),
        'bar_rmse': round(math.sqrt(sum(v * v for v in bar) / 3) / 255 * 100, 3),
        'bbox': mask.getbbox()}


def settle_ms(stamps, directory):
    """First change, and the time after which no frame differs from the final one (the transition end)."""
    from PIL import Image, ImageChops, ImageStat
    files = sorted(directory.glob('*.jpg'))
    if len(files) < 3:
        return None
    images = [Image.open(f).convert('L') for f in files]

    def distance(x, y):
        return ImageStat.Stat(ImageChops.difference(x, y)).mean[0]
    first, last = images[0], images[-1]
    changing = [t for t, image in zip(stamps, images) if distance(image, first) > 0.6]
    unsettled = [t for t, image in zip(stamps, images) if distance(image, last) > 0.6]
    # B0067: bottom and right edge of the pixels that differ from the first frame (px of the sequence image): a growing panel
    # shows its direction and, with the stamps, its speed and easing at the frame rate of --seq-interval.
    edges = []
    for t, image in zip(stamps, images):
        box = ImageChops.difference(first, image).point(lambda v: 255 if v > 24 else 0).getbbox()
        edges.append([t, box[3] if box else 0, box[2] if box else 0])
    return {'start': changing[0] if changing else None, 'end': (unsettled[-1] if unsettled else None),
            'frames': len(files), 'progress': progress_times(stamps, [distance(image, last) for image in images]),
            'edge': edges}


def progress_times(stamps, distances):
    """Easing comparison: progress = 1 - distance(frame, final) / distance(first, final), a monotone proxy for how far the
    transition is. Returns the times (ms, linearly interpolated) at which it passes 10, 50 and 90 %."""
    total = distances[0]
    if total < 0.6:
        return None
    progress = [1 - d / total for d in distances]
    out = {}
    for level in (0.1, 0.5, 0.9):
        for i in range(1, len(progress)):
            if progress[i] >= level > progress[i - 1] or (progress[i] >= level and i == 1):
                span = progress[i] - progress[i - 1]
                out[f't{round(level * 100)}'] = round(stamps[i - 1] + (stamps[i] - stamps[i - 1]) * ((level - progress[i - 1]) / span if span > 0 else 1))
                break
    return out


def motion(directory):
    """Vertical centre of the pixels that differ from the first frame, over time: negative slope = rises/upward."""
    from PIL import Image, ImageChops
    files = sorted(directory.glob('*.jpg'))
    if len(files) < 3:
        return None
    first = Image.open(files[0]).convert('L')
    rows = []
    for f in files[1:]:
        mask = ImageChops.difference(first, Image.open(f).convert('L')).point(lambda v: 255 if v > 24 else 0)
        box = mask.getbbox()
        rows.append(box)
    return rows


def strip(directory, stamps, path, count=12, height=170):
    from PIL import Image, ImageDraw
    files = sorted(directory.glob('*.jpg'))
    if not files:
        return False
    picks = sorted({round(i * (len(files) - 1) / max(count - 1, 1)) for i in range(count)})
    thumbs = []
    for i in picks:
        image = Image.open(files[i]).convert('RGB')
        image = image.resize((round(image.width * height / image.height), height))
        ImageDraw.Draw(image).text((4, 4), f'{stamps[i]} ms', fill=(255, 255, 0))
        thumbs.append(image)
    sheet = Image.new('RGB', (sum(t.width for t in thumbs) + 2 * len(thumbs), height), (20, 20, 20))
    x = 0
    for t in thumbs:
        sheet.paste(t, (x, 0))
        x += t.width + 2
    sheet.save(path, quality=80)
    return True


def build_gallery(args, names):
    from PIL import Image
    out = Path(args.out).resolve()
    (out / 'diff').mkdir(exist_ok=True)
    (out / 'thumb').mkdir(exist_ok=True)
    (out / 'strips').mkdir(exist_ok=True)
    (out / 'full').mkdir(exist_ok=True)
    drive = {arm: json.loads((out / arm / 'drive.json').read_text()) if (out / arm / 'drive.json').is_file() else {}
             for arm in ['qml', 'bead']}
    baseline = json.loads(Path(args.baseline).read_text()) if args.baseline and Path(args.baseline).is_file() else {}
    results = {}
    for name in names:
        row = {'errors': {arm: drive[arm].get(name, {}).get('errors', ['not captured']) for arm in drive}}
        pa, pb = out / 'qml/shots' / (name + '.png'), out / 'bead/shots' / (name + '.png')
        if pa.is_file() and pb.is_file():
            with Image.open(pa) as a, Image.open(pb) as b:
                diff, mask, row['metrics'] = metrics(a, b)
                scale = 960 / a.width
                for arm, image in [('qml', a), ('bead', b)]:  # the full-size capture, light enough to copy around
                    image.convert('RGB').save(out / 'full' / f'{name}-{arm}.jpg', quality=92)
                for arm, image in [('qml', a), ('bead', b)]:
                    image.convert('RGB').resize((960, round(a.height * scale))).save(out / 'thumb' / f'{name}-{arm}.jpg', quality=88)
                # amplified difference, plus the changed mask in red over the QML shot
                diff.point(lambda v: min(255, v * 4)).resize((960, round(a.height * scale))).save(out / 'diff' / (name + '.jpg'), quality=88)
        for phase in ['open', 'close']:
            entry = {}
            for arm in ['qml', 'bead']:
                directory = out / arm / 'seq' / f'{name}-{phase}'
                stamps_path = directory / 'stamps.json'
                if stamps_path.is_file():
                    stamps = json.loads(stamps_path.read_text())
                    entry[arm] = settle_ms(stamps, directory)
                    strip(directory, stamps, out / 'strips' / f'{name}-{phase}-{arm}.jpg')
            if entry:
                row[phase] = entry
        results[name] = row
    (out / 'results.json').write_text(json.dumps(results, indent=1))
    write_html(out, names, results, baseline, args)
    return results


def write_html(out, names, results, baseline, args):
    def cell(name, key, arm):
        return f'<img loading="lazy" src="{key}/{name}{"-" + arm if arm else ""}.jpg">'
    rows, summary = [], []
    for name in names:
        row = results[name]
        m = row.get('metrics')
        base = (baseline.get(name) or {}).get('metrics')
        delta = ''
        if m and base:
            delta = f' <span class="d{"good" if m["rmse"] < base["rmse"] else "bad"}">(before {base["rmse"]})</span>'
        summary.append(f'<tr><td><a href="#{name}">{html.escape(name)}</a></td>'
                       + (f'<td>{m["rmse"]}{delta}</td><td>{m["bar_rmse"]}</td><td>{m["changed_pct"]}</td>' if m else '<td colspan=3>missing</td>')
                       + f'<td>{html.escape("; ".join(sum((e for e in row["errors"].values() if e), [])))[:160]}</td></tr>')
        images = ''
        if m:
            images = (f'<div class="trio"><figure><figcaption>QML (Nacre)</figcaption><a href="full/{name}-qml.jpg">{cell(name, "thumb", "qml")}</a></figure>'
                      f'<figure><figcaption>Bead</figcaption><a href="full/{name}-bead.jpg">{cell(name, "thumb", "bead")}</a></figure>'
                      f'<figure><figcaption>difference x4 (rmse {m["rmse"]}%, changed {m["changed_pct"]}%)</figcaption>{cell(name, "diff", "")}</figure></div>')
        seqs = ''
        for phase in ['open', 'close']:
            if phase in row:
                timing = ' / '.join(f'{arm}: ' + (f'{v["start"]}..{v["end"]} ms, {v["frames"]} frames, progress 10/50/90 % at {"/".join(str((v.get("progress") or {}).get(k, "-")) for k in ("t10", "t50", "t90"))} ms' if v else 'n/a')
                                    for arm, v in row[phase].items())
                seqs += f'<p class="t">{phase} transition, first change..settled: {html.escape(timing)}</p>'
                for arm, v in row[phase].items():
                    if v and v.get('edge'):
                        seqs += f'<p class="t">{arm} bottom edge (ms:px): ' + ' '.join(f'{t}:{b}' for t, b, _ in v['edge']) + '</p>'
                for arm in ['qml', 'bead']:
                    if (out / 'strips' / f'{name}-{phase}-{arm}.jpg').is_file():
                        seqs += f'<div class="strip">{arm}<br><img loading="lazy" src="strips/{name}-{phase}-{arm}.jpg"></div>'
        rows.append(f'<section id="{name}"><h2>{html.escape(name)}</h2>{images}{seqs}</section>')
    (out / 'index.html').write_text(f'''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Bead parity gallery</title>
<style>:root{{color-scheme:dark;--bg:#14161a;--fg:#e4e6ea;--mut:#9aa0aa;--line:#2a2e36}}
body{{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:14px/1.4 system-ui,sans-serif}}
table{{border-collapse:collapse;margin:8px 0 24px}}td,th{{border:1px solid var(--line);padding:3px 8px;text-align:left}}
a{{color:#8ab4f8}}.trio{{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}}figure{{margin:0}}figcaption{{color:var(--mut);font-size:12px}}
img{{max-width:100%}}section{{border-top:1px solid var(--line);padding-top:8px;margin-top:16px}}
.strip{{font-size:12px;color:var(--mut);margin:4px 0;overflow-x:auto}}.t{{margin:6px 0 0;color:var(--mut)}}
.dgood{{color:#7bd88f}}.dbad{{color:#f28b82}}@media(max-width:900px){{.trio{{grid-template-columns:1fr}}}}</style>
<h1>Bead against the QML shell</h1>
<p>{html.escape(args.title)} Output {html.escape(json.dumps(json.loads((out / "qml/geometry.json").read_text()) if (out / "qml/geometry.json").is_file() else {}))}.
rmse and bar rmse are percent of full scale (lower is closer); changed is the share of pixels differing by more than 16/255. Click an image for the full-size capture.</p>
<table><tr><th>state<th>rmse %<th>bar rmse %<th>changed %<th>notes</tr>{"".join(summary)}</table>
{"".join(rows)}''')


# ---------------------------------------------------------------- driver

def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--nacre', required=True, type=Path)
    parser.add_argument('--bead-bin', required=True, type=Path)
    parser.add_argument('--bead-shell', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--states', default='all', help='comma list, or all: ' + ','.join(STATES))
    parser.add_argument('--arms', default='qml,bead')
    parser.add_argument('--size', default='2880x1800')
    parser.add_argument('--scale', type=float, default=1.5)
    parser.add_argument('--wallpapers', type=int, default=0, help='generate N gradient wallpapers into the private HOME (Colors app library)')
    parser.add_argument('--wallpaper-videos', type=int, default=0, help='B0100: generate N short video wallpapers in each private library')
    parser.add_argument('--wallpaper-fixture', action='store_true', help='B0100: derive the matching private presentation from the first generated wallpaper')
    parser.add_argument('--presentation', default='~/.local/state/nacre/presentation.json',
                        help='record copied read-only into both private states (wallpaper + palette); empty for none')
    parser.add_argument('--cpu', action='store_true', help='Bead on the CPU renderer')
    parser.add_argument('--fake-nm', action='store_true', help='B0087: a fake NetworkManager with two VPN profiles on the private bus (both arms)')
    parser.add_argument('--fake-wifi', action='store_true', help='B0095: the fake NetworkManager also has a Wi-Fi device with networks and a scan (implies --fake-nm)')
    parser.add_argument('--shortcut-fixture', action='store_true', help='include three harmless generated bindings in the private niri configuration')
    parser.add_argument('--closed-settle', type=float, default=0.6, help='seconds after close before memory sampling')
    parser.add_argument('--settle', type=float, default=2.0, help='seconds between a state opening and its screenshot')
    parser.add_argument('--boot-settle', type=float, default=4.0)
    parser.add_argument('--seq-scale', type=float, default=0.5)
    parser.add_argument('--parent-output-tool', default=shutil.which('wlr-randr') or '', help='wlr-randr for the private parent output of pointer captures')
    parser.add_argument('--pointer-tool', default='', help='built spikes/virtual-pointer.c (hover and pressed states); empty skips them')
    parser.add_argument('--seq-interval', type=int, default=25, help='ms between overlapped grim captures of a transition (B0067)')
    parser.add_argument('--baseline', help='an earlier results.json: adds a before value to the gallery')
    parser.add_argument('--title', default='')
    parser.add_argument('--gallery-only', action='store_true', help='rebuild index.html/results.json from existing captures')
    parser.add_argument('--capture-guard-child', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--capture-guard-ready-file', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--capture-guard-release-file', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--run-arm', help=argparse.SUPPRESS)
    parser.add_argument('--private', help=argparse.SUPPRESS)
    args = parser.parse_args()
    # Resolve caller-owned read-only inputs before the arm receives its private HOME.
    if args.presentation:
        args.presentation = str(Path(args.presentation).expanduser().resolve())
    args.size = tuple(int(v) for v in args.size.split('x'))
    if args.wallpaper_fixture and args.wallpapers < 1:
        parser.error('--wallpaper-fixture needs at least one generated wallpaper')
    args.fake_nm = args.fake_nm or args.fake_wifi
    args.states = list(STATES) if args.states == 'all' else args.states.split(',')
    unknown = [s for s in args.states if s not in STATES]
    if unknown:
        parser.error('unknown states: ' + ', '.join(unknown))
    if args.run_arm:
        return run_arm(args)
    if not args.gallery_only:
        capture_safety.require_wayland_info()
        if args.capture_guard_child:
            if not args.capture_guard_ready_file or not args.capture_guard_release_file:
                parser.error('capture guard child requires its parent readiness gate')
            capture_safety.wait_for_release(args.capture_guard_ready_file, args.capture_guard_release_file)
        else:
            return capture_safety.run_guarded(
                [sys.executable, __file__, *sys.argv[1:]],
                report_path=args.out / f'capture-safety-{args.arms}.json', env=os.environ.copy())
        if os.getpriority(os.PRIO_PROCESS, 0) < 10:
            os.setpriority(os.PRIO_PROCESS, 0, 10)
        for tool in ['labwc', 'niri', 'grim', 'dbus-run-session', 'gdbus'] + (['quickshell'] if 'qml' in args.arms else []):
            if not shutil.which(tool):
                parser.error('missing tool: ' + tool)
        if 'WAYLAND_DISPLAY' in os.environ:
            log('note: the parent session is never used; a private headless labwc hosts each nested niri')
        args.out.mkdir(parents=True, exist_ok=True)
        meminfo = Path('/proc/meminfo').read_text()
        available = int([line for line in meminfo.splitlines() if line.startswith('MemAvailable')][0].split()[1]) // 1024
        if available < 4096:
            parser.error(f'MemAvailable {available} MiB is below 4 GiB; not starting')
        log('free -h before:\n' + subprocess.run(['free', '-h'], capture_output=True, text=True).stdout)
        arms = args.arms.split(',')
        if {'qml', 'bead'}.issubset(arms) and any(name in args.states for name in ('osd-brightness', 'osd-keyboard')):
            arms = ['qml', 'bead'] + [name for name in arms if name not in ('qml', 'bead')]
        if 'qml' in arms:
            (args.out / 'osd-preview-levels.json').unlink(missing_ok=True)
        for arm in arms:
            meminfo = Path('/proc/meminfo').read_text()
            swap_used = int([line for line in meminfo.splitlines() if line.startswith('SwapTotal:')][0].split()[1]) - int(
                [line for line in meminfo.splitlines() if line.startswith('SwapFree:')][0].split()[1])
            log(f'swap used before {arm}: {swap_used // 1024} MiB')
            if swap_used >= 64 * 1024:
                parser.error(f'swap used {swap_used // 1024} MiB is at least 64 MiB before {arm}; reset between arms and retry')
            swap_in_before = int(dict(line.split() for line in Path('/proc/vmstat').read_text().splitlines())['pswpin'])
            shutil.rmtree(args.out / arm, ignore_errors=True)
            with tempfile.TemporaryDirectory(prefix='bd-par-') as private:
                runtime = Path(private) / 'r'
                runtime.mkdir(mode=0o700)
                env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_DATA_DIRS='/usr/local/share:/usr/share')
                for name in ['home', 'config', 'data', 'state', 'cache']:
                    Path(private, name).mkdir(mode=0o700, exist_ok=True)
                env.update(HOME=str(Path(private, 'home')), XDG_CONFIG_HOME=str(Path(private, 'config')),
                           XDG_DATA_HOME=str(Path(private, 'data')), XDG_STATE_HOME=str(Path(private, 'state')),
                           XDG_CACHE_HOME=str(Path(private, 'cache')))
                for key in ['NIRI_SOCKET', 'HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY', 'NACRE_DAEMON_SOCKET',
                            'NACRE_QS_CONFIG', 'DBUS_SESSION_BUS_ADDRESS']:
                    env.pop(key, None)
                command = ['dbus-run-session', '--', sys.executable, __file__, '--run-arm', arm, '--private', private]
                for flag in ['nacre', 'bead_bin', 'bead_shell', 'out', 'size', 'scale', 'presentation', 'wallpapers', 'wallpaper_videos', 'settle', 'closed_settle', 'boot_settle',
                             'seq_scale', 'seq_interval', 'pointer_tool', 'parent_output_tool']:
                    value = getattr(args, flag)
                    command += ['--' + flag.replace('_', '-'), 'x'.join(map(str, value)) if flag == 'size' else str(value)]
                command += ['--states', ','.join(args.states)] + (['--cpu'] if args.cpu else []) + (['--wallpaper-fixture'] if args.wallpaper_fixture else []) + (['--shortcut-fixture'] if args.shortcut_fixture else []) + (['--fake-nm'] if args.fake_nm else []) + (['--fake-wifi'] if args.fake_wifi else [])
                log(f'arm {arm} starting')
                child = subprocess.Popen(command, env=env, start_new_session=True)
                try:
                    child.wait(timeout=args.arm_timeout if hasattr(args, 'arm_timeout') else 1800)
                except subprocess.TimeoutExpired:
                    stop(child, 20)
                log(f'arm {arm} finished rc={child.returncode}')
                swap_in_after = int(dict(line.split() for line in Path('/proc/vmstat').read_text().splitlines())['pswpin'])
                (args.out / arm).mkdir(parents=True, exist_ok=True)
                (args.out / arm / 'host-swap.json').write_text(json.dumps({
                    'swap_used_before_kib': swap_used, 'pswpin_before': swap_in_before,
                    'pswpin_after': swap_in_after, 'pswpin_delta': swap_in_after - swap_in_before,
                }))
                if swap_in_after != swap_in_before:
                    parser.error(f'host swap-in changed by {swap_in_after - swap_in_before} pages during {arm}; rejecting arm')
                if child.returncode != 0:
                    return child.returncode
        log('free -h after:\n' + subprocess.run(['free', '-h'], capture_output=True, text=True).stdout)
    results = build_gallery(args, args.states)
    worst = sorted(((v['metrics']['rmse'], k) for k, v in results.items() if 'metrics' in v), reverse=True)[:8]
    print('worst states (rmse %):', ', '.join(f'{k} {v}' for v, k in worst))
    print('gallery:', args.out / 'index.html')
    return 0


if __name__ == '__main__':
    sys.exit(main())
