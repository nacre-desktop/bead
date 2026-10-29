#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Two realistic SNI items for the private bar parity bus; never run on the live bus."""
import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

SNI = 'org.kde.StatusNotifierItem'
PROPS = 'org.freedesktop.DBus.Properties'
WATCHER = 'org.kde.StatusNotifierWatcher'


class Item(dbus.service.Object):
    def __init__(self, bus, name, title, icon):
        self.name = dbus.service.BusName(name, bus)
        super().__init__(self.name, '/StatusNotifierItem')
        self.props = {
            'Category': 'Hardware', 'Id': title, 'Title': title, 'Status': 'Active',
            'IconName': icon, 'IconPixmap': dbus.Array([], signature='(iiay)'),
            'AttentionIconName': '', 'AttentionIconPixmap': dbus.Array([], signature='(iiay)'),
            'OverlayIconName': '', 'OverlayIconPixmap': dbus.Array([], signature='(iiay)'),
            'ToolTip': dbus.Struct(('', dbus.Array([], signature='(iiay)'), title, ''), signature='sa(iiay)ss'),
            'ItemIsMenu': False, 'Menu': dbus.ObjectPath('/NO_DBUSMENU'),
        }

    @dbus.service.method(PROPS, in_signature='s', out_signature='a{sv}')
    def GetAll(self, interface):
        return dbus.Dictionary(self.props if interface == SNI else {}, signature='sv')

    @dbus.service.method(PROPS, in_signature='ss', out_signature='v')
    def Get(self, interface, name):
        return self.props[name]

    @dbus.service.method(SNI, in_signature='ii')
    def Activate(self, x, y):
        pass


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    items = [Item(dbus.SessionBus(private=True), 'org.bead.BarBluetooth', 'blueman-applet', 'bluetooth-active'),
             Item(dbus.SessionBus(private=True), 'org.bead.BarNetwork', 'nm-applet', 'network-wireless-signal-excellent')]

    def register():
        if not bus.name_has_owner(WATCHER):
            return True
        for item in items:
            dbus.Interface(item.name.get_bus().get_object(WATCHER, '/StatusNotifierWatcher'), WATCHER).RegisterStatusNotifierItem(item.name.get_name())
        print('registered two private bar SNI items', flush=True)
        return False

    GLib.timeout_add(250, register)
    GLib.MainLoop().run()


if __name__ == '__main__':
    main()
