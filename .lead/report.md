Status: done; r2 branch pushed, pending the lead’s laptop gate.
Branch: task/codex-b0177-panel-polish-r2-20261028.
Code source passed full CI at b6800318; docs/status update is 6efe2a49.

Design: worker attaches the complete open-pose raster before publishing the prepared scene. UI pins it by motion/scale; reveal frames clear damage and composite only by offset/clip. Repeat opens use a two-entry cache. The replacement renderer test checks worker attachment, open-pixel equivalence, zero UI tree builds, and clipped/offset output.

CPU-only release benchmark (`cargo test --release -p nacre-shell --no-default-features b0177_cpu_bench --locked -j2 -- --ignored --nocapture`), dev host, 2880×1800 at scale 1.5, 240 samples after 24 warmups:
- Dashboard prepare p50/p99: baseline 110/350 ns, r2 110/480 ns; compose: 1.905/3.336 ms, r2 1.857/3.311 ms.
- Control centre prepare: baseline 40/320 ns, r2 40/330 ns; compose: 1.797/2.044 ms, r2 1.852/2.066 ms.
- Retained rasters: dashboard 6,734,640 B; control centre 3,531,360 B.
The isolated CPU results are similar; they do not claim a live frame-rate win. Debug full-damage timings were 25–26 ms.

Checks: full Bead CI recipe in Ubuntu 24.04 container, `bash /workspace/src/full-ci.sh`; marker `BEAD_CI_AND_RELEASE_ARMS_PASS`. Workspace: 778 passed, 4 ignored. Formatting, Clippy `-D warnings`, Luau, Python suites, private bus tests and cargo-deny passed. Renderer crate: 97 passed.

Open latency, presented frame gaps, QML parity and close ghost-bar frames were not measured. Per instruction, the owner’s desktop was untouched; these remain for the lead’s laptop gate.
Code diff vs main: +1,244 / -153 Rust and test lines.
