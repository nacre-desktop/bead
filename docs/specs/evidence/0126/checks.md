# B0126 validation record

## Shipped continuation

The baseline and rejected-patch rows below are historical. The owner subsequently
accepted the thumb candidate; default-on `312c63e7`, readiness `c7772734`, shared
paint `e2d95769` and local values `3731c52e` are shipped. Each improvement passed
the full Arch job before safe-push. The continuation invocation was
`RUST_TEST_THREADS=2 nice -n 10 bash /task/ci-canonical.sh`, with Cargo `-j2`.
It replicates the workflow coverage listed below, including private lifecycle
probes and all independent cargo-deny graphs. All three canonical logs end with
`CI_ALL_OK`; their exit markers are zero.

| Snapshot | Full job | Log SHA-256 |
| --- | --- | --- |
| Readiness, clean canonical repeat | pass | `ef120251d746b8133f2b7fa61fa786ebccff0d995f022be691932d414654b0c2` |
| Shared paint | pass | `2a31065392e739960935f497b47350d710e294859fbef4d22b74f933fdee9f8a` |
| Local values | pass | `2df985b385eb844a0178c2639218c9c1f2512da266b5fc240e4368ab1384dde0` |

Canonical script SHA-256:
`24bcac9bf920d04d4aaa60bc74b1c566a26935138417e2addb0e35b97a1d7053`.
All 818 tracked `crates/**` and `shell/**` blobs in the final validation snapshot
`86190a12d03dfa11af666c07a0dd29c330c681b6` match shipped `3731c52e` exactly.
The archive's `REVISION` text is stale; the blob comparison supplies provenance.
Other differences are docs, metadata and the private-preview script's inclusion
of pearl keybindings, outside these product changes. Published exact-commit
GitHub CI also passes for [shared paint](https://github.com/nacre-desktop/bead/actions/runs/40240217079)
and [local values](https://github.com/nacre-desktop/bead/actions/runs/40242542837).
The readiness run was superseded by a later push; its full Arch validation passed.

Shared raster tests cover all 101 poses at 1×/1.5×, transparent corners, BGRA/RGBA
and the borrowed target tail. Descriptor origin tests cover 1×/1.5×/2×. Runtime
tests assert zero projection for local values/empty source wakes, no accepted
node or fill-bound changes, final release ordering and immediate explicit close.
A fake-daemon regression proves completed calls and newest queued targets keep
progressing while projection is suppressed. These checks do not claim the nested
25 ms latency gate passed; see the spec's fresh measurements.

The direct private 120 Hz capture reuses the existing headless labwc parent,
verifies physical/logical dimensions and scale, and retains all source, swap,
containment, watchdog and full-window gates. Five matched CPU pairs have stable
per-phase source hashes, alternating order, zero host swap-in and zero process
SwapPss. The paired median unreclaimable deltas pass in all four phases.

The first direct QML arm had no observable slider state through the legacy
mouse-grabber lookup; it is excluded from latency numbers. The measurement-only
`qml-state-probe.patch` observes the unique control under a held press using a
guarded `QPointer`, then reads its real state at `afterSynchronizing`. It changes
neither event acceptance nor QML, and keeps the exact frame/generation/state/
presentation join. The replacement yields 180 accepted joins, no unmatched
coordinates, no rejected positions and 41/41 coherent sampled PNGs. It reports
QML p95 11.979 ms versus Bead 17.291 ms on the direct synthetic proxy; the
previous nested comparison cannot establish this direct-path ordering.
The probe rebuilt successfully with Qt 6.12 base/declarative/Wayland private
headers and wayland-protocols in the CI container; no runtime dependency was
added to the product. Its binary SHA-256 is
`fe762efdcb2205d38935c019532abb5917d441fad551e1835c160e7edefe7046`.
`git apply --check` passes for the retained measurement patch.

Fresh fixture first-key process CPU is 7.101703 ms control / 7.069714 ms final,
both below 8 ms. Each has exactly one key, the typed state, matching generation
and update, 320 fixture apps, 12 favorites, 8 recents, 64 result rows and a
2880×1800 typed screenshot. Source hashes remain stable; host swap-in and process
SwapPss are zero. The first combined driver failed before measurement because
`--ui-provider` rejects `--menu-fixture`; the successful run uses the existing
local-command-socket fixture benchmark. The production guard was preserved.
The component memory and typing data are in `continuation-memory.json`.

The isolated owner preview was rebuilt from the landed product batch; it was
not started. Raw evidence, exact binaries and reproduction tools are retained
in the ignored task directory. Task-owned remote processes, container and work
directories were removed after capture/analysis; no live desktop, installed
Nacre, configuration, keybinding or unit was changed.

## Historical experiment validation

All builds and tests ran in a task-owned Arch Linux container, Rust 1.99.0,
`nice -n 10`, Cargo `-j2`. No build/test/capture ran on the laptop. UI captures
used the private desktop harness and one heavy capture lock per command.

The exact task invocations were `RUST_TEST_THREADS=2 nice -n 10 bash
/task/ci-visible.sh` and `RUST_TEST_THREADS=2 nice -n 10 bash
/task/ci-shipping.sh`. These task-owned scripts expand the workflow commands.

The full `.github/workflows/ci.yml` job was replicated with Arch package
substitutions, including fmt, warnings-denied Clippy, service boundary, workspace
and trace memory-policy tests, no-default/all-feature renderer checks, minimal
facade/headless UI/Luau generated schema, every Python regression suite and
packaging suite, wallpaper smoke, private services/SNI/DBusMenu lifecycles, and
cargo-deny for the workspace and every independent framework graph.
`RUST_TEST_THREADS=2` bounds contention for the existing 50 ms markup deadline;
the initial unconstrained run failed that unchanged test, whose focused retry
and full bounded reruns passed. No test deadline was weakened.

| Source | Full job | Log SHA-256 |
| --- | --- | --- |
| Baseline `61e73507` | pass, `CI_ALL_OK`, exit 0 | `50c14793b136e7333d044dd304f374f42fecda4bc6dfb59f62d347801edde20e` |
| Visible CPU prototype | pass, `CI_ALL_OK`, exit 0 | `ecb8d3b60036b756e5bc3b386bb2229bbdf178486f93da334ce9af4797f7d5ec` |
| GL-pinned prototype | pass, `CI_ALL_OK`, exit 0 | `cca763fd51676ee426687c7b327cd82d79b8b5e1ffe2ad0ac5cf9293c341f1bd` |
| Shipping code `d6854c44` | pass, `CI_ALL_OK`, exit 0 | `135e310b8311df75ade5d0e798733a11a55bc2d947c3672adbb6b33545efc2b6` |

The shipping-code run is distinct from the experiment-source jobs above. Later
upstream `9ed4d651` also has [green CI](https://github.com/nacre-desktop/bead/actions/runs/40231190757).
Matched capture source remains `61e73507`; no runtime change from this unit is
integrated by a later documentation rebase.

The prototype additionally ran
`cargo test -j2 -p bead-wayland pointer_motion`: two tests passed, covering
absolute clamped positioning and a real painted scene primitive rasterized to
visible BGRA pixels with transparent corners. The kit/runtime integration test
checks scene-layer identity and preview-to-travel geometry. Final release builds
use `cargo build -j2 --release -p nacre-shell`.

No runtime dependency was added. Private probe builds use the normal Qt 6
base/declarative private headers, Wayland/wlr protocols and C toolchain. Python
Pillow is used for capture inspection. There is no installer or deployment run.

The retained patch SHA-256 is
`e68ae418969c8ab60c9f6b41d6d3c375806dbca6b479803cfd791a9baba57355`.
Blank context markers are normalized without changing the applied source;
`git apply --check` passes against `61e73507`.

The retained patch is a rejected experiment based on `61e73507`, not merged
runtime code. Its CPU variant precedes the opt-in dynamic-panel GL diagnostic
pin, which is inactive in CPU arms. Raw private logs, source snapshots, tools and
captures are available to the lead in the task's ignored work directory; public
summaries and hashes are in `results.json`; raw timing/position joins and
per-arm memory snapshots are in `trace-samples.jsonl`. Task-owned remote copies are
removed before handoff; no temporary product replacement is installed.
