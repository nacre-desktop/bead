# 0262 — Non-exhaustive host configuration and snapshot types

Status: done; the host migrated with its Bead pin bump. This is step 2 of
[0238 §6](0238-api-stability.md#6-recommended-next-units).

## What changed

- `#[non_exhaustive]` on `bead_services::{Config, Snapshot}`, `media::MediaSnapshot`,
  `power_profiles::PowerProfilesSnapshot`, `bluetooth::BluetoothSnapshot`, `network::NetworkSnapshot`, and
  `bead_wayland::{Options, WindowSpec}`. Enums stay exhaustive.
- `WindowSpec::new(app_id, title, size, fallback, factory)`. It sets `min_size` to `size`, leaves
  `command_targets` empty and `size_for_output`/`on_resize` unset, and maps every `open` to an empty store
  patch. The fallback scene is required, so a forgotten fallback cannot compile into a blank first frame.
- Outside the defining crate, a struct expression is now error E0639, including `..Default::default()` and
  `..other`. Reading and assigning fields still works: `let mut x = T::default(); x.f = v;`. For
  `WindowSpec`, call `WindowSpec::new` and assign the optional fields.
- Each marked type has a `compile_fail` doc test that writes the struct literal from outside the crate. If
  someone deletes the attribute, the snippet compiles and the doc test fails. Checked: with all eight
  attributes removed, all eight doc tests fail.
- Bead call sites changed: `bead` host and source code and tests (`desktop_config`, the five service snapshot
  tests, the clock and desktop-view tests), the five `Config` literals in bead-services integration tests,
  `bead-cli` `run`, `bead --example minimal`, and `examples/minimal-shell`.

Why: before this change, adding a field to any of these types broke every host that built it with a struct
literal (0238 §2). Now new fields are additive for hosts.

## Deferred

These types have the same literal hazard. They are left for later units:

- `bead_wayland::PanelSpec`: no `Default`, about 20 Nacre literals and one in `bead-cli`.
- `TrayItem`, `MenuEntry` and the other nested service records (`Audio`, `Battery`, `Session`, `Network`,
  `Bluetooth`, `Player`, and the bluetooth and network rows). Nacre builds `TrayItem` in production. This
  waits for the facade inventory (0238 §6.1).
- `PearlOptions` and `NativePearlOptions`.
- `notification_bus::{Options, Config}`.
- `bead_backends::Snapshot`, which Nacre's compositor adapters produce.
- `Scene` encapsulation (0238 §6.3).
