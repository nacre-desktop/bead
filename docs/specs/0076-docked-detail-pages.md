# B0076 — Docked device detail pages

Status: implementation and validation in progress.

The control dock hosts the existing Wi-Fi, Bluetooth, battery and audio Luau
builders with a Back header, its existing framed viewport and scroll policy.
Standalone bar entry points call those same builders. No second surface, runtime,
provider or action engine is introduced. The shell composes existing service
adapters while the control view exists; all adapters, leases and body modules
retire with that view after closed+idle.

The default order follows QML’s complete catalog, including mic volume before
brightness. Unsupported hardware/action tiles are hidden; supported but currently
unavailable tiles keep their explanatory state. Layout uses shared theme tokens.

General primitive: `kit.SliderTile.on_detail` adds an optional detail affordance,
leaving the slider and toggle actions intact. No Runtime framework API was added.

Validation, parity measurements and known gaps: pending remote checks.
Temporary duplicates: none; standalone files are entry points to shared builders.
