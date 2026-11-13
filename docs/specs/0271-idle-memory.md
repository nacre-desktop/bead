# 0271 — Idle memory ownership

The memory half of the resource goal in [0245](0245-interaction-cpu.md): what a Bead process keeps alive at idle, who owns it, and which parts a host gets back for free. Method first (the [0232](0232-graph-canvas.md) rule: measure ownership before optimising), then the fixes. Status: measurement phase done on the CPU path; fixes proposed, none landed yet.

## Method

Release build (`profiling` profile: release codegen plus symbols), headless sway 1.9 with the pixman renderer in the Ubuntu 24.04 CI container (6 CPUs, 8 font files), CPU tree renderer. Scenarios: (a) `bead run examples/hello-bar`; (b) `examples/bar-with-panels` after opening and closing calendar, volume and status once through a private virtual pointer; (c) hello-bar on two headless outputs. Each run settles 60 s, then `smaps_rollup` and per-mapping `smaps` are read (PSS = Pss_Anon + Pss_File + Pss_Shmem + SwapPss; unreclaimable = anon + shmem + swap). Five alternating rounds, medians. Ownership: heaptrack on the same binary, the process killed with SIGKILL at the idle point so heaptrack's "leaked" set is exactly the live heap at idle, aggregated by call site. The harness and raw snapshots stay untracked in `.lead/mem/`. GL was not measured: the owner's GPU host went offline during the unit and the CLI builds `bead` without `native-gl`; a llvmpipe run is queued (see the unit handoff).

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
| glibc arenas: 8 to 11 after three panel workers exited; arena RSS 2.2 to 4.7 MiB; main-thread brk 0.1 to 1.9 MiB | ~2.4 MiB after the keep-alive share | undetermined until the 150 s heaptrack lands: live data that outlives the worker, or allocator retention that the two `malloc_trim(0)` calls on close do not return | smaps categories |
| accesskit a11y adapter (`accesskit_unix` default `async-io` executor): threads `async-io` and `blocking-1`, reactor, zbus session connection | ~90 KiB heap, 2 arenas, 2 stacks, one 500 ms wakeup each | leaked by design: created with no assistive technology present, for the process's life | thread list, heaptrack |
| SHM pools, companion renderer, GL transport of a closed panel | 0 | released on close; memfd stays at 3 bar buffers (0.79 MiB, 0.13 PSS) | smaps shmem |
| Font database (one `FontSystem` per worker `TreeCache`) | negligible with 8 font files; scales with installed fonts for hosts that leave `scene.font_files` empty | kept | heaptrack |

Fixes proposed, ranked by bytes saved per risk (none landed; see the unit handoff for the verification step each needs):

1. `accesskit_unix` with `default-features = false, features = ["tokio"]` in `bead-wayland`: drops the async-io reactor and blocking-pool threads and their 500 ms wakeups (`.lead/candidate-accesskit-tokio.patch`; needs a build to confirm the thread count 7 to 5 and clippy/tests).
2. Arena residue after a panel worker exits: decide between `mallopt(M_ARENA_MAX, n)` at startup and a trim after the keep-alive release, once the 150 s heaptrack shows how much is live.
3. The markup VM of a dormant (parked or keep-alive) panel: do not release; reopen latency (B0270) and warm panels (N0231) spend that memory on purpose.

What Nacre gets at the next pin bump: fix 1 (threads and wakeups in every Bead host). Host-side: the 120 s keep-alive and the warm budget are Nacre's settings; the per-Source Luau cost is paid once per open or parked panel.
