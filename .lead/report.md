status: implementation complete; lead acceptance pending (real-shell smoke and release benchmark)
commit: f751c008 `[no-publish]`; pushed task/codex-b0183-svg-cache-20261029
identity/trailer: siverteh <92440237+Siverteh@users.noreply.github.com>; Codex Luna co-author
changes: 8 MiB SVG raster LRU keyed by SVG bytes hash + pixel dimensions; unchanged recovery bundles skip serialization; bounds hash directly without serde JSON
proofs: `cargo test -p bead-render --locked -j2 svg_raster_cache_skips_second_resvg_render` passed
proofs: `cargo test -p bead-ui --locked -j2 steady_presentations_do_not_serialize_recovery_bundle_again` passed
proofs: `cargo test -p bead-wayland --locked -j2 companion_slide_callbacks_at_120_hz_keep_committing_without_staging` passed; `cargo test -p bead-wayland --locked -j2 close` passed (5)
remote CI: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -j2 -- -D warnings`; workspace tests passed: 784 passed, 2 ignored
remote CI: trace memory tests (4 passed); no-default/all-features checks, minimal facade and shell checks passed
remote CI: Luau analyzer (175 modules), 12 capture/proof suites, py_compile, packaging, wallpaper smoke, private service tests and cargo-deny passed
profile: `perf report -i .lead/tmp/prof2.data --no-inline --children --comm nacre-shell --sort sym --stdio` confirmed sliced_art/resvg (~15.7%) and presented/Vec serialization (~12.9%) paths
pending: matched release dashboard-frame CPU benchmark; real-shell open/close 12-route smoke with zero UI_ERROR (lead-owned live gate)
size: +94 / -9 product lines; +33 test lines; no dependencies
