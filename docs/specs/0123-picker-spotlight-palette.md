# B0123 — Spotlight overlay, first content and synchronized palette

Status: implementation and remote validation in progress.

Spotlight floats a centered picker card over the live desktop, using the existing
palette, typography and controls. Its root is a translucent dimmer; it no longer
paints a selected-wallpaper gallery backdrop. Optional compositor blur is not needed.
Layout, dimensions, spacing and motion remain in Luau and theme tokens.

## First content and retention

A catalogue-loading scene is not a content frame. The worker commits its model
privately so hydration can advance, but publishes pixels only after the visible
thumbnail requests settle (or an explicit service error). The panel no longer
falls through to a blank fallback after 750 ms. Worker failures still publish
an error scene. The single resident picker stays prepared after closing, beyond
the ordinary 120-second panel expiry; opening another panel replaces it normally.
This intentionally spends warm memory under SMOOTHNESS FIRST. Never-opened state
is unchanged; there is no new always-on catalogue or image cache.
Budget: ≤1 MiB additional never-opened unreclaimable memory. Warm retention is
explicitly a smoothness trade rather than an always-on feature allocation;
its measured retained cost is reported separately (allowed by SMOOTHNESS FIRST).

General framework primitives: `PanelSpec.overlay` and `retain_when_closed`.
The latter retains only the existing single panel slot, not a pool of hidden VMs.

## Palette

B0109 deliberately showed an optimistic wallpaper with the previous palette.
Selection now remains a generation/identity fence only: new pixels come from the
confirmed image/palette pair, through the existing prepare/accept/commit fence.
A different palette snaps the wallpaper in the same coordinated turn; same-palette
changes retain their crossfade, attaching the first target blend before acknowledgment.
Motion waits until every output using that source commits its poster/palette pair. Nacre warms
its existing settings/file-identity-keyed palette markers for explicitly hydrated
catalogue ids, using bounded work and bounded marker retention; see paired commit.
Never-seen files retain the async extraction path and the old scene until ready.

## Acceptance evidence

Pending: full remote CI; regenerated and inspected Spotlight goldens; matched
private QML/Bead captures; cold/warm/after-120s first-content timings; palette
versus wallpaper timing; five alternating memory pairs. Bead renderer: CPU.
No live desktop, installation, system configuration, or permissions changes.

## Temporary duplicates

None added. The optimistic preview raster/token path is removed; the bounded
selection channel remains because superseded activations still need identity fences.
