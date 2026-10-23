#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""B0062: a fake org.bluez for the private parity sessions: one adapter and a few devices on the bus it is run on.

Run it ONLY on a private D-Bus session (spikes/parity-compare.py does): both shells read it as their "system" bus, so
the Nacre daemon (the Bead arm) and Quickshell's Bluetooth module (the QML arm) see the same fixtures, with no real
Bluetooth and no system bus involved. Control (also on that bus): `org.bead.FakeBluez` at `/ctl`, method `Set(key, value)`:
  power on|off       Adapter1.Powered
  discovering on|off Adapter1.Discovering (on also makes a nearby speaker appear after a moment)
  reset              back to the start state
Connect/Disconnect/Pair succeed after a short delay, so a busy row is visible."""
import sys

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

ADAPTER = '/org/bluez/hci0'
OM, PROPS = 'org.freedesktop.DBus.ObjectManager', 'org.freedesktop.DBus.Properties'


def device(address, name, icon, paired, connected, battery=None, rssi=None):
    props = {
        'Address': dbus.String(address), 'Alias': dbus.String(name), 'Name': dbus.String(name), 'Icon': dbus.String(icon),
        'Paired': dbus.Boolean(paired), 'Trusted': dbus.Boolean(paired), 'Connected': dbus.Boolean(connected),
        'Adapter': dbus.ObjectPath(ADAPTER),
    }
    if rssi is not None:
        props['RSSI'] = dbus.Int16(rssi)
    ifaces = {'org.bluez.Device1': props}
    if battery is not None:
        ifaces['org.bluez.Battery1'] = {'Percentage': dbus.Byte(battery)}
    return ifaces


def start_state():
    return {
        ADAPTER: {'org.bluez.Adapter1': {'Address': dbus.String('00:1A:7D:DA:71:13'), 'Alias': dbus.String('Fixture adapter'),
                                         'Name': dbus.String('Fixture adapter'), 'Powered': dbus.Boolean(True),
                                         'Discovering': dbus.Boolean(False)}},
        ADAPTER + '/dev_AA_00_00_00_00_01': device('AA:00:00:00:00:01', 'Fixture headphones', 'audio-headset', True, True, 80),
        ADAPTER + '/dev_AA_00_00_00_00_02': device('AA:00:00:00:00:02', 'Fixture keyboard', 'input-keyboard', True, False),
        ADAPTER + '/dev_AA_00_00_00_00_03': device('AA:00:00:00:00:03', 'Fixture phone', 'phone', True, False, 55),
        ADAPTER + '/dev_AA_00_00_00_00_04': device('AA:00:00:00:00:04', 'Fixture mouse', 'input-mouse', False, False, None, -58),
    }


class Node(dbus.service.Object):
    """One object path with its interfaces' properties; the same class serves adapter, devices and the manager."""

    def __init__(self, bus, path, ifaces, world):
        super().__init__(bus, path)
        self.ifaces, self.world = ifaces, world

    @dbus.service.method(PROPS, in_signature='s', out_signature='a{sv}')
    def GetAll(self, iface):
        return dbus.Dictionary(self.ifaces.get(iface, {}), signature='sv')

    @dbus.service.method(PROPS, in_signature='ss', out_signature='v')
    def Get(self, iface, name):
        return self.ifaces[iface][name]

    @dbus.service.method(PROPS, in_signature='ssv')
    def Set(self, iface, name, value):
        self.change(iface, name, type(self.ifaces[iface][name])(value))

    @dbus.service.signal(PROPS, signature='sa{sv}as')
    def PropertiesChanged(self, iface, changed, invalidated):
        pass

    def change(self, iface, name, value):
        self.ifaces[iface][name] = value
        self.PropertiesChanged(iface, {name: value}, [])

    # org.bluez.Adapter1
    @dbus.service.method('org.bluez.Adapter1')
    def StartDiscovery(self):
        if self.ifaces['org.bluez.Adapter1']['Discovering']:
            raise dbus.exceptions.DBusException('In Progress', name='org.bluez.Error.InProgress')
        self.change('org.bluez.Adapter1', 'Discovering', dbus.Boolean(True))
        GLib.timeout_add(1200, self.world.nearby)

    @dbus.service.method('org.bluez.Adapter1')
    def StopDiscovery(self):
        self.change('org.bluez.Adapter1', 'Discovering', dbus.Boolean(False))

    # org.bluez.Device1
    def later(self, name, value, ok, err):
        def done():
            self.change('org.bluez.Device1', name, dbus.Boolean(value))
            ok()
            return False
        GLib.timeout_add(900, done)

    @dbus.service.method('org.bluez.Device1', async_callbacks=('ok', 'err'))
    def Connect(self, ok, err):
        self.later('Connected', True, ok, err)

    @dbus.service.method('org.bluez.Device1', async_callbacks=('ok', 'err'))
    def Disconnect(self, ok, err):
        self.later('Connected', False, ok, err)

    @dbus.service.method('org.bluez.Device1', async_callbacks=('ok', 'err'))
    def Pair(self, ok, err):
        self.later('Paired', True, ok, err)


class Agents(dbus.service.Object):
    """The pairing agent registration the Nacre daemon makes; nothing is asked of the user here."""

    @dbus.service.method('org.bluez.AgentManager1', in_signature='os')
    def RegisterAgent(self, agent, capability):
        pass

    @dbus.service.method('org.bluez.AgentManager1', in_signature='o')
    def UnregisterAgent(self, agent):
        pass

    @dbus.service.method('org.bluez.AgentManager1', in_signature='o')
    def RequestDefaultAgent(self, agent):
        pass


class Control(dbus.service.Object):
    def __init__(self, bus, manager):
        super().__init__(bus, '/ctl')
        self.manager = manager

    @dbus.service.method('org.bead.FakeBluez', in_signature='ss')
    def Set(self, key, value):
        adapter = self.manager.nodes[ADAPTER]
        if key == 'power':
            adapter.change('org.bluez.Adapter1', 'Powered', dbus.Boolean(value == 'on'))
        elif key == 'discovering':
            if value == 'on':
                adapter.StartDiscovery()
            else:
                adapter.StopDiscovery()
        elif key == 'reset':
            self.manager.reset()


class Manager(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, '/')
        self.bus, self.nodes = bus, {}
        self.agents, self.control = Agents(bus, '/org/bluez'), Control(bus, self)
        self.load()
    def load(self):
        for path, ifaces in start_state().items():
            self.add(path, ifaces)

    def add(self, path, ifaces):
        self.nodes[path] = Node(self.bus, path, ifaces, self)
        self.InterfacesAdded(path, ifaces)

    def nearby(self):
        path = ADAPTER + '/dev_AA_00_00_00_00_05'
        if path not in self.nodes and self.nodes[ADAPTER].ifaces['org.bluez.Adapter1']['Discovering']:
            self.add(path, device('AA:00:00:00:00:05', 'Nearby speaker', 'audio-card', False, False, None, -61))
        return False

    @dbus.service.method(OM, out_signature='a{oa{sa{sv}}}')
    def GetManagedObjects(self):
        return dbus.Dictionary({dbus.ObjectPath(p): dbus.Dictionary({i: dbus.Dictionary(v, signature='sv') for i, v in n.ifaces.items()},
                                                                    signature='sa{sv}')
                                for p, n in self.nodes.items()}, signature='oa{sa{sv}}')

    @dbus.service.signal(OM, signature='oa{sa{sv}}')
    def InterfacesAdded(self, path, ifaces):
        pass

    @dbus.service.signal(OM, signature='oas')
    def InterfacesRemoved(self, path, ifaces):
        pass

    def reset(self):
        for path in list(self.nodes):
            if path != ADAPTER:
                node = self.nodes.pop(path)
                self.InterfacesRemoved(path, list(node.ifaces))
                node.remove_from_connection()
        fresh = start_state()
        adapter = self.nodes[ADAPTER]
        for name, value in fresh[ADAPTER]['org.bluez.Adapter1'].items():
            if adapter.ifaces['org.bluez.Adapter1'][name] != value:
                adapter.change('org.bluez.Adapter1', name, value)
        for path, ifaces in fresh.items():
            if path != ADAPTER:
                self.add(path, ifaces)


def main():
    import os
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    assert address and f'/run/user/{os.getuid()}/bus' not in address, 'a private bus only'
    bus = dbus.SessionBus()
    names = [dbus.service.BusName('org.bluez', bus), dbus.service.BusName('org.bead.FakeBluez', bus)]
    Manager(bus)
    print('fake org.bluez ready', len(names), flush=True)
    GLib.MainLoop().run()


if __name__ == '__main__':
    sys.exit(main())
