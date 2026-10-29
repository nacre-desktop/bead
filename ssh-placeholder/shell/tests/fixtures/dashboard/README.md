# Dashboard preview data

These artificial records follow the reserved `dashboard.weather`,
`dashboard.media`, `dashboard.system` and `dashboard.updates` contracts in
Nacre revision `c2b26d989d4e1c66a64b93d30af6fe19a9b83c62`.
They are not observations of a desktop, a weather provider or a package database.
Tests validate them with the pinned protocol's Rust types and stable-ID rules.

The preview requires an explicit `--dashboard-fixtures` directory and labels
the pages “Preview data”. Without it, the existing shared daemon link leases
only advertised topic version 1. A reserved schema does not enable a service.
Missing or stale readings remain unknown or visibly stale; no actions are
simulated. The media fixture represents lyrics enrichment, not a playback
identity or a working player. CPU seconds are cumulative process time.

Each page reads only its own data when its worker starts. System also reads the
updates fixture. All page modules, snapshots and subscriptions retire with the
page worker after close and idle. No process scan, network fetch or timer is
implemented in these views.

Each entry has its own bundle folder and recovery key. Four relative source
links select only that page plus the common frame, components and behavior.
The loader checks that link targets remain inside the UI root and snapshots
their bytes, so a last-known-good bundle is self-contained without other pages.
