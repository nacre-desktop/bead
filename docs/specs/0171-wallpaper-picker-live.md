# B0171 — Wallpaper picker live: lost selections, slow switches, cold open

Owner 2026-10-28: the picker is laggy, sometimes does not open, and is buggy on the laptop.
Reproduced on the desktop test host in a private nested session (niri 2880x1800 at 1.5, the owner's 17
`Static` wallpapers incl. the 7680x4320 PNG, a current-main `nacre-helper` daemon built from 9b87d913) with
`tools/daily-acceptance/run.sh --wallpaper-live` (open picker, five rapid Down+Return selections, close,
reopen; three cycles).

## Causes
1. **Lost selections (N0089, the main bug).** The daemon's `wallpaper.activate` took the global palette-publication
   `flock` on its single event loop. The `wallpaper-media select` child it spawned for the previous click holds that
   lock while it publishes, so every click after the first parked the daemon in `flock_lock_inode_wait`
   (`ps` wchan in the repro): no `daemon_ack`, no new revision, picker "does nothing" (live: the 3840x2160 click).
   Fix in Nacre `active_wallpaper.rs`: write the generation file with the atomic rename only, no lock.
2. **Cold first selection of any file (Bead).** The display-size cache (B0146) only held files already shown once, so the
   first selection decoded the full image (430 ms for the 8K PNG), and the selection predecode and the synchronous
   prepare decoded it twice in parallel (second `decode_start` in the live log).
   Fixes: (a) `wallpaper_prefetch.rs` decodes every catalogue still to display size while the picker is open
   (nearest to the active one first, one at a time, nice 10, cancelled on close); (b) `wallpaper_cache` is single-flight per
   entry, so concurrent callers share one decode; (c) disk cap 144 -> 512 MiB so the whole set stays cached (17 files need
   about 120-170 MiB; LRU churn re-created misses).
3. Diagnosis aid: `WALLPAPER_ACTIVATE_ERROR revision= code=` is journaled when an activation fails (it was silent).

## Numbers (desktop test host, nested 2880x1800@1.5, 3 cycles x 5 rapid selections)
| Arm | Selection to presented p50 / p95 / max | Selections acknowledged |
| --- | ---: | ---: |
| before (old daemon, old Bead) | 440 ms first; later clicks never acknowledged | 2-3 of 10 per cycle |
| N0089 only (new daemon, old Bead) | 190 / 273 / 307 ms | all |
| both (new daemon, prefetch + single flight) | 61 / 89 / 206 ms (206 = first click 1.5 s after a cold open, prefetch unfinished) | all |

Warm reopen: 17-24 ms to first content frame (unchanged). Memory trade: prefetch holds one decoded raster (about 20 MiB,
plus its zstd copy) transiently in the shell while it works; steady state unchanged. Disk cache up to 512 MiB under
`~/.cache/bead/wallpaper-display`.

## Open
- **Cold first open is 2.6 s** (live: about 4 s) on the first picker open after the shell starts; warm opens are 17-24 ms.
  During those 2.6 s two `wallpaper-media catalog --catalogue-ids` children run at 100 % CPU and the shell main thread at
  60-120 %; the panel worker's first commit waits on that. Not fixed here (open path belongs to B0170 / panel code, and the
  catalogue hydration is daemon side). Next step: stop hydrating all ids at open, request only the visible ones first.
- 120 ms p95 is met for cached files on this host; a click inside the first ~1.5 s after a cold open can still take ~200 ms.
- `WALLPAPER_BUFFER_RETAINED reason=no_release` is niri keeping the buffer (no `wl_buffer.release`); it is one buffer per output
  and did not grow in the repro.
- Duplicate `WALLPAPER_SURFACE_COMMITTED` for one revision (a repaint after the predecode finishes) is harmless and left.

## Tests
`wallpaper_cache::concurrent_requests_for_one_entry_decode_it_once`, `wallpaper::rapid_selections_end_on_the_last_click_and_older_generations_are_dead`,
`wallpaper::raster_pair_keeps_the_cache_key_inputs_of_the_selected_file_only`, `wallpaper_prefetch::prefetch_order_starts_beside_the_active_wallpaper`.
Nacre side: see N0089.
