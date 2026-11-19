# 0271 — Idle memory ownership

The memory half of the resource goal in [0245](0245-interaction-cpu.md): what a Bead process keeps alive at idle, who owns it, and which parts a host gets back for free. Method first (the [0232](0232-graph-canvas.md) rule: measure ownership before optimising), then the fixes. Status: CPU path measured; three fixes landed (a11y threads, arena cap, closed-panel service wakeups). GL not measured (llvmpipe only, see below).

## Method

Release build (`profiling` profile: release codegen plus symbols), headless sway 1.9 with the pixman renderer in the Ubuntu 24.04 CI container (6 CPUs, 8 font files), CPU tree renderer. Scenarios: (a) `bead run examples/hello-bar`; (b) `examples/bar-with-panels` after opening and closing calendar, volume and status once through a private virtual pointer; (c) hello-bar on two headless outputs. Each run settles 60 s, then `smaps_rollup` and per-mapping `smaps` are read (PSS = Pss_Anon + Pss_File + Pss_Shmem + SwapPss; unreclaimable = anon + shmem + swap). Five alternating rounds, medians. Ownership: heaptrack on the same binary, the process killed with SIGKILL at the idle point so heaptrack's "leaked" set is exactly the live heap at idle, aggregated by call site. The harness and raw snapshots stay untracked in `.lead/mem/`. GL was not measured on a real driver (the GPU host was offline); the llvmpipe runs show no Bead-owned GL state retained after a close.

## Idle PSS (medians of 5, MiB)

| Scenario | PSS | anon | file (code) | shmem | threads |
|---|---:|---:|---:|---:|---:|
| hello-bar | 16.3 | 2.87 | 13.3 | 0.13 | 7 |
| hello-bar, two outputs | 17.5 | 3.92 | 13.4 | 0.27 | 8 |
| bar-with-panels, before any open | 17.9 | 3.28 | 14.5 | 0.13 | 9 |
| bar-with-panels, 60 s after three open/close cycles | 22.9 | 7.72 | 15.2 | 0.13 | 10 |

Swap was zero throughout. The file-backed share is the `bead` binary's text and rodata (13 MiB mapped, 10-11 resident); it is clean and shared but counts in PSS scoreboards.

## Ownership

| Owner | Bytes | Kept on purpose or leaked | Evidence |
|---|---:|---|---|
| Luau markup VM: kit prelude and bridge loaded by `Vm::new`, plus the build state (`LuauFrontEnd::build`) | ~1.0 MiB per Source | kept: lives for the Source's life (`Session::release_markup_vm` exists and has no caller) | heaptrack, hello-bar |
| Luau behaviour VM (`Script::load_inner`), even without a `behavior.luau` | ~0.4 MiB per Source | kept | heaptrack |
| Second output | +1.0 MiB anon | kept: one worker, Session and VM set per output ([0025](0025-per-output-ui-sessions.md)) | scenario (c) |
| Closed panel for `PANEL_KEEP_ALIVE` (120 s, `panel.rs`), regardless of `retain_when_closed` or the warm budget | ~2 MiB (worker thread, Session, VMs, CPU snapshot) | kept on purpose for a fast reopen; released at 120 s, then parked under the host's warm budget | 60 s snapshots identical with `retain_when_closed=false` and with a 64 MiB warm budget |
| glibc arenas: 8 to 11 after three panel workers exited; main-thread brk 0.1 to 1.9 MiB | ~2.6 MiB anon at 150 s (6.0 vs 3.35 before the first open) | allocator retention, not live data: heaptrack at 150 s shows 0.75 MiB live in the whole process; the two `malloc_trim(0)` calls on close do not return the arenas of exited workers | heaptrack 150 s, smaps |
| accesskit a11y adapter (`accesskit_unix` default `async-io` executor): threads `async-io` and `blocking-1`, reactor, zbus session connection | ~90 KiB heap, 2 arenas, 2 stacks, one 500 ms wakeup each | leaked by design: created with no assistive technology present, for the process's life | thread list, heaptrack |
| SHM pools, companion renderer, GL transport of a closed panel | 0 | released on close (a retained GL panel keeps its renderer and the transport for a 300 s grace, [0270](0270-panel-first-frame.md#retained-gl-open-b0334)); memfd stays at 3 bar buffers (0.79 MiB, 0.13 PSS) | smaps shmem |
| Font database (one `FontSystem` per worker `TreeCache`) | negligible with 8 font files; scales with installed fonts for hosts that leave `scene.font_files` empty | kept | heaptrack |
| Implicit system font database (B0350) | one `FontSystem` per process and locale for every scene with empty `font_files`, reloaded only when the mtimes of the font directories (XDG data, `~/.fonts`, `XDG_DATA_DIRS`, `/usr/share/fonts`, `/usr/local/share/fonts`) and their immediate subdirectories change; fontdb maps font files lazily, so retained is face metadata (about `size_of::<FaceInfo>()` plus path and family strings per face, 8 faces in the CI image, about 0.5 KiB per face on a desktop) plus the pages of fonts actually shaped, which are file-backed and reclaimable; the 90 s wallpaper re-warm no longer rescans the system | kept for the process (not torn down with a renderer) | `system_fonts_are_cached_until_the_stamp_changes` |
| `scene.font_files` contract (B0344) | non-empty means exactly those files and no system fonts; a missing or unreadable file is skipped with one `FONT_FILE_SKIPPED file=<name> reason=missing\|unreadable` line per path, and none loading falls back to the system fonts | kept | `missing_font_files_are_skipped_not_fatal` |

## Fixes landed

Medians of 3 alternating rounds, CPU path, same container, 60 s idle (`ab-before` = origin/main build, `ab` = this branch). Anon is the unreclaimable share; PSS file moves with the binary and is not a fix signal.

| Fix | hello-bar anon / threads | two outputs | bar-with-panels after 3 open/close | evidence |
|---|---|---|---|---|
| 1. `accesskit_unix` on its `tokio` feature (`bead-wayland/Cargo.toml`, adds `tokio-stream`): no `async-io` and `blocking-1` threads | 2.87 to 2.64 MiB, 7 to 5 | 3.91 to 3.65, 8 to 6 | 7.67 to 6.55 (with 2) | `blocking-1` had 133 voluntary context switches in 60 s (the 500 ms timers), now gone |
| 2. Arena cap, NOT applied by Bead (lead decision: a framework must not change its host's process-wide allocator; a host may set `MALLOC_ARENA_MAX=2` in its launcher and must measure the CPU effect) | arenas 6 to 1 | | idle150 `retain_when_closed=false`: 6.0 to 5.0 and 5.5 (arena=2), 5.3 and 4.9 (arena=1), baseline 3 runs at 6.0 | variants2 and arena runs; heaptrack shows the 1 MiB is freed pages in per-thread arenas |
| 3. Closed panel stops service wakeups (`FolderView` unregisters on `PanelVisibility(false)`, registers again on `true`, which delivers one fresh model hint) | no memory change | | | test `closed_panel_gets_no_service_wakeups_and_resubscribes_on_reopen` |

Fix 3 is the Bead half of the closed-panel keep-alive: the host pauses by reacting to the existing `UiEvent::PanelVisibility` that every closed, kept-alive source already receives (no new trait method). A host that owns its own subscriptions (Nacre's adapter: store subscriptions, daemon leases such as the `dashboard.system` process listing, the seconds clock) must do the same in its `SceneSource::stage` on `PanelVisibility(false)` and re-subscribe with a fresh snapshot on `true`. The folder host's clock lease is process-wide (shared with the bar), so it stays. Not done: releasing a closed panel's decoded images before the 120 s release; with `retain_when_closed` the CPU snapshot already frees them at 1.5 s, but without it they live until release (UI-side `release_prepared` of the companion scene plus the worker cache, needs a reopen-latency measurement on the image-heavy Nacre dashboard).

Not changed: markup VMs of kept-alive or parked panels (warm panels spend that memory on purpose, B0270/N0231); the 120 s `PANEL_KEEP_ALIVE`. Nacre gets fixes 1 and 2 at its next pin bump; fix 3 needs the adapter change above.

## Budget overruns degrade (B0318)

A render that exceeds a retained-cache budget must not end the host. When admission refuses a CPU raster and nothing is left to evict, `TreeCache::prepare` keeps it for the frame only (uncharged, dropped from the cache once the frame holds it, at most 24 MiB per prepare), else replaces it with a blank 1x1 (`skipped`). A Polyline, Arc or Rounded shape whose repair scratch would pass the 16 MiB cap paints straight into the masked strip (`direct`). Each degrade prints one marker per owner every 5 s: `RENDER_BUDGET owner=<budget> need=<bytes> cap=<bytes> action=<uncached|skipped|direct>`.
A refused reservation is routine and silent (it only records owner, need, cap, used). Where a budget error surfaces (the fatal draw sink and `UI_ERROR CPU snapshot`) `cache::explain_refusal` appends ` owner= need= cap= used=` to the printed text, no env var.
GL textures charge their renderer, not the view that baked the pixels (B0338, approved by the coordinator 2026-11-18): each `GlRenderer` owns one `native-gl-images` budget of 64 MiB for its image-payload and layer-raster textures, kept B0334 textures included, and the opacity-group composite keeps the renderer's `native-gl-composite` (64 MiB), while a view's `retained-view-resources` (40 MiB: worker rasters, upload pixels, nodes, frames) counts no texture any more. With Strand open this stays inside the owner's 160-200 MB envelope and below the old Dense 136 MiB `retained-cpu-rasters` cap. A refusal evicts textures the frame does not use (least recently drawn first); with none left a GL companion takes the gpu-to-cpu fallback and prints `RENDER_BUDGET owner=native-gl-images need= cap= action=cpu`. `PANEL_GPU_SUSPENDED` reports `gl_texture_peak=<bytes>`, the most the renderer held since that panel's open.
