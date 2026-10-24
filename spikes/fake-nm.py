#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0087: a fake org.freedesktop.NetworkManager for the private parity sessions: two VPN profiles and a wired one, no devices.

Run it ONLY on a private D-Bus session (spikes/parity-compare.py --fake-nm does): the Nacre daemon (the Bead arm) reads it as its
"system" bus, and spikes/fake-nmcli.py (the QML arm's `nmcli`) talks to the same object, so both shells see the same profiles and the
same changes. No real VPN, secret or network is involved: activation just walks Activating -> Activated after a moment.
Control (same bus): `org.bead.FakeNm` at `/ctl`, method `Set(key, value)`:
  connect work|home   make that profile Activated at once
  disconnect work|home drop it
  reset               back to the start state (nothing active)"""
import sys

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

NM = 'org.freedesktop.NetworkManager'
ROOT = '/org/freedesktop/NetworkManager'
PROPS = 'org.freedesktop.DBus.Properties'
SETTINGS = ROOT + '/Settings'
PROFILES = {  # key -> (path, id, type, uuid)
    'work': (SETTINGS + '/1', 'Work VPN', 'vpn', 'aaaaaaaa-1111-4222-8333-444444444444'),
    'home': (SETTINGS + '/2', 'Home WG', 'wireguard', 'bbbbbbbb-1111-4222-8333-444444444444'),
    'wired': (SETTINGS + '/3', 'Wired', '802-3-ethernet', 'cccccccc-1111-4222-8333-444444444444'),
}
ACTIVATION_SECONDS = 0.9
DEACTIVATION_SECONDS = 0.5


class Props(dbus.service.Object):
    """Properties of one interface kept in `self.props`; PropertiesChanged on change."""
    interface = ''

    def __init__(self, bus, path, props):
        super().__init__(bus, path)
        self.props = props

    @dbus.service.method(PROPS, in_signature='ss', out_signature='v')
    def Get(self, interface, name):
        return self.props[name]

    @dbus.service.method(PROPS, in_signature='s', out_signature='a{sv}')
    def GetAll(self, interface):
        return dbus.Dictionary(self.props, signature='sv')

    @dbus.service.method(PROPS, in_signature='ssv')
    def Set(self, interface, name, value):
        self.change(name, value)

    @dbus.service.signal(PROPS, signature='sa{sv}as')
    def PropertiesChanged(self, interface, changed, invalidated):
        pass

    def change(self, name, value):
        self.props[name] = value
        self.PropertiesChanged(self.interface, {name: value}, [])


class Active(Props):
    interface = NM + '.Connection.Active'

    def __init__(self, bus, path, profile_path):
        super().__init__(bus, path, {'Connection': dbus.ObjectPath(profile_path), 'State': dbus.UInt32(1),
                                     'Devices': dbus.Array([], signature='o')})

    @dbus.service.signal(NM + '.Connection.Active', signature='uu')
    def StateChanged(self, state, reason):
        pass

    def move(self, state, reason):
        self.change('State', dbus.UInt32(state))
        self.StateChanged(state, reason)


class Manager(Props):
    interface = NM

    def __init__(self, bus):
        super().__init__(bus, ROOT, {'WirelessEnabled': dbus.Boolean(False),
                                     'ActiveConnections': dbus.Array([], signature='o'),
                                     'PrimaryConnection': dbus.ObjectPath('/')})
        self.bus, self.actives, self.serial = bus, {}, 0

    def publish(self):
        self.change('ActiveConnections', dbus.Array([dbus.ObjectPath(p) for p in self.actives], signature='o'))

    @dbus.service.method(NM, out_signature='ao')
    def GetDevices(self):
        return dbus.Array([], signature='o')

    @dbus.service.method(NM, in_signature='ooo', out_signature='o')
    def ActivateConnection(self, profile, device, specific):
        if any(p.props['Connection'] == profile for p in self.actives.values()):
            return next(path for path, p in self.actives.items() if p.props['Connection'] == profile)
        return dbus.ObjectPath(self.start(str(profile), settle=False))

    @dbus.service.method(NM, in_signature='a{sa{sv}}ooa{sv}', out_signature='ooa{sv}')
    def AddAndActivateConnection2(self, settings, device, specific, options):
        raise dbus.exceptions.DBusException('not supported by the fixture', name=NM + '.NotSupported')

    @dbus.service.method(NM, in_signature='o')
    def DeactivateConnection(self, active):
        node = self.actives.get(str(active))
        if node is None:
            raise dbus.exceptions.DBusException('not active', name=NM + '.ConnectionNotActive')
        node.move(3, 2)
        GLib.timeout_add(int(DEACTIVATION_SECONDS * 1000), self.drop, str(active))

    def start(self, profile, settle):
        self.serial += 1
        path = f'{ROOT}/ActiveConnection/{self.serial}'
        node = Active(self.bus, path, profile)
        self.actives[path] = node
        self.publish()
        if settle:
            node.move(2, 0)
        else:
            GLib.timeout_add(int(ACTIVATION_SECONDS * 1000), lambda: node.move(2, 0) or False)
        return path

    def drop(self, path):
        node = self.actives.pop(path, None)
        if node:
            node.move(4, 2)
            self.publish()
            node.remove_from_connection()
        return False

    def reset(self):
        for path in list(self.actives):
            self.drop(path)


class SettingsRoot(dbus.service.Object):
    @dbus.service.method(NM + '.Settings', out_signature='ao')
    def ListConnections(self):
        return dbus.Array([dbus.ObjectPath(p[0]) for p in PROFILES.values()], signature='o')


class Profile(dbus.service.Object):
    def __init__(self, bus, path, name, kind, uuid):
        super().__init__(bus, path)
        self.name, self.kind, self.uuid = name, kind, uuid

    @dbus.service.method(NM + '.Settings.Connection', out_signature='a{sa{sv}}')
    def GetSettings(self):
        return {'connection': {'id': dbus.String(self.name), 'type': dbus.String(self.kind), 'uuid': dbus.String(self.uuid)}}


class Control(dbus.service.Object):
    def __init__(self, bus, manager):
        super().__init__(bus, '/ctl')
        self.manager = manager

    @dbus.service.method('org.bead.FakeNm', in_signature='ss')
    def Set(self, key, value):
        profile = PROFILES.get(value)
        if key == 'connect' and profile:
            self.manager.start(profile[0], settle=True)
        elif key == 'disconnect' and profile:
            for path, node in list(self.manager.actives.items()):
                if node.props['Connection'] == profile[0]:
                    self.manager.drop(path)
        elif key == 'reset':
            self.manager.reset()


def main():
    import os
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    assert address and f'/run/user/{os.getuid()}/bus' not in address, 'a private bus only'
    bus = dbus.SessionBus()
    names = [dbus.service.BusName(NM, bus), dbus.service.BusName('org.bead.FakeNm', bus)]
    manager = Manager(bus)
    SettingsRoot(bus, SETTINGS)
    keep = [Profile(bus, path, name, kind, uuid) for path, name, kind, uuid in PROFILES.values()]
    keep.append(Control(bus, manager))
    print('fake NetworkManager ready', len(names), len(keep), flush=True)
    GLib.MainLoop().run()


if __name__ == '__main__':
    sys.exit(main())
