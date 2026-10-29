# B0187: launcher typing and list scroll cost

Goal: a keypress in the launcher shows the updated field within one frame and the updated results within two, and list scrolling commits inside the frame budget at 120 Hz.

## Measurement

`tools/daily-acceptance/run.sh --launcher-bench` (real shell tree, real `nacre-helper` daemon, 114 staged `.desktop` entries copied from the laptop, nested labwc/niri 2880x1800 scale 1.5, release build, CPU renderer). It types `terminal` three times (backspacing between rounds) with `wtype`, then wheels (20 notches) and flings (120 finger pixel events, 8 ms apart) over the result list through a virtual pointer (`pointer.c` gained `wheel`/`fling`). Numbers come from `BEAD_PROFILE_INPUT=1` and `BEAD_FRAME_TRACE=1` markers parsed by `launcher_bench.py`: `INPUT_RECEIVED` to the first `UI_PRESENTED` (field), to the `UI_PRESENTED` of the commit after `MENU_RESULT` (results), and per `UI_FRAME` the draw time plus the UI worker (`worker`) build time. `INPUT_RECEIVED` is now logged before the panel-slot key dispatch so panel keys are counted.

The nested compositor ran at 60 Hz: frames are delivered every 16.7 ms there, so the present time of a keystroke includes up to one frame of compositor wait; frame cost is reported as main-thread time, not presentation spacing.

## Cause

`resolve_icon_path` cached only successful lookups. Every icon name that does not resolve in the GTK theme, Papirus, hicolor or Adwaita (several per result page) re-ran the full theme scan, about 12.5 ms each, on every scene build. The `assets` stage therefore cost 57 ms on the UI worker for each keystroke (new result page) and 5-90 ms for each scroll step (new rows). Daemon round trip is not involved: `menu.query` returns in 2-3 ms.

## Change

Unresolvable icon names are remembered for 60 s (4096-entry bound) and answered with the fallback icon without rescanning; a newly installed icon appears within the TTL. Test: `icon_miss_tests`.

## Numbers (nested, release, 3 rounds x 8 keys; scroll over a 64-row result list)

| Metric | Before | After |
|---|---|---|
| Keypress to field presented, p50 / p95 | 60.9 / 62.7 ms | 3.1 / 4.8 ms |
| Keypress to results presented, p50 / p95 / max | 61.1 / 73.6 / 74.0 ms | 12.9 / 20.6 / 27.8 ms |
| Wheel frame cost (draw + worker), p50 / p95 / max | 9.8 / 93.9 / 102.6 ms | 9.6 / 20.6 / 28.3 ms |
| Fling frame cost, p50 / p95 / max | 47.3 / 115.6 / 115.6 ms (41 ms apart) | 7.3 / 10.5 / 16.2 ms (8.1 ms apart) |

## Remaining

- Per-update frame cost is still about 8-10 ms at p50 (`worker`). Of that about 6 ms is `markup` (a fresh Luau VM, kit load and whole-entry evaluation for every view update; 0.6 setup, 4.8 eval, 0.4 serialise). This is the engine-wide cost B0186 also targets; fixing it needs a persistent VM or incremental evaluation and is out of this unit's scope. The 4 ms/frame goal is not met until then.
- The first key after the grid (grid to list transition) costs 4-37 ms once (list creation).
- Not measured: grid, notification list, Settings pages and Widgets app scrolling (same `markup` and `assets` stages; the icon fix applies to all). The grid wheel in the bench produced no frames and needs a pointer target check.
- Panel slide rendering (B0185) and `widgets/registry.luau` (B0186) untouched.

No temporary duplicates introduced.
