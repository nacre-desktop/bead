# B0183: SVG and presented-frame costs

This change caches rasterized SVG crops and avoids rewriting an unchanged recovery bundle. The SVG cache is process-wide and evicts least-recently-used entries at 8 MiB of pixel storage. Cache identity hashes the SVG bytes (including embedded color/tint) and output pixel dimensions. Frame bounds now enter the renderer key as four f32 bit patterns, with no temporary JSON buffer.

The `presented` call chain is `bead-wayland::worker::Job::Presented` → `nacre-shell::reload::Runtime::presented` → `bead-ui::Session::presented` → `Bundle::save`. The profile shows `serde_core::ser::Serialize<Vec<u8>>` under this path. Presentation revisions can advance while recovery content is unchanged; a content fingerprint now gates saving. Bundles are still saved after content changes.

No temporary duplicate paths or dependencies were added. General framework primitive: bounded SVG raster LRU cache in `bead-render`.

| Check | Result |
|---|---|
| Raster reuse | Pass — `svg_raster_cache_skips_second_resvg_render` confirms the second identical request does not call resvg |
| Bundle serialization | Pass — `steady_presentations_do_not_serialize_recovery_bundle_again`; fingerprint is computed once when capturing a candidate bundle |
| 120 Hz / close proofs | Pass — scheduler callback test and Wayland close tests |
| Remote CI | Pass — workspace fmt, all-target Clippy, full workspace tests, feature checks, Luau analyzer, Python proof/packaging suites, wallpaper smoke, private bus tests, and cargo-deny |
| Release CPU benchmark | Pending — needs matched before/after dashboard slide harness |
| Real-shell 12-route smoke | Pending lead live audit |
