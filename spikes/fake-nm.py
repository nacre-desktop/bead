#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0087: a fake org.freedesktop.NetworkManager for the private parity sessions: two VPN profiles and a wired one, no devices.
B0095: with `--wifi` it also has one Wi-Fi device (wlan0) connected to "Home Wi-Fi", three networks in range and two more that
appear when a scan finishes (`RequestScan` takes SCAN_SECONDS, then `LastScan` moves), and two saved Wi-Fi profiles.

Run it ONLY on a private D-Bus session (spikes/parity-compare.py --fake-nm does): the Nacre daemon (the Bead arm) reads it as its
"system" bus, and spikes/fake-nmcli.py (the QML arm's `nmcli`) talks to the same object, so both shells see the same profiles and the
same changes. No real VPN, secret or network is involved: activation just walks Activating -> Activated after a moment.
Control (same bus): `org.bead.FakeNm` at `/ctl`, method `Set(key, value)`:
  connect work|home   make that profile Activated at once
  disconnect work|home drop it
  scan now            (--wifi) a scan, as if a client asked: new networks appear after SCAN_SECONDS
  reset               back to the start state (nothing active; with --wifi the scan results too)"""
import sys
import time

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
# B0095 (--wifi): key -> (ssid, bssid, strength, MHz, security); the first three are in range at the start, the rest appear after a scan.
DEVICE = ROOT + '/Devices/1'
POINTS = {
    'home': ('Home Wi-Fi', 'AA:BB:CC:00:00:01', 92, 5180, 'wpa2'),
    'guest': ('Guest', 'AA:BB:CC:00:00:03', 71, 2412, 'open'),
    'workshop': ('Workshop', 'AA:BB:CC:00:00:02', 48, 2437, 'wpa3'),
    'neighbour': ('Neighbour', 'AA:BB:CC:00:00:04', 33, 2462, 'wpa2'),
    'cafe': ('Cafe', 'AA:BB:CC:00:00:05', 21, 2412, 'open'),
}
IN_RANGE, APPEAR = ['home', 'guest', 'workshop'], ['neighbour', 'cafe']
SECURITY = {'open': (0, 0, 0), 'wpa2': (1, 0, 0x188), 'wpa3': (1, 0, 0x400)}  # NM 802.11 Flags, WpaFlags, RsnFlags
WIFI_PROFILES = {  # key -> (path, id, uuid, ssid, key-mgmt)
    'home': (SETTINGS + '/4', 'Home Wi-Fi', 'dddddddd-1111-4222-8333-444444444444', 'Home Wi-Fi', 'wpa-psk'),
    'workshop': (SETTINGS + '/5', 'Workshop', 'eeeeeeee-1111-4222-8333-444444444444', 'Workshop', 'sae'),
}
SCAN_SECONDS = 2.0
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
        self.bus, self.actives, self.serial, self.keep, self.devices = bus, {}, 0, set(), []

    @dbus.service.method('org.freedesktop.DBus.Introspectable', out_signature='s', path_keyword='object_path', connection_keyword='connection')
    def Introspect(self, object_path, connection):
        """The daemon decides which controls to offer from the introspection XML (a writable WirelessEnabled), which dbus-python leaves out."""
        xml = dbus.service.Object.Introspect(self, object_path, connection)
        return xml.replace(f'<interface name="{NM}">', f'<interface name="{NM}">\n    <property name="WirelessEnabled" type="b" access="readwrite"/>', 1)

    def publish(self):
        self.change('ActiveConnections', dbus.Array([dbus.ObjectPath(p) for p in self.actives], signature='o'))

    @dbus.service.method(NM, out_signature='ao')
    def GetDevices(self):
        return dbus.Array([dbus.ObjectPath(p) for p in self.devices], signature='o')

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
        if profile in [p[0] for p in WIFI_PROFILES.values()]:
            node.props['Devices'] = dbus.Array([dbus.ObjectPath(DEVICE)], signature='o')
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
            if path not in self.keep:
                self.drop(path)


class AccessPoint(Props):
    interface = NM + '.AccessPoint'

    def __init__(self, bus, path, key):
        ssid, bssid, strength, mhz, security = POINTS[key]
        flags, wpa, rsn = SECURITY[security]
        super().__init__(bus, path, {'Ssid': dbus.ByteArray(ssid.encode()), 'HwAddress': dbus.String(bssid),
                                     'Strength': dbus.Byte(strength), 'Frequency': dbus.UInt32(mhz), 'Flags': dbus.UInt32(flags),
                                     'WpaFlags': dbus.UInt32(wpa), 'RsnFlags': dbus.UInt32(rsn)})


class Wifi(Props):
    """wlan0: connected to Home Wi-Fi, three networks in range; a scan adds the others after SCAN_SECONDS."""
    interface = NM + '.Device.Wireless'

    def __init__(self, bus):
        super().__init__(bus, DEVICE, {'DeviceType': dbus.UInt32(2), 'Interface': dbus.String('wlan0'), 'State': dbus.UInt32(100),
                                       'ActiveConnection': dbus.ObjectPath('/'), 'ActiveAccessPoint': dbus.ObjectPath('/'),
                                       'StateReason': dbus.Struct((dbus.UInt32(0), dbus.UInt32(0)), signature='uu'),
                                       'AccessPoints': dbus.Array([], signature='o'), 'LastScan': dbus.Int64(1000)})
        self.bus, self.points, self.scanning = bus, {}, False
        self.reset()

    def add(self, key):
        path = f'{ROOT}/AccessPoint/{list(POINTS).index(key) + 1}'
        self.points[key] = AccessPoint(self.bus, path, key)
        return path

    def publish(self):
        self.change('AccessPoints', dbus.Array([dbus.ObjectPath(p.__dbus_object_path__) for p in self.points.values()], signature='o'))

    def reset(self):
        for key in list(self.points):
            self.points.pop(key).remove_from_connection()
        for key in IN_RANGE:
            self.add(key)
        self.props['ActiveAccessPoint'] = dbus.ObjectPath(self.points['home'].__dbus_object_path__)
        self.publish()

    @dbus.service.method(NM + '.Device.Wireless', in_signature='a{sv}')
    def RequestScan(self, options):
        if self.scanning:
            raise dbus.exceptions.DBusException('scan in progress', name=NM + '.Device.NotAllowed')
        self.scanning = True
        GLib.timeout_add(int(SCAN_SECONDS * 1000), self.finish)

    def finish(self):
        for key in APPEAR:
            if key not in self.points:
                self.add(key)
        self.scanning = False
        self.publish()
        self.change('LastScan', dbus.Int64(int(time.monotonic() * 1000)))
        return False


class SettingsRoot(dbus.service.Object):
    def __init__(self, bus, path, wifi):
        super().__init__(bus, path)
        self.wifi = wifi

    @dbus.service.method(NM + '.Settings', out_signature='ao')
    def ListConnections(self):
        paths = [p[0] for p in PROFILES.values()] + ([p[0] for p in WIFI_PROFILES.values()] if self.wifi else [])
        return dbus.Array([dbus.ObjectPath(p) for p in paths], signature='o')


class Profile(dbus.service.Object):
    def __init__(self, bus, path, name, kind, uuid, extra=None):
        super().__init__(bus, path)
        self.name, self.kind, self.uuid, self.extra = name, kind, uuid, extra or {}

    @dbus.service.method(NM + '.Settings.Connection', out_signature='a{sa{sv}}')
    def GetSettings(self):
        return {'connection': {'id': dbus.String(self.name), 'type': dbus.String(self.kind), 'uuid': dbus.String(self.uuid)}, **self.extra}


class Control(dbus.service.Object):
    def __init__(self, bus, manager, wifi=None):
        super().__init__(bus, '/ctl')
        self.manager, self.wifi = manager, wifi

    @dbus.service.method('org.bead.FakeNm', in_signature='ss')
    def Set(self, key, value):
        profile = PROFILES.get(value)
        if key == 'connect' and profile:
            self.manager.start(profile[0], settle=True)
        elif key == 'disconnect' and profile:
            for path, node in list(self.manager.actives.items()):
                if node.props['Connection'] == profile[0]:
                    self.manager.drop(path)
        elif key == 'scan' and self.wifi:
            self.wifi.RequestScan({})
        elif key == 'reset':
            self.manager.reset()
            if self.wifi:
                self.wifi.reset()


def main():
    import os
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    assert address and f'/run/user/{os.getuid()}/bus' not in address, 'a private bus only'
    bus = dbus.SessionBus()
    names = [dbus.service.BusName(NM, bus), dbus.service.BusName('org.bead.FakeNm', bus)]
    manager = Manager(bus)
    wifi = Wifi(bus) if '--wifi' in sys.argv else None
    SettingsRoot(bus, SETTINGS, wifi is not None)
    keep = [Profile(bus, path, name, kind, uuid) for path, name, kind, uuid in PROFILES.values()]
    if wifi:
        for path, name, uuid, ssid, mgmt in WIFI_PROFILES.values():
            keep.append(Profile(bus, path, name, '802-11-wireless', uuid, {
                '802-11-wireless': {'ssid': dbus.ByteArray(ssid.encode())},
                '802-11-wireless-security': {'key-mgmt': dbus.String(mgmt)}}))
        manager.devices.append(DEVICE)
        manager.props['WirelessEnabled'] = dbus.Boolean(True)
        active = manager.start(WIFI_PROFILES['home'][0], settle=True)
        manager.keep.add(active)
        wifi.props['ActiveConnection'] = dbus.ObjectPath(active)
        keep.append(wifi)
    keep.append(Control(bus, manager, wifi))
    print('fake NetworkManager ready', len(names), len(keep), flush=True)
    GLib.MainLoop().run()


if __name__ == '__main__':
    sys.exit(main())
