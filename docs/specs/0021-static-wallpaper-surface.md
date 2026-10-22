# B0021 — Static wallpaper surface

Status: **Blocked on published selection/presentation contract**. Spec first;
implementation must not infer wallpaper state from filesystem paths.

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

The current published `wallpaper.watch` topic contains only `available`, opaque
source `generations`, and optional `error`. It signals input changes but does
not identify the selected image or publish its presentation/palette. Nacre's
`wallpaper/last.txt`, `presentation.json`, and helper `--stream-presentation`
are implementation details, not published contracts. Before product code,
use a published daemon method/topic that provides the active static image and
presentation, and the published activation/palette route. If no such contract
is available, stop here and request that migration publish it; do not read or
write Nacre private state files from Bead and do not add a Bead-owned writer.

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
  source copies. Report settled and transition PSS per output when measurable.
- Cover initial state, switch, palette pairing, invalid image, daemon loss and
  recovery, two outputs, output removal, and idle resource release with focused
  tests/proofs. Run the full desktop CI job in the dev-1 Arch container.
- Run a private nested smoke on the owner's desktop only after acquiring
  `~/nacre-lead/desktop-heavy.lock`; never use the laptop. Do not install,
  publish, or push.

## Validation boundary and duplicates

Build/CI only in the task's uniquely named dev-1 Arch container, with `nice -n 10` and Cargo `-j2`; remove its container, image, and build directory after.
Nested proof only on the owner's desktop under the lock and private HOME. No
live desktop, installed Nacre, config, keybinding, autostart, or unit changes.

No temporary duplicate is planned. If implementation retains a compatibility
path or fallback beside its replacement, list it here with the exact rule:
delete N days after the replacement is trusted.

## Reference audit

Nacre `origin/main` at `aa32dbf82b66570c74b3774101f954de7ce0367a`: one background
layer surface per screen; global active poster; asynchronous output-sized
`PreserveAspectCrop`; old image retained until decode, then a short fade; image
activation controls pairing with presentation colors. The QML picker and helper
own selection and persistence. The published `wallpaper.watch` topic is only a
debounced source-generation signal.
