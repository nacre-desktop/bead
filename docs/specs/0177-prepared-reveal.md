# B0177 Prepared panel reveals (2026-10-28)

Status: r2 candidate. Full Bead CI passed on source `b6800318` (778 passed, 4 ignored). The resource worker now bakes the complete open-pose picture before publishing the prepared scene; repeated frames only clear damage and composite that picture at the animated offset and clip. The artifact stays in a two-entry shared cache for warm reopens. Release CPU timings are below; the owner’s private desktop visual gate remains pending, so no live opening-latency or close-frame claim is made.

| Scene, CPU renderer at 2880×1800 / scale 1.5 | Baseline 4756b652 prepare p50 / p99 | r2 prepare p50 / p99 | Baseline compose p50 / p99 | r2 compose p50 / p99 | r2 artifact |
| --- | ---: | ---: | ---: | ---: | ---: |
| Dashboard Today | 110 ns / 350 ns | 110 ns / 480 ns | 1.905 / 3.336 ms | 1.857 / 3.311 ms | 6,734,640 B |
| Control centre Home | 40 ns / 320 ns | 40 ns / 330 ns | 1.797 / 2.044 ms | 1.852 / 2.066 ms | 3,531,360 B |

Numbers are p50 and p99 from 240 timed frames after 24 warmups using `cargo test --release -p nacre-shell --no-default-features b0177_cpu_bench --locked -j2 -- --ignored --nocapture`, CPU-only `TreeCpuRenderer`, full 2880×1800 damage, and identical shell fixtures overlaid on 4756b652 and r2 (the baseline harness omits the candidate-only artifact attach). The table reports measured UI preparation and rendering separately; it does not include worker bake time or measure presentation gaps. Render p95s were 3.162 / 3.175 ms (dashboard baseline / r2) and 2.009 / 2.013 ms (control centre). The isolated compositor results are similar, so this benchmark does not claim a CPU win over the baseline. The debug-profile full-damage runs were about 25–26 ms and are not representative of the release path.

| Owner target | Evidence |
| --- | --- |
| Complete open picture before slide frame 1; no per-pose rebuild | Worker attaches the artifact before publishing `scene.prepared`; `prepare_on_ui` pins it by motion and scale. Renderer test checks open-pose pixel equivalence, zero tree builds during a reveal, offset/clip output and clearing. |
| Repeat-open warmth | Two-entry LRU keyed by prepared-frame identity and scale. Retained raster bytes are measured above. |
| ≤12 ms every slide gap; no increase in open latency | Not measured on a compositor. The requested private desktop was occupied by the owner; the lead will gate these results on the laptop. |
| No ghost bars on close (control centre, launcher, wallpaper picker) | Close-frame captures not run; the desktop was left untouched. |

QML/Bead motion parity also remains unverified. No temporary duplicate implementation is retained. The reusable reveal raster is a renderer primitive; panel layout and behavior stay in shell Luau.
