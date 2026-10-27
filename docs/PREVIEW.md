# Preview and research notes

This page keeps the detailed development-preview and measurement instructions
that are useful when working on Bead. For an overview, current project status,
build instructions, and architecture, see the [project README](../README.md).

## Nested development preview

Run from a Wayland terminal with the required compositor tools available:

```sh
./scripts/dev.sh niri
```

This builds `nacre-shell` in release mode and opens a separate nested niri
desktop with a launcher-sized drawing preview. It demonstrates fonts and emoji,
SVG/PNG/icons, rounded corners, a border, and a cached shadow; it is not a
working launcher. Close the nested window or press Ctrl+C in the launching
terminal to stop. For Hyprland, run `./scripts/dev.sh hyprland`.

The stateful UI demo runs inside the nested window:

```sh
./scripts/dev.sh niri --demo
```

Click Search and type, scroll the application rows, and click Home to collapse
or reopen. Every eight seconds the demo edits a temporary UI copy with a valid
change, a syntax error, and a recovery. Input, scroll, and open state should
survive all three. Close the nested window or press Ctrl+C to stop; use
`hyprland` for the other compositor. Without `--demo`, edit `shell/ui/*.luau`
or imported component files while the preview runs.

## Full Nacre Shell preview

The private nested preview script starts the whole shell with a standalone
daemon and fixture data. Supply the helper and shell executable explicitly:

```sh
python3 scripts/try-bead.py \\
  --daemon /path/to/nacre-helper \\
  --bead /path/to/nacre-shell
```

It uses private niri, D-Bus, runtime directories, and HOME; it does not change
the live session, install anything, or use real account or device data. The
system application catalog is read-only and personal file/history data starts
empty. The shell preview has launcher, dashboard, settings, AI sidebar, session,
wallpaper and other routes; its fixture daemon does not provide real network,
audio, battery, or account state. See `python3 scripts/try-bead.py --help` for
options such as `--headless-smoke` and explicit dashboard fixtures.

## Retained-rendering preview

The private retained-rendering stress command runs a nested Hyprland session
with a virtual output at 2880×1800, scale 1.5, and synthetic 120 Hz scheduling:

```sh
python3 spikes/measure.py --retained --compositor hyprland --virtual120 --runs 2 --seconds 60
```

It compares full/content surfaces and first/later launches. Each renderer and
size gets a fresh private shader cache for the first launch; later launches
reuse it. System font and filesystem caches are not cleared. Virtual compositor
scheduling is not physical scanout timing.

The immutable panel image is CPU-baked once for all three variants, including
text/emoji and a small Gaussian shadow template stretched along straight edges.
GPU variants upload the image once and composite position/opacity per frame;
this compares retained composition, not GPU text/vector generation. GPU draw
times measure CPU encoding/submission and exclude asynchronous GPU execution.
Callback timing measures client scheduling.

Memory figures in the research log report PSS and DRM separately and together;
DRM descriptors are deduplicated by client/region. The requested sum can overlap
shared system-memory accounting and is not exclusive VRAM. Keep component values
when comparing runs. See [FEASIBILITY.md](../FEASIBILITY.md) for measurements
and limits.

Bars use CPU rendering. Current sound/launcher panel specifications also choose
CPU; explicit native GL companions can start lazily and release on idle. The
policy is CPU for small panels and GL for large animated surfaces. Native
contexts share their EGL transport, but Mesa may retain about 20 MiB after
teardown. `BEAD_RENDERER=cpu` selects the no-GPU path; `femtovg` selects wgpu
for comparison, and `vello` supports the retained stress fixture. The CPU-only
build is:

```sh
cargo build -j2 -p nacre-shell --no-default-features
```

GPU content is CPU-baked and uploaded before GPU composition. Results and
limitations are recorded in [project status](STATUS.md) and
[FEASIBILITY.md](../FEASIBILITY.md).

## Other private checks

The loaded-font panel comparison runs only in a nested compositor:

```sh
python3 spikes/measure.py --panel --modes bead qml --runs 1
```

`spikes/measure.py --palette PATH` accepts an existing Orient format 1 palette
JSON. The tracked `spikes/palette-stress.json` is the light-palette stress
fixture; preview palette data omits wallpaper metadata and personal paths. Bead
uses the existing palette format and does not copy Orient's color engine.

The full-shell preview accepts `--headless-smoke` to exercise its private
compositor path and `--dashboard-fixtures` to show labeled artificial dashboard
data without dashboard backend calls. Its `--legacy` mode is the old foundation
preview, with a separate notice process and without wallpaper, frame, or
Settings. These are preview/test modes, not install or shell-switch commands.

The Vello comparison path converts Vello's straight-alpha pixels to Wayland's
premultiplied-alpha form during transfer. The retained comparison's private
static screenshots matched CPU and femtovg pixel-for-pixel after that
conversion; this describes the recorded comparison fixture, not all rendering
paths.
