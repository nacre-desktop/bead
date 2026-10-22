# B0021 — Static wallpaper surface

Status: **Contract unblocked; implementation complete; further nested validation blocked by the owner memory gate**. Nacre N0027 is published. Consumer pins
`nacre-protocol` at Nacre commit `294d3f4034e0ffc8c6ee4fe38a3ed4109aa56c84`
(`nacre-protocol` crate `0.1.0-dev.2`, wire contract v1). No wallpaper state is
inferred from filesystem paths; only the contract's validated local path is used.

## Objective and scope

Add the Nacre static wallpaper as a passive background layer surface in
`nacre-shell`. Show the daemon/config-owned active image on each output using
Nacre's current global wallpaper behavior (same selected image on every
output), decoded at that output's pixel size and scaled with fill/crop. Create
one bottom-layer, input-transparent surface per output; remove its decoded
image and surface resources when that output disappears.

When the wallpaper changes, load and validate the new image before presenting
it, keep the old image visible on load failure, and crossfade successful
switches in the same short interval as Nacre when motion is enabled. Refresh
palette/theme only through a published protocol route, paired with activation
of the displayed image. Persistence belongs to the daemon/config; Bead adds no
config writer. Read-only Bead dependency on the MIT OR Apache-2.0
`nacre-protocol` crate is permitted.

## Contract gate

Subscribe to `wallpaper.active` v1 and invoke `wallpaper.activate({revision})`
v1. Pin protocol types, schemas and goldens through the exact dependency revision
above. Treat unavailable/stale/malformed state as a retain-last-good event.
Never read Nacre private state files or add a Bead-owned writer.

## Non-goals

Video, animated images, wallpaper effects, wallpaper picker UI, per-output
wallpaper selection, wallpaper/config mutation, palette generation, or any
Nacre-specific appearance or compositor policy in `bead-*`. Keep compositor
details behind existing adapters and record any new framework assumption in
the roadmap generalization list only if implementation demonstrates one.

## Acceptance

- Contract use is pinned to published `nacre-protocol` types/schema and golden
  transcripts; unavailable, stale, malformed, or disconnected state preserves
  the last good image and palette.
- Hyprland and niri each show the selected static image on every output, with
  output-sized decode, Nacre fill/crop, no input, bottom background layer,
  correct output hotplug removal, and palette refresh paired with image
  activation. Wallpaper switches do not flash blank; failed decodes retain the
  previous image.
- Image memory is bounded to one decoded current image per output plus at most
  one output-sized transition image while switching; no retained full-size
  source copies. Once the compositor sends `wl_buffer.release`, destroy the
  client buffer and SHM pool/memfd, and retain only wallpaper metadata. If a
  compositor does not release within 3 seconds, log its identity and preserve
  the existing SHM resources there. Animated switches securely reopen and
  decode the previous metadata pair at output size for the transition; they do
  not rely on a settled raster copy. Reconfiguration securely reopens and
  decodes at the new output size. Report settled and transition PSS per output
  when measurable.
- Cover initial state, switch, palette pairing, invalid image, daemon loss and
  recovery, two outputs, output removal, and idle resource release with focused
  tests/proofs. Run the full desktop CI job in the dev-1 Arch container.
- Run a private nested smoke on the owner's desktop only after acquiring
  `~/nacre-lead/desktop-heavy.lock`; never use the laptop. Do not install,
  publish, or push.

## Validation boundary and duplicates

Build/CI only in the task's uniquely named prod Arch container, with `nice -n 10` and Cargo `-j2`; remove its container and build directory after.
Nested proof only on the owner's desktop under the lock and private HOME. No
live desktop, installed Nacre, config, keybinding, autostart, or unit changes.

No temporary duplicate is planned. If implementation retains a compatibility
path or fallback beside its replacement, list it here with the exact rule:
delete N days after the replacement is trusted.

Build/runtime addition: GdkPixbuf 2.x and the PNG, JPEG, WebP, GIF and TIFF
loader modules provide streamed decode-time scaling from the validated file
descriptor. The CI package adds the GdkPixbuf development files; deployments
must include the same five loader modules. Output rasters and any scaled decode
intermediate are capped at 64 MiB and 16,384 pixels per axis, which covers 4K
outputs. Larger outputs fail closed; the 100-million source-pixel contract cap
continues to apply independently.

## Generalization list

The first implementation must record any Nacre assumption demonstrated inside
framework crates in `docs/ROADMAP.md`; keep wallpaper selection, palette policy,
and contract handling in `nacre-shell`.

## Reference audit

Nacre `origin/main` at `aa32dbf82b66570c74b3774101f954de7ce0367a`: one background
layer surface per screen; global active poster; asynchronous output-sized
`PreserveAspectCrop`; old image retained until decode, then a short fade; image
activation controls pairing with presentation colors. The QML picker and helper
own selection and persistence. The published `wallpaper.watch` topic is only a
debounced source-generation signal.
