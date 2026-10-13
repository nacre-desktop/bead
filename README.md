# Bead + Nacre Shell

Feasibility experiment: a Rust Wayland shell framework with atomic hot
reload, and the Nacre desktop shell rebuilt on it. Not a replacement for the
installed Nacre shell.

ADR 0001 is approved. Feature 2 now has retained CPU, femtovg/wgpu and Vello composition for comparison.
Each feature is measured and reported before the next starts. See [current status](docs/STATUS.md) and
[feasibility log](FEASIBILITY.md).

Planned licensing: `crates/bead-*` is MIT OR Apache-2.0. `crates/nacre-shell`
and `shell/` retain Nacre's GPL-3.0 terms. Each crate must set
`publish = false`. Research tooling and measurement spikes are MIT OR Apache-2.0
unless an upstream notice specifies otherwise. Framework code and shell assets have separate license boundaries.

The repository is public by owner request. Do not publish crates/packages/releases.

Feature 2 development preview (from a Wayland terminal):

```sh
./scripts/dev.sh niri
```

A separate nested desktop opens with a launcher-sized drawing preview that
slides open and closed. It demonstrates fonts/emoji, SVG/PNG/icons, rounded
corners, a border and a cached shadow. It is not a working launcher. Close its window or
press Ctrl+C to stop. For the other compositor use `./scripts/dev.sh hyprland`.
Neither command replaces installed Nacre or changes session configuration.

CI checks fmt, clippy, tests and cargo-deny. Framework dependency graphs use
`deny-framework.toml`; the complete workspace, including GPL nacre-shell, uses
`deny.toml`. No new dependency/license ledgers are maintained.

The completed live 120 Hz stress is recorded in FEASIBILITY. Live benchmark
panels were stopped at the owner's request; use nested displays for further
checks. For a private nested loaded-font idle comparison:
`python spikes/measure.py --panel --modes bead qml --runs 1`.
An explicit `--palette PATH` reads an existing Orient format 1 JSON file. The
checked-in preview palette contains colors/mode from an existing Orient output
matching the active dark palette. The stress fixture preserves the measured light
palette. Both omit wallpaper metadata and personal paths. No color engine is copied.

Stateful hot-reload demo, safely inside a nested window:

```sh
./scripts/dev.sh niri --demo
```

Click Search and type; scroll the application rows; click Home to collapse/open.
Every eight seconds the demo edits a temporary UI copy: valid change, syntax
error, then recovery. Your input/scroll/open state should survive all three.
Close the nested window or Ctrl+C to stop. Use `hyprland` for the other compositor.
Without `--demo`, edit `shell/ui/*.json` or `behavior.luau` yourself while it runs.

`auto` (default) uses one native OpenGL/femtovg context shared by all surfaces,
with tiny-skia if hardware GL initialization is unavailable. `BEAD_RENDERER=cpu`
selects the no-GPU path; `femtovg` selects wgpu for comparison, and `vello` supports
only the retained stress fixture. The CPU-only build is
`cargo build -p nacre-shell --no-default-features`; it creates no GPU context.
GPU content is currently CPU-baked once and uploaded, then composited on GPU.

UI v1 is a small, strict JSON primitive format, not complete QML parity. Stable
node/store IDs, typed Luau stores/actions/computed bindings, included JSON and
asset snapshots are staged before all configured surfaces swap. Failures show
file/line/column and keep the previous UI; fixing the source clears the overlay.
Store values (including input, open state, scroll and focused ID) outlive views.
Computed bindings must be pure; state changes belong in named actions. Luau has
an 8 MiB VM limit and a 50 ms action/binding-batch execution limit. Directory and
source bounds prevent accidental unbounded bundles. Full controls, layout,
animations, IME and accessibility remain later features. Reload still bakes on
the UI thread; the measured 80–103 ms interruption needs reducing for larger UIs.

Private measurement round (no live desktop surfaces):

```sh
python spikes/measure.py --retained --compositor hyprland --virtual120 --runs 2 --seconds 60
```

This reuses the existing private compositor harness and a virtual output at
2880×1800, scale 1.5, 120 Hz. It compares full/content surfaces and first/later
launches. Raw logs stay ignored; summaries print to the terminal. The compositor
schedules virtual frames; these are not physical scanout measurements. First
launch has a fresh private shader cache per renderer/size; later reuses it.
System font/filesystem caches are not cleared. Never use the live stress script
without first asking the owner.

The immutable panel picture is CPU-baked once for all three variants, including
text/emoji and a small Gaussian shadow template stretched along straight edges.
GPU variants upload that picture once and composite position/opacity each frame;
this round compares retained composition, not GPU text/vector generation.
GPU draw times mean CPU encoding/submission time, exclude acquisition and do not
measure asynchronous GPU execution. Callback timing measures client scheduling.
Memory comparisons now use **PSS + DRM total**, with DRM descriptors deduplicated
by client/region. This requested sum can overlap shared system-memory accounting
and is not a measure of exclusive VRAM; component values are retained in FEASIBILITY.

Vello returns straight-alpha output; its existing transfer blit converts it to
premultiplied alpha for Wayland. Private static screenshots match CPU/femtovg
pixel for pixel after this conversion.
