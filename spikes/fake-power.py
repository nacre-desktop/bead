#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0083: a fake org.freedesktop.UPower and power-profiles-daemon for the private parity sessions.

Run it ONLY on a private D-Bus session (spikes/parity-compare.py does): both shells read that bus as their "system" bus
(like spikes/fake-bluez.py), so the Nacre daemon and Quickshell's UPower / PowerProfiles modules see one fixed machine,
with the values in spikes/fake_system.py: a battery at 63 % discharging with 2 h 10 min left, and the profiles
power-saver, balanced and performance (balanced active). Setting `ActiveProfile` works and signals, like the real daemon;
nothing is ever written outside this process. The profile service answers on both names power-profiles-daemon uses
(`org.freedesktop.UPower.PowerProfiles` and the legacy `net.hadess.PowerProfiles`)."""
import os
from pathlib import Path
import sys

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fake_system  # noqa: E402

PROPS = 'org.freedesktop.DBus.Properties'
UPOWER, DEVICE = 'org.freedesktop.UPower', 'org.freedesktop.UPower.Device'
DISPLAY = '/org/freedesktop/UPower/devices/DisplayDevice'
BATTERY = '/org/freedesktop/UPower/devices/battery_BAT0'
SERVICES = [('org.freedesktop.UPower.PowerProfiles', '/org/freedesktop/UPower/PowerProfiles'),
            ('net.hadess.PowerProfiles', '/net/hadess/PowerProfiles')]


def device_props():
    b = fake_system.BATTERY
    return {
        'NativePath': dbus.String('BAT0'), 'Vendor': dbus.String('Fixture'), 'Model': dbus.String('Fixture battery'),
        'Serial': dbus.String('0001'), 'UpdateTime': dbus.UInt64(1), 'Type': dbus.UInt32(2), 'PowerSupply': dbus.Boolean(True),
        'HasHistory': dbus.Boolean(False), 'HasStatistics': dbus.Boolean(False), 'Online': dbus.Boolean(False),
        'Energy': dbus.Double(31.5), 'EnergyEmpty': dbus.Double(0.0), 'EnergyFull': dbus.Double(50.0),
        'EnergyFullDesign': dbus.Double(52.0), 'EnergyRate': dbus.Double(14.5), 'Voltage': dbus.Double(11.8),
        'ChargeCycles': dbus.Int32(-1), 'Luminosity': dbus.Double(0.0),
        'TimeToEmpty': dbus.Int64(b['time_to_empty']), 'TimeToFull': dbus.Int64(b['time_to_full']),
        'Percentage': dbus.Double(b['percentage']), 'Temperature': dbus.Double(0.0), 'IsPresent': dbus.Boolean(True),
        'State': dbus.UInt32(b['state']), 'IsRechargeable': dbus.Boolean(True), 'Capacity': dbus.Double(96.0),
        'Technology': dbus.UInt32(1), 'WarningLevel': dbus.UInt32(1), 'BatteryLevel': dbus.UInt32(1),
        'IconName': dbus.String('battery-good-symbolic'),
    }


class Node(dbus.service.Object):
    """One object path with the properties of its interfaces."""

    def __init__(self, bus, path, ifaces, writable=()):
        super().__init__(bus, path)
        self.ifaces, self.writable = ifaces, set(writable)

    @dbus.service.method(PROPS, in_signature='s', out_signature='a{sv}')
    def GetAll(self, iface):
        return dbus.Dictionary(self.ifaces.get(iface, {}), signature='sv')

    @dbus.service.method(PROPS, in_signature='ss', out_signature='v')
    def Get(self, iface, name):
        try:
            return self.ifaces[iface][name]
        except KeyError:
            raise dbus.exceptions.DBusException('No such property ' + name, name='org.freedesktop.DBus.Error.UnknownProperty')

    @dbus.service.method(PROPS, in_signature='ssv')
    def Set(self, iface, name, value):
        if (iface, name) not in self.writable:
            raise dbus.exceptions.DBusException('Read-only property ' + name, name='org.freedesktop.DBus.Error.PropertyReadOnly')
        self.change(iface, name, type(self.ifaces[iface][name])(value))

    @dbus.service.signal(PROPS, signature='sa{sv}as')
    def PropertiesChanged(self, iface, changed, invalidated):
        pass

    def change(self, iface, name, value):
        self.ifaces[iface][name] = value
        self.PropertiesChanged(iface, {name: value}, [])


class Root(Node):
    """/org/freedesktop/UPower: the device list and OnBattery."""

    @dbus.service.method(UPOWER, out_signature='ao')
    def EnumerateDevices(self):
        return [dbus.ObjectPath(BATTERY)]

    @dbus.service.method(UPOWER, out_signature='o')
    def GetDisplayDevice(self):
        return dbus.ObjectPath(DISPLAY)

    @dbus.service.method(UPOWER, in_signature='s', out_signature='s')
    def GetCriticalAction(self):
        return 'HybridSleep'

    @dbus.service.signal(UPOWER, signature='o')
    def DeviceAdded(self, path):
        pass


def profile_entries():
    return dbus.Array([dbus.Dictionary({'Profile': dbus.String(name), 'Driver': dbus.String('fixture')}, signature='sv')
                       for name in fake_system.PROFILES['profiles']], signature='a{sv}')


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    assert address and f'/run/user/{os.getuid()}/bus' not in address, 'a private bus only'
    bus = dbus.SessionBus()
    keep = [dbus.service.BusName(UPOWER, bus)]
    root = {UPOWER: {'DaemonVersion': dbus.String('1.90.0'), 'OnBattery': dbus.Boolean(fake_system.BATTERY['on_battery']),
                     'OnLowBattery': dbus.Boolean(False), 'LidIsClosed': dbus.Boolean(False), 'LidIsPresent': dbus.Boolean(True)}}
    keep.append(Root(bus, '/org/freedesktop/UPower', root))
    keep.append(Node(bus, DISPLAY, {DEVICE: device_props()}))
    keep.append(Node(bus, BATTERY, {DEVICE: device_props()}))
    for name, path in SERVICES:
        keep.append(dbus.service.BusName(name, bus))
        props = {'ActiveProfile': dbus.String(fake_system.PROFILES['active']), 'Profiles': profile_entries(),
                 'PerformanceDegraded': dbus.String(''), 'PerformanceInhibited': dbus.String(''),
                 'Actions': dbus.Array([], signature='s'), 'ActionsInfo': dbus.Array([], signature='a{sv}'),
                 'ActiveProfileHolds': dbus.Array([], signature='a{sv}'), 'Version': dbus.String('0.30')}
        keep.append(Node(bus, path, {name: props}, writable=[(name, 'ActiveProfile')]))
    print('fake UPower and power-profiles-daemon ready', len(keep), flush=True)
    GLib.MainLoop().run()


if __name__ == '__main__':
    sys.exit(main())
