#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0083: fixed audio, battery and power-profile values for the private parity sessions, and a stand-in `pactl`.

Harness-only. `spikes/parity-compare.py` puts a wrapper for this file first on PATH (as `pactl`) and
`spikes/fake-power.py` serves the BATTERY / PROFILES values on the private bus, so the Nacre daemon (Bead arm) and
Quickshell (QML arm, which reads UPower and power-profiles-daemon itself) see one machine: 2 sinks and 1 source at
45 %, a battery at 63 % discharging with 2 h 10 min left, and the profiles power-saver, balanced and performance.

The stand-in answers what the daemon's N0059/N0043 producers run, from files in $FAKE_AUDIO_DIR (`sink` and `source`
hold the level of the default node, `events` is what `subscribe` follows):
  pactl --format=json list sinks|sources   a Pulse JSON array (65536 = 100 %, perceptual like pactl)
  pactl get-default-sink|get-default-source
  pactl subscribe                          follows $FAKE_AUDIO_DIR/events
Anything else exits 0 silently. No PipeWire, sound card or real pactl is touched."""
import json
import os
import sys

DEFAULT_LEVEL = 0.45
# (pulse name, PipeWire node.name, description)
SINKS = [('fixture-speakers', 'fixture-speakers', 'Fixture speakers'),
         ('fixture-headphones', 'fixture-headphones', 'Fixture headphones')]
SOURCES = [('fixture-microphone', 'fixture-microphone', 'Fixture microphone')]
DEFAULTS = {'sink': SINKS[0][0], 'source': SOURCES[0][0]}

BATTERY = {'percentage': 63.0, 'state': 2, 'time_to_empty': 2 * 3600 + 10 * 60, 'time_to_full': 0, 'on_battery': True}
PROFILES = {'active': 'balanced', 'profiles': ['power-saver', 'balanced', 'performance']}


def entries(kind, level):
    """The pactl JSON list for `kind` (`sink` or `source`); every node carries `level`."""
    nodes = SINKS if kind == 'sink' else SOURCES
    channels = ('front-left', 'front-right') if kind == 'sink' else ('mono',)
    volume = {side: {'value': round(level * 65536), 'value_percent': f'{round(level * 100)}%'} for side in channels}
    return [{'index': 50 + index, 'state': 'RUNNING' if name == DEFAULTS[kind] else 'IDLE', 'name': name,
             'description': description, 'mute': False, 'volume': volume,
             'properties': {'node.name': node, 'device.class': 'sound'}}
            for index, (name, node, description) in enumerate(nodes)]


def level_of(directory, kind):
    try:
        return float(open(os.path.join(directory, kind)).read().strip())
    except (OSError, ValueError):
        return DEFAULT_LEVEL


def pactl(argv, directory, out=sys.stdout):
    """Run one stand-in command and return the exit status (`subscribe` replaces the process instead)."""
    args = [a for a in argv if not a.startswith('--format=')]
    if args[:1] == ['list'] and args[1:2] in (['sinks'], ['sources']):
        kind = 'sink' if args[1] == 'sinks' else 'source'
        json.dump(entries(kind, level_of(directory, kind)), out)
        out.write('\n')
    elif args in (['get-default-sink'], ['get-default-source']):
        out.write(DEFAULTS[args[0].rsplit('-', 1)[1]] + '\n')
    elif args == ['subscribe']:
        os.execvp('tail', ['tail', '-n', '0', '-F', os.path.join(directory, 'events')])
    return 0


if __name__ == '__main__':
    sys.exit(pactl(sys.argv[1:], os.environ.get('FAKE_AUDIO_DIR', '.')))
