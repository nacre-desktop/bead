# 0244 — Power profiles service

`bead_services::power_profiles` is a separate system-bus service for the public power-profiles-daemon interface. It prefers `org.freedesktop.UPower.PowerProfiles`, falls back to `net.hadess.PowerProfiles`, and publishes `available`, `ActiveProfile`, ordered `Profiles`, and `PerformanceDegraded` through snapshots and subscriptions. It refreshes on `PropertiesChanged` and `NameOwnerChanged`; `SetProfile` accepts only a profile in the current daemon list.

Folder apps opt in with `[services].desktop = [..., "power"]`. The host exposes `desktop.power.available`, `desktop.power.profile`, `desktop.power.profiles` (daemon order), and `desktop.power.degraded`, plus `desktop.power.set` and `desktop.power.cycle` (wraps in daemon order).

Out of scope: profile holds, automation or policy, and UPower battery/device details. The service remains separate from `Config` and `Snapshot`. No temporary duplicate implementation was introduced.

Evidence: [power-profiles-daemon D-Bus API](https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html), Bead service patterns, private-bus owner/property/action tests, and folder projection/routing tests. No UI parity capture applies: this adds an example folder-app button, not a framework surface.
