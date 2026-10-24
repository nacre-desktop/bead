#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0087: the QML arm's `nmcli` in the private parity sessions, answering the two calls the QML VPN page makes from the
fake NetworkManager of spikes/fake-nm.py (the real nmcli needs a full libnm object model; the QML itself is unmodified):

  nmcli --colors no --terse --escape yes --fields UUID,TYPE,NAME,DEVICE connection show
  nmcli --wait 30 connection up|down uuid UUID

Reads DBUS_SYSTEM_BUS_ADDRESS (the private bus). Exit 0 when the change settled, 1 otherwise, like `nmcli --wait`."""
import os
import sys
import time

import dbus

NM, ROOT = 'org.freedesktop.NetworkManager', '/org/freedesktop/NetworkManager'


def escape(text):
    return text.replace('\\', '\\\\').replace(':', '\\:')


def main(argv):
    words = [word for word in argv if not word.startswith('--')]
    # Values of the flags (`--wait 30`, `--colors no`, `--fields F`, `--escape yes`) are not commands.
    values = {argv[i + 1] for i, word in enumerate(argv[:-1]) if word.startswith('--') and word != '--terse'}
    words = [word for word in words if word not in values]
    if words[:1] != ['connection']:
        return 2
    bus = dbus.bus.BusConnection(os.environ['DBUS_SYSTEM_BUS_ADDRESS'])
    props = lambda path, interface, name: bus.get_object(NM, path).Get(interface, name, dbus_interface='org.freedesktop.DBus.Properties')
    profiles = {}
    for path in bus.get_object(NM, ROOT + '/Settings').ListConnections(dbus_interface=NM + '.Settings'):
        section = bus.get_object(NM, path).GetSettings(dbus_interface=NM + '.Settings.Connection')['connection']
        profiles[str(path)] = (str(section['uuid']), str(section['type']), str(section['id']))
    actives = {}
    for path in props(ROOT, NM, 'ActiveConnections'):
        actives[str(path)] = str(props(path, NM + '.Connection.Active', 'Connection'))
    if words[1:2] == ['show']:
        active_profiles = set(actives.values())
        for path, (uuid, kind, name) in profiles.items():
            print(':'.join([uuid, kind, escape(name), 'vpn-link' if path in active_profiles else '--']))
        return 0
    if words[1:2] in (['up'], ['down']) and words[2:3] == ['uuid'] and len(words) == 4:
        path = next((p for p, (uuid, _, _) in profiles.items() if uuid == words[3]), None)
        if path is None:
            return 10
        manager = bus.get_object(NM, ROOT)
        if words[1] == 'up':
            active = manager.ActivateConnection(path, '/', '/', dbus_interface=NM)
            deadline = time.time() + 30
            while time.time() < deadline:
                try:
                    if int(props(active, NM + '.Connection.Active', 'State')) == 2:
                        return 0
                except dbus.DBusException:
                    return 1
                time.sleep(0.1)
            return 1
        active = next((a for a, p in actives.items() if p == path), None)
        if active is None:
            return 10
        manager.DeactivateConnection(active, dbus_interface=NM)
        deadline = time.time() + 30
        while time.time() < deadline:
            if active not in [str(a) for a in props(ROOT, NM, 'ActiveConnections')]:
                return 0
            time.sleep(0.1)
        return 1
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
