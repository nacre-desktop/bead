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

Retained renderer preview, safely inside a nested window:

```sh
BEAD_RENDERER=femtovg ./scripts/dev.sh niri
```

Use `cpu` for the no-GPU fallback or `vello` for the comparison. `auto` attempts
femtovg and falls back to CPU if graphics initialization fails. The CPU-only
build is `cargo build -p nacre-shell --no-default-features`. No GPU context is
created in the CPU path. Renderer choice remains explicit pending the owner.

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
DRM totals are reported per client/region after descriptor deduplication; do not
add them to PSS or treat them as exclusive VRAM on an integrated GPU.

Vello returns straight-alpha output; its existing transfer blit converts it to
premultiplied alpha for Wayland. Private static screenshots match CPU/femtovg
pixel for pixel after this conversion.
