#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0087: the QML arm's `nmcli` in the private parity sessions, answering the calls the QML makes from the
fake NetworkManager of spikes/fake-nm.py (the real nmcli needs a full libnm object model; the QML itself is unmodified):

  nmcli --colors no --terse --escape yes --fields UUID,TYPE,NAME,DEVICE connection show    (any of those four fields)
  nmcli --wait 30 connection up|down uuid UUID
  B0095 (needs fake-nm.py --wifi):
  nmcli --colors no --get-values 802-11-wireless.ssid connection show uuid UUID           (saved Wi-Fi names)
  nmcli --wait 15 --fields IN-USE device wifi list --rescan yes                           (RequestScan, then wait for LastScan)
  nmcli --terse --fields DEVICE,TYPE,STATE device status

Reads DBUS_SYSTEM_BUS_ADDRESS (the private bus). Exit 0 when the change settled, 1 otherwise, like `nmcli --wait`."""
import os
import sys
import time

import dbus

NM, ROOT = 'org.freedesktop.NetworkManager', '/org/freedesktop/NetworkManager'


def escape(text):
    return text.replace('\\', '\\\\').replace(':', '\\:')


def scan(bus, props):
    """`device wifi list --rescan yes`: ask the Wi-Fi device for a scan and wait (like the daemon) until `LastScan` moves."""
    wireless = NM + '.Device.Wireless'
    for device in bus.get_object(NM, ROOT).GetDevices(dbus_interface=NM):
        before = props(device, wireless, 'LastScan')
        try:
            bus.get_object(NM, device).RequestScan({}, dbus_interface=wireless)
        except dbus.DBusException:
            return 1
        deadline = time.time() + 15
        while time.time() < deadline:
            if props(device, wireless, 'LastScan') != before:
                return 0
            time.sleep(0.25)
        return 1
    return 1


def main(argv):
    words = [word for word in argv if not word.startswith('--')]
    # Values of the flags (`--wait 30`, `--colors no`, `--fields F`, `--escape yes`) are not commands.
    values = {argv[i + 1] for i, word in enumerate(argv[:-1]) if word.startswith('--') and word != '--terse'}
    words = [word for word in words if word not in values]
    bus = dbus.bus.BusConnection(os.environ['DBUS_SYSTEM_BUS_ADDRESS'])
    props = lambda path, interface, name: bus.get_object(NM, path).Get(interface, name, dbus_interface='org.freedesktop.DBus.Properties')
    if words[:2] == ['device', 'status']:
        for device in bus.get_object(NM, ROOT).GetDevices(dbus_interface=NM):
            print(f"{props(device, NM + '.Device', 'Interface')}:wifi:connected")
        return 0
    if words[:3] == ['device', 'wifi', 'list']:
        return scan(bus, props)
    if words[:1] != ['connection']:
        return 2
    profiles = {}
    for path in bus.get_object(NM, ROOT + '/Settings').ListConnections(dbus_interface=NM + '.Settings'):
        section = bus.get_object(NM, path).GetSettings(dbus_interface=NM + '.Settings.Connection')['connection']
        profiles[str(path)] = (str(section['uuid']), str(section['type']), str(section['id']))
    actives = {}
    for path in props(ROOT, NM, 'ActiveConnections'):
        actives[str(path)] = str(props(path, NM + '.Connection.Active', 'Connection'))
    if words[1:2] == ['show'] and words[2:3] == ['uuid'] and len(words) == 4 and '802-11-wireless.ssid' in argv:
        path = next((p for p, (uuid, _, _) in profiles.items() if uuid == words[3]), None)
        if path is None:
            return 10
        settings = bus.get_object(NM, path).GetSettings(dbus_interface=NM + '.Settings.Connection')
        print(bytes(settings['802-11-wireless']['ssid']).decode())
        return 0
    if words[1:2] == ['show']:
        active_profiles = set(actives.values())
        fields = (argv[argv.index('--fields') + 1] if '--fields' in argv else 'UUID,TYPE,NAME,DEVICE').split(',')
        for path, (uuid, kind, name) in profiles.items():
            columns = {'UUID': uuid, 'TYPE': kind, 'NAME': escape(name), 'DEVICE': 'vpn-link' if path in active_profiles else '--'}
            print(':'.join(columns[field] for field in fields))
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
