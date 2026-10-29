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
  pactl --format=json list sink-inputs|source-outputs   B0095: the application streams below (the daemon's N0061 producer)
  parec ...                                B0095: the daemon's `audio.peak` capture at the level in $FAKE_AUDIO_DIR/peak (default PEAK)
Anything else exits 0 silently. No sound card or real pactl is touched. Each arm's private PipeWire has the same streams
(`stream_command`), so Quickshell's sound popout lists what the daemon's does."""
import json
import os
import sys

DEFAULT_LEVEL = 0.45
# (pulse name, PipeWire node.name, description)
SINKS = [('fixture-speakers', 'fixture-speakers', 'Fixture speakers'),
         ('fixture-headphones', 'fixture-headphones', 'Fixture headphones')]
SOURCES = [('fixture-microphone', 'fixture-microphone', 'Fixture microphone')]
DEFAULTS = {'sink': SINKS[0][0], 'source': SOURCES[0][0]}

# B0095: application streams, (pulse index, application.name, icon name, level). The level is the pactl scale like DEFAULT_LEVEL;
# the private PipeWire stores its cube.
STREAMS = {'sink-inputs': [(70, 'Firefox', 'firefox', 0.8), (71, 'Spotify', 'spotify', 0.5)],
           'source-outputs': [(80, 'OBS Studio', 'com.obsproject.Studio', 0.6)]}
PEAK = 0.0  # the input meter's default level: the daemon's parec stand-in plays silence unless $FAKE_AUDIO_DIR/peak holds a level
# (the QML meter reads a PipeWire capture stream, which needs a session manager to link; the private PipeWire has none, so it stays at 0)

BATTERY = {'percentage': 63.0, 'state': 2, 'time_to_empty': 2 * 3600 + 10 * 60, 'time_to_full': 0, 'on_battery': True}
PROFILES = {'active': 'balanced', 'profiles': ['power-saver', 'balanced', 'performance']}


def entries(kind, level, muted=False):
    """The pactl JSON list for `kind` (`sink` or `source`); every node carries `level`; the default one `muted`."""
    nodes = SINKS if kind == 'sink' else SOURCES
    channels = ('front-left', 'front-right') if kind == 'sink' else ('mono',)
    volume = {side: {'value': round(level * 65536), 'value_percent': f'{round(level * 100)}%'} for side in channels}
    return [{'index': 50 + index, 'state': 'RUNNING' if name == DEFAULTS[kind] else 'IDLE', 'name': name,
             'description': description, 'mute': muted and name == DEFAULTS[kind], 'volume': volume,
             'properties': {'node.name': node, 'device.class': 'sound'}}
            for index, (name, node, description) in enumerate(nodes)]


def stream_entries(kind):
    """The pactl JSON list for `sink-inputs` or `source-outputs`."""
    out = []
    for index, name, icon, level in STREAMS[kind]:
        value = round(level * 65536)
        out.append({'index': index, 'mute': False, 'volume': {'mono': {'value': value, 'value_percent': f'{round(value / 655.36)}%'}},
                    'properties': {'application.name': name, 'application.icon_name': icon, 'media.name': f'{name} audio'}})
    return out


def stream_commands():
    """(name, argv, PipeWire volume) of one pw-cat per stream for the private PipeWire (no session manager: they stay unlinked
    nodes with an application name and a volume, which is all the sound popout reads). Playback reads /dev/zero."""
    commands = []
    for kind, flag, source in (('sink-inputs', '-p', '< /dev/zero'), ('source-outputs', '-r', '> /dev/null')):
        for index, name, icon, level in STREAMS[kind]:
            props = f'{{ application.name = "{name}" application.icon-name = "{icon}" media.name = "{name} audio" node.name = "fixture-stream-{index}" }}'
            command = f"exec pw-cat {flag} --raw --rate 48000 --channels 2 --format s16 -P '{props}' - {source}"
            commands.append((f'stream-{index}', ['sh', '-c', command], level ** 3))
    return commands


def pipewire_conf():
    """A pipewire.conf.d drop-in with the same nodes as null sinks and a null source, and the default metadata (for Quickshell, which reads PipeWire)."""
    def node(name, description, media_class, position):
        return (f'  {{ factory = adapter\n    args = {{ factory.name = support.null-audio-sink node.name = "{name}" '
                f'node.description = "{description}" media.class = {media_class} object.linger = true audio.position = [ {position} ] }} }}\n')
    nodes = [node(n, d, 'Audio/Sink', 'FL FR') for _, n, d in SINKS] + [node(n, d, 'Audio/Source', 'MONO') for _, n, d in SOURCES]
    # The "default" metadata object (a session manager's job) is where Quickshell reads the default sink and source;
    # the harness fills it with `pw-metadata` (see default_metadata).
    default = '  { factory = metadata\n    args = { metadata.name = default } }\n'
    return 'context.objects = [\n' + ''.join(nodes) + default + ']\n'


def default_metadata():
    """(key, value) pairs for `pw-metadata -n default 0 KEY VALUE Spa:String:JSON`."""
    return [('default.audio.sink', json.dumps({'name': SINKS[0][1]})), ('default.audio.source', json.dumps({'name': SOURCES[0][1]}))]


def level_of(directory, kind):
    try:
        return float(open(os.path.join(directory, kind)).read().strip())
    except (OSError, ValueError):
        return DEFAULT_LEVEL


def pactl(argv, directory, out=sys.stdout):
    """Run one stand-in command and return the exit status (`subscribe` replaces the process instead)."""
    args = [a for a in argv if not a.startswith('--format=')]
    if args[:1] == ['list'] and args[1:2] in (['sink-inputs'], ['source-outputs']):
        json.dump(stream_entries(args[1]), out)
        out.write('\n')
    elif args[:1] == ['list'] and args[1:2] in (['sinks'], ['sources']):
        kind = 'sink' if args[1] == 'sinks' else 'source'
        json.dump(entries(kind, level_of(directory, kind), os.path.exists(os.path.join(directory, kind + '-mute'))), out)
        out.write('\n')
    elif args in (['get-default-sink'], ['get-default-source']):
        out.write(DEFAULTS[args[0].rsplit('-', 1)[1]] + '\n')
    elif args == ['subscribe']:
        os.execvp('tail', ['tail', '-n', '0', '-F', os.path.join(directory, 'events')])
    return 0


def parec(directory):
    """The daemon's peak capture (`parec --raw --format=s16le --rate=8000 --channels=1 --latency-msec=66`): one 66 ms window per
    66 ms of a square wave whose amplitude is read from $FAKE_AUDIO_DIR/peak each time (default PEAK), so the topic holds that level."""
    import time
    while True:
        try:
            level = float(open(os.path.join(directory, 'peak')).read().strip())
        except (OSError, ValueError):
            level = PEAK
        high = int(max(0.0, min(1.0, level)) * 32767)
        sys.stdout.buffer.write(high.to_bytes(2, 'little', signed=True) * 264 + (-high).to_bytes(2, 'little', signed=True) * 264)
        sys.stdout.buffer.flush()
        time.sleep(0.066)


if __name__ == '__main__':
    if sys.argv[1:2] == ['--parec']:
        sys.exit(parec(os.environ.get('FAKE_AUDIO_DIR', '.')))
    sys.exit(pactl(sys.argv[1:], os.environ.get('FAKE_AUDIO_DIR', '.')))
