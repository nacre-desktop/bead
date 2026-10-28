# B0178: panel slide frame cost

The laptop profile attributes substantial time to libc copies and allocations, buffer repair, and repainting. Existing animated surfaces already use three reusable SHM buffers and per-buffer damage history. This change removes an allocation and sort from stale-buffer adoption; it does not change which pixels are repaired or rendered.

| Check | Result |
|---|---|
| SHM pool | Existing three-slot reusable pool retained |
| Adoption copy setup | Per-call `Vec` allocation and sort removed |
| Damage behavior | Unsorted and overlapping repair rectangles covered by a unit test |
| Full slide tick benchmark | Not yet measured |
| Laptop slide acceptance | Lead-owned live audit pending |

The profiling profile inherits release optimization and keeps debuginfo. Build with `RUSTFLAGS="-C force-frame-pointers=yes" cargo build -p nacre-shell --profile profiling --locked -j2`.

Remaining work before B0178 acceptance: full 2880×1800 scale-1.5 tick benchmarks for control centre, sound, and dashboard; prepared panel/contour rasterization; removal of model work from animation ticks; idle warmup; first-open and close artifact checks. Live laptop results remain the lead's gate.

General renderer primitive: none added. No temporary duplicate implementation introduced.
