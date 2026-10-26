# B0148 — Recoverable cache admission

Picker cache pressure must not stop its open panel from rendering. The existing
CPU snapshot path can fail admission while retiring GL resources. Its callback
reported a UI error, leaving the picker visibly corrupted.
This unit preserves strict byte accounting and retries presentation through the
existing renderer when an optional snapshot cannot be retained.

| Resource | Bound / estimate |
| --- | ---: |
| Picker retained owner | 40 MiB (B0134/B0141 ceiling) |
| Display-size card at 1.5 scale (460×380 logical) | 1.50 MiB RGBA |
| 17 static wallpapers + 4 adjacent entries | 31.5 MiB raster estimate |
| 24-entry 4K-source regression fixture | 36 MiB raster estimate |

Admission pressure is a recoverable `CACHE_PRESSURE` marker, rate limited to one
per five seconds. Same-owner LRU eviction remains the first response; if live
charges still prevent admission, the picker redraws on its existing renderer.
CPU snapshot admission happens after GPU release, so a failed snapshot leaves
the GL renderer available to rebuild resources and draw the open panel.

Validation and real-wallpaper evidence will be recorded below after remote CI and
the nested desktop run. No personal wallpaper paths or image assets are committed.
