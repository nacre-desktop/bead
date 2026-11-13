# 0246 — Bluetooth devices service

`bead_services::bluetooth` is an independent event-driven client of BlueZ on the system bus. It obtains the initial object tree with `org.freedesktop.DBus.ObjectManager.GetManagedObjects`, follows `org.bluez` ownership, and refreshes its cache on object-manager and Properties signals. Snapshots expose adapters, devices, optional Battery1 percentage and optional Device1 RSSI. Actions cover adapter power/discovery and device connect, disconnect, pair, trust and removal; paths must exist in the current snapshot. Discovery is stopped after 30 seconds unless explicitly stopped sooner.

Folder apps opt in with `[services].desktop = ["bluetooth_devices"]`, which publishes `desktop.bluetooth_devices.available`, `powered`, `discovering`, and `devices`, and registers power, scan, connect, disconnect and forget actions. The first sorted adapter is the default for the adapter-level actions. The existing `bluetooth` desktop summary is unchanged.

Pairing that needs a PIN, passkey, or confirmation agent is out of scope; BlueZ's pairing errors are returned with an agent-required explanation. OBEX, file transfer, and audio profile selection are out of scope. No temporary duplicate implementation was introduced.

Evidence: [BlueZ Adapter1](https://bluez.readthedocs.io/en/latest/adapter-api/), [Device1](https://bluez.readthedocs.io/en/latest/device-api/), [Battery1](https://bluez.readthedocs.io/en/latest/battery-api/), and the [BlueZ 5 ObjectManager overview](https://www.bluez.org/bluez-5-api-introduction-and-porting-guide/). This design uses the documented public interfaces only; no application source was consulted.
