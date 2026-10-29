# B0196: widget context reads at build time

Goal: widget modules stay loaded between bar builds (B0191 reuse rule), so a bar update stops re-running them. Target was bar markup p50 < 1 ms.

## Change

- `shell/widgets/context.luau` reads no store while loading. Its store values are functions (`c.popouts()`, `c.header()`, `c.title()`, `c.title_width()`, `c.dwell()`, `c.thread()`, `c.ws_on()` and so on) that the render functions call at build time; `pearl_accent` reads the workspace count when called. Same stores, same values per build; no golden changed.
- The widget-preview sample (`widgets.preview`) is read once per build in `c.begin()`, which each entry that uses the context calls first (`bar/main`, `widgets/app/main`, `widgets/editor/main`, `widgets/drag/ghost`). `ui.bind` in the context uses it. Before, it was read at context load, so also once per build.
- Consumers read through the accessors inside render functions: all `widgets/*/widget.luau` that used `popouts`, `updates_on`, `ws_on`, `focus_on`, `thread`, `launcher`, `title`, `title_width`, `dwell`, `header`, plus `widgets/tray.luau`.
- Context reads are cached per build (`begin()` clears them), so repeated `c.popouts()` calls in the widgets gallery do one host call per store (without the cache the widgets app exceeded its 50 ms markup budget in debug tests).
- `widgets/tray.luau` built nodes while loading, so it returned the same impure module every build; it now returns a function that `bar/main` and `widgets/app/preview` call.
- One small engine change (`crates/bead-ui/src/frontend.rs`): `text_width` of a literal string no longer marks a module as reading build input (a bound text still does). A literal measurement is a constant of the generation (font and text style changes rebuild the VM). The clock, notifications and updates widgets measure constant strings at load. Test: `persistent_vm_reuses_pure_modules_and_reruns_input_readers` now has a pure module that measures a literal.

## Result

Modules re-run per build (release, real `shell/` tree, bar update, diagnostic print of the VM's dynamic set): before 7 (`app/preview`, `clock/widget`, `dragging`, `notifications/widget`, `registry`, `tray`, `updates/widget`), after 4 (`app/preview`, `clock/widget`, `dragging`, `registry`). `widgets/context.luau` and the other widget modules stay loaded. `clock/widget` stays dynamic because the preferred spans in its descriptor depend on `bar.popouts` (`reserve_clock`), and the registry, dragging and preview import it. Making that lazy means descriptor fields read at use time, which touches the registry, editing, preview, dragging and app readers; not done here.

Bar numbers (dev-2 container, release, `markup_bench`, 56 updates, thread CPU):

| | markup p50 / p95 | eval p50 | serde p50 |
|---|---|---|---|
| before (origin/main 35ac2f88) | 1.68 / 1.80 ms | 1.02 | 0.61 |
| after | 1.56 / 1.68 ms | 0.90 | 0.61 |

Target (< 1 ms) not met. The module re-runs were not the main cost. One bar build creates about 91 nodes and 265 host calls; eval (about 0.90 ms) is the render functions and constructors of those nodes, serialising the tree costs another 0.6 ms. The B0191 estimate that the context re-run was most of the 0.84 ms eval was too high. Launcher, dashboard and notification views are unchanged (0.61 ms launcher).

Real-shell smoke: the bench and the whole CI suite load the real `shell/` tree (0 `UI_ERROR`); a nested desktop run was not made in this unit.

Next steps for < 1 ms (not in this unit): fewer host calls per node (bind resolution in render functions, about 4 per node), `debug.info` per constructed node in the prelude, memoising unchanged widget subtrees with `ui.memo`, and a cheaper RawNode serialisation. Each is a separate measured unit.

## Checks

See the report for exact commands. Bench: `BEAD_PROFILE_INPUT=1 cargo test --release -p nacre-shell markup_bench -- --ignored --nocapture`. `persistent_vm_updates_match_fresh_loads_on_shell_views` (bar, launcher, dashboard, notifications equal to a fresh load) covers equal values per build.

No temporary duplicates introduced.
