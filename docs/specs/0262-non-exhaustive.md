# 0262 — Non-exhaustive host configuration and snapshot types

Status: Bead side done. Nacre migrates in the same commit as its next Bead pin bump. This is step 2 of
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

## Nacre migration

Re-checked against Nacre origin/main `248b691c`, which pins Bead `a6fbc10f`. All paths are under
`crates/nacre-shell/src/`. There are 26 sites in 13 files, about −40 net lines. Bump all five Bead pins,
`Cargo.lock` and `tools/check_product_shell.py` `BEAD_REVISION` in the same commit. `nacre-helper` uses none of these types.

| Site | Before | After |
|---|---|---|
| `desktop.rs:11` (prod) | `start_config(Config { audio: false, bus, tray_watcher: false })` | `let mut config = Config::default(); config.audio = false; config.bus = bus; start_config(config)` |
| `reload/widget_drag_preview_tests.rs:1206`, `:1472`; `service_views_tests.rs:609`, `:1926`; `tray.rs:261` | `Desktop::start_config(Config { tray_watcher: false, audio: false, bus: false })` | `crate::desktop::start_services(false)`: same values, and `widgets.rs:141`, `:163` already call it |
| `desktop.rs:255` | `let snapshot = Snapshot { clock, clock_seconds, date, .. }` | `let mut snapshot = Snapshot::default();` + 3 assignments; the `LocalDate { … }` literal stays |
| `desktop.rs:510`, `:631`; `tray.rs:194`, `:384` | `let [mut] s = Snapshot { tray: vec![TrayItem { … }], .. }` | `let mut s = Snapshot::default(); s.tray = vec![TrayItem { … }];` (the fixture is unchanged) |
| `desktop.rs:572` | `let snapshot = Snapshot { tray: vec![item.clone()], .. }` | `let mut snapshot = Snapshot::default(); snapshot.tray = vec![item.clone()];` |
| `desktop.rs:598` | `&Snapshot { tray: vec![item], .. }` | `&snapshot` (same value) |
| `desktop.rs:607`–`:618` | `let mut missing = snapshot.tray[0].clone(); missing.icon_name = …; … &Snapshot { tray: vec![missing], .. }` | `let mut missing = snapshot.clone(); missing.tray[0].icon_name = …; … &missing` |
| `tray.rs:275` | `let snapshot = Arc::new(Snapshot { tray: vec![…], .. })` | `let mut snapshot = Snapshot::default(); snapshot.tray = vec![…]; let snapshot = Arc::new(snapshot);` |
| `tray.rs:345`, `:356` | inline `Arc::new(Snapshot { revision: 3, .. })`, then `{ revision: 2, tray: vec![…], .. }` | `let mut next = Snapshot::default(); next.revision = 3;` → `Arc::new(next.clone())`; then `next.revision = 2; next.tray = vec![…];` → `Arc::new(next)` |
| `status_views_tests.rs:15` | `bead::services::Snapshot { network, bluetooth, battery, .. }` | `Snapshot::default()` + 3 assignments; the nested literals stay |
| `main.rs:193` | `Options { command_map: Some(…), ..Options::default() }` | `let mut options = Options::default(); options.command_map = Some(…);` |
| `app.rs:883` | `Options { namespace, command_socket, output, panel_output, interactive, keyboard_focus, trace_state, app, app_as_pearl, .. }` | `Options::default()` + 8 assignments (`keyboard_focus` is already `true`). Keep the B0194 comment, and keep `output.clone()` before `panel_output` moves `output` |
| `layer_motion_wayland_tests.rs:230` | `Options { namespace, panels, app, panel_output, seconds, renderer, interactive, keyboard_focus, .. }` | `Options::default()` + 5 assignments: `seconds: None`, `renderer: Cpu` and `keyboard_focus: true` are the defaults |
| `brain.rs:837` | `Ok(WindowSpec { … 10 fields })` | `let mut spec = WindowSpec::new("nacre-brain", tr(…), size, fallback, factory); spec.min_size = min_size; spec.on_resize = …; spec.model = …; Ok(spec)` |
| `colors.rs:1241` | `Ok(WindowSpec { … })` | `WindowSpec::new(…, (1100, 760), fallback, factory)` + `min_size = (760, 560)`, `model` |
| `settings.rs:1670` | `Ok(WindowSpec { … })` | `WindowSpec::new(…)` + `min_size`, `size_for_output = Some(…)`, `model` |
| `welcome.rs:264` | `Ok(WindowSpec { … })` | `WindowSpec::new(…, (1100, 800), …)` + `model`; `min_size` equals `size`, which is the default |
| `widgets.rs:168` | `Ok(WindowSpec { … })` | `WindowSpec::new(…, factory)` + `command_targets = vec!["bead.widgets".into()]`, `min_size`, `size_for_output`, `on_resize`, `model` |

For each `WindowSpec` site, drop the three fields the constructor defaults: `command_targets: Vec::new()`,
`size_for_output: None` and `on_resize: None`. Factory closures move into the fifth argument. The
argument's type is `WindowFactory`, so closure signature inference works exactly as it did in field
position.

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
