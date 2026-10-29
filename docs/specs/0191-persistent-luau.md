# B0191: persistent Luau VM per view generation

Goal: a UI update stops paying for a fresh Luau VM, prelude, kit and module tree; only what reads the update's stores re-runs. Behaviour, sandbox and budgets stay as they were.

## Cause

Every view update (`Generation::materialize` → `LuauFrontEnd::build`) created a new `Lua`, sandboxed it, loaded and ran the `bead.ui` installer, `bead.host` and `bead.kit` (tokens JSON parsed each time), then `luau_load`ed and ran every required module and the entry from bytecode. Bytecode was already compiled once per generation; the VM, prelude, kit and module evaluation were not.

Breakdown, release, real `shell/` tree, `markup_bench` (below), p50 per update, thread CPU:

| View (update) | setup (VM, prelude, kit) | eval (modules + entry) | serde (Lua → RawNode) | markup |
|---|---|---|---|---|
| bar (workspace + title) | 0.35 ms | 1.16 ms | 0.50 ms | 2.13 ms |
| launcher (keystroke, new result page) | 0.33 | 0.27 | 0.24 | 0.90 |
| dashboard (data refresh) | 0.31 | 0.14 | 0.17 | 0.68 |
| notifications (new rows) | 0.31 | 0.13 | 0.09 | 0.58 |

Setup was a fixed ~0.33 ms per update in every view; for the bar, re-reading module bytecode (`luau_load` of about 20 widget modules that read stores while loading) was another ~0.3 ms.

## Change (`crates/bead-ui/src/frontend.rs`)

- One `Vm` per `LuauFrontEnd`, so per generation: the VM, sandbox, interrupt, memory limit, `bead.ui` installer, `bead.host` and the kit are created once. A source edit makes a new generation and so a new VM.
- Loaded chunks are kept as functions. Re-running a chunk gives fresh module locals without re-reading bytecode, so re-runs behave like the old reload.
- A module whose load calls no host callback and builds no node stays loaded for the generation. A module that reads build input while loading (`resolve_for_check`, `text_width`, `model_count`, `memo`, `Repeat`, `preview_value`, `animate`, or a constructor), or that imports such a module, re-runs in every build as before. The entry re-runs in every build.
- Host callbacks that borrow the build (resolve, read sets, memo fragments, width dependencies) are still `lua.scope` functions, created per build into a private `host` table. Kit, modules and `require` call them through small stable Luau bridge functions. `source()` reads stack level 2, the caller of the bridge.
- Isolation kept: a failed build (error, 50 ms CPU budget, memory) drops the VM, so the next build starts fresh. A script that writes a global also drops it after the build, so global state never carries over. A catalog or text-style change (new `i18n::epoch`) rebuilds the VM, because the kit tokens and module-level `ui.tr` text are read when they load. The 8 MiB memory limit, the thread-CPU 50 ms budget (B0186), the non-sticky overrun notice and the memo/materialized caches are unchanged.
- Contract (new): state that a module keeps in its own tables persists across the generation's builds, as in any module system. The shell does not depend on per-build fresh module tables; the equivalence test below checks the real views.

## Numbers (release, prod build host, same bench before/after, 55 updates per view)

| View | markup p50 / p95 before | after | setup after | eval after |
|---|---|---|---|---|
| bar | 2.13 / 2.16 ms | 1.37 / 1.51 ms | 0.007 | 0.84 |
| launcher (per keystroke) | 0.90 / 0.92 | 0.50 / 0.54 | 0.006 | 0.25 |
| dashboard | 0.68 / 0.69 | 0.32 / 0.35 | 0.005 | 0.13 |
| notification list | 0.58 / 0.67 | 0.20 / 0.30 | 0.005 | 0.11 |

The first build of a generation still pays setup (max column ~0.35 ms higher). The bar stays above 1 ms: `widgets/context.luau` reads about 15 stores while loading, so it and every widget module importing it re-run on each bar update (eval 0.84 ms), and serialising the full bar tree costs 0.50 ms. Making that cheaper means restructuring the shell's widget context (reads inside render functions), which is a shell change outside this unit.

These are build-host numbers; B0187's ~6 ms nested `markup` was measured on the desktop test host with the live launcher model. Keystroke-to-results on the laptop is for the lead's live audit.

Memory: the VM now stays resident while its view is in use instead of being freed after each build. Measured `markup_vm_build_bytes` after 60 updates: bar 2.62 MiB, launcher 1.46, dashboard 1.49, notifications 1.57 (live heap after a full collect: 1.59 / 1.14 / 1.12 / 1.11 MiB; the rest is garbage the incremental GC reuses). The old fresh VM reached the same size during every build, and on the always-visible bar those freed pages stayed in the UI worker's arena between builds (it is only trimmed when idle). When a panel closes, the UI worker's `set_idle(true)` now calls `Session::release_markup_vm` (via `Runtime::set_idle`) before its idle trim, so closed views keep no VM and reopening pays one setup (~0.35 ms). The bar keeps its VM while visible. Unreclaimable deltas are for the lead's laptop audit (new: `markup_vm_build_bytes` is a retained VM size, not a transient one).

Real shell, nested on the desktop test host (`animation_audit.py`, bead arm, 2880x1800 scale 1.5, 120 Hz, CPU): all 13 audited surfaces (dashboard, control centre, sidebar, launcher, five popouts, power menu, notification, wallpaper carousel, OSD) opened and closed; `shell.log` has 0 `UI_ERROR`, 0 panics, 0 diagnostics; pacing 26 windows, 0 errors. Notification close and wallpaper-carousel open reported "no frames" in this one-repeat run (no baseline run to compare).

## Checks

- `cargo test -p bead-ui` (149 pass), including `persistent_vm_reuses_pure_modules_and_reruns_input_readers` (pure module kept, input-reading importers re-run, epoch rebuild) and `global_writes_and_failed_builds_keep_per_build_isolation`.
- `nacre-shell` `markup_bench::persistent_vm_updates_match_fresh_loads_on_shell_views`: bar, launcher, dashboard and notifications on the real `shell/` tree, 6 sequential updates each, drawn texts/images/kinds equal a fresh load of the same stores (the animated bar title is excluded).
- Bench: `BEAD_PROFILE_INPUT=1 cargo test --release -p nacre-shell markup_bench -- --ignored --nocapture`, then sum `INPUT_PROFILE stage=markup*` per `MARKUP_BENCH` step.
- `release_vm` drops the VM (test above, step 6) and the next build starts fresh.
- Full Bead CI (ubuntu:24.04 recipe, prod): pass. One debug-build behaviour-script timeout (`extras::tests::long_shortcut_lists_materialize_only_the_viewport`, `behavior.luau` 50 ms limit, not markup) failed once under parallel load and passed on rerun.
- Reload semantics are unchanged by construction (a new generation gets a new VM) and covered by the existing reload/recovery tests in the full CI run.

No temporary duplicates introduced.
