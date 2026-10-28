# B0173 — Wallpaper picker cold open

Status: implementation candidate; cold-open acceptance and full CI are pending the shared heavy slot.

## Problem and baseline

B0171 recorded a 2.6 s first open in a nested 2880×1800, scale-1.5 session and about 4 s live; warm reopen was 17–24 ms. The opening path waited for full catalogue metadata and the worker used `wallpaper.catalogue {ids:[]}` to enumerate IDs. A separate wallpaper warmer issued the same full-catalogue request during shell startup. The first frame therefore waited on slow child processes, while the picker worker and warmer competed with the shell.

The B0171 selection numbers remain the only measured switch baseline: with prefetch and single-flight decode, selection-to-presented was 61 ms p50, 89 ms p95, and 206 ms max across three cycles of five selections; all selections were acknowledged. The 206 ms outlier was the first click 1.5 s after cold open, before prefetch completed. These figures do not measure the B0173 candidate.

## Implementation

- Wallpaper content no longer gates the panel commit. The loading state is renderable immediately, so the panel can slide while catalogue and image work runs.
- The shell keeps a bounded, versioned JSON metadata snapshot at `$XDG_CACHE_HOME/bead/wallpaper-catalogue-v1.json` (or the standard cache fallback). Writes use a same-directory temporary file and atomic rename; reads reject oversized, malformed, wrong-version, or invalid-entry files. A stale snapshot is a fast first-frame fallback while the daemon refreshes it.
- A full refresh merges previously known thumbnail, preview, and palette paths for matching IDs so a metadata-only reply cannot blank thumbnails already on disk. Successful full and visible-ID replies refresh the snapshot.
- `wallpaper_warm` consumes the shell's saved snapshot instead of issuing a second full-catalogue query. With no snapshot it waits for the UI adapter's refresh to populate one. Visible IDs continue to hydrate from the current scene; other entries are deferred.
- The B0171 display-raster prefetch remains one-decode-at-a-time at nice 10, is cancelled on close, and waits 100 ms after scheduling to allow the populated picker frame to be committed first. Persistent display rasters remain under the existing 512 MiB cache cap.

Generic addition: a bounded persistent wallpaper-catalogue metadata snapshot; no engine primitive or Nacre-specific engine dependency was added.

## Evidence and acceptance

| Check | Result |
| --- | --- |
| Cold open with normal-day cache, first presented content <150 ms | Pending desktop TEST capture |
| Fully cold cache, first presented content <300 ms | Pending desktop TEST capture |
| Selection-to-presented p95 <120 ms | Pending `--wallpaper-live` acceptance |
| Shell CPU returns to 0% within 2 s after close | Pending desktop TEST capture |
| QML/Bead matched idle, open, hover, pressed, and sub-page states | Pending paired capture |
| Full Bead CI and Nacre desktop CI | Pending; heavy slot withheld |

Capture environment required by the task: fresh shell and daemon on desktop TEST, laptop mode, 2880×1800 at scale 1.5, owner's 17 static wallpapers including the 7680×4320 PNG. Record the warm-cache arm after a normal day separately from the fully cold-cache arm. Include both CPU and GL renderer designation with memory numbers. No candidate latency or memory values are claimed yet.

The snapshot may contain local wallpaper identifiers and thumbnail paths, so it is stored only in the user's cache directory and is not committed. The daemon remains the source of truth; every open refreshes the snapshot asynchronously. No temporary duplicate implementation or rollback file was introduced, so no duplicate expiry applies.

## Files

`crates/nacre-shell/src/extras.rs`, `wallpaper_catalogue_cache.rs`, `wallpaper_warm.rs`, and `wallpaper_prefetch.rs`; behavior stays in the existing Luau view. No Nacre files changed.
