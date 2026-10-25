# B0126 validation record

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
