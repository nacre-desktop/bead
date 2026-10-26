# B0131 evidence

[Build identities](build-identity.json) distinguish source manifests and binaries.
The release build prewarms a separate native target before release, sharing the
renderer and GL context. CPU prewarm imports the existing SHM alias while a
verified transparent corner hides it. GL paints an unswapped backbuffer at the
complete natural extent. Release commits the final offset and content, then
unmaps the retained source in the same connection flush. Stale generations and
cold failures retain the source. Host Luau selects 180 ms OutCubic dock easing.
[Cold CPU traces](cold-render-diagnostic.json) attribute the old roughly 442 ms tail
to reveal-edge construction and composition, not GL context creation. The
release-profile control is much faster too; no code-only speedup factor is claimed.

[Aggregate](latest-summary.json) and [four raw cohorts](latest-release-proofs.json.gz)
keep image and timing claims separate. Each backend has 20 image trials and 20
presentation-only trials. The image cohort measures blanks, reversals, native
pose and size, with at least eight samples after handoff. It retains its timing
failure if present. The presentation cohort performs no live screencopy during
drag/release and leaves all image gates unmeasured. Passing separate cohorts is
not a simultaneous guarantee for one set of 20 trials. All retain the 33 ms
handoff p95 and 16 ms render p99 limits; no threshold was relaxed.

The C pointer sends relative held-button motion directly to nested niri for
650 ms. Exact input-release timestamps are matched to native-surface
`wp_presentation_feedback.presented`; client commit latency is separate.
`UI_FRAME` measures render callbacks during drag and release. Parent geometry
verifies 2880×1800 at synthetic 120 Hz, scale 1; nested output scale is 1.5.
Niri's Winit backend uses 60 Hz. These are compositor software timestamps,
not physical scanout, and do not demonstrate the aspirational 17 ms target.
[Three alternating screencopy/no-screencopy pairs](observer-diagnostic.json) isolate observer
cost; they are a diagnostic, not another formal percentile cohort.

[Memory summary](memory-summary.json) and [complete paired rows](memory-trials.json.gz) record five alternating release-build pairs per
backend, matched to the same-host control. Unreclaimable is anon + SHM + DRM +
SwapPss; clean-file PSS is separate. Private compositor memory is also separate.
The always-on budget is 1 MiB; active warm buffers are an explicit smoothness
trade. The matched memory arms also reduce captured handoff medians from
about 504.5 ms to 36.0 ms CPU / 33.4 ms GL; these five-sample diagnostics
are separate from the formal 20-trial percentiles. [Allocator diagnostic](allocator-residency.json) with [raw maps](allocator-residency-trials.json.gz) shows a 4.49 MiB trim
without changing live allocations. The final implementation trims after idle
repair scratch is freed, rather than before that cleanup; this does not eliminate
all active allocator residency. The measured active cost remains explicit.
The second GL control arm had a real pre-presentation blank; its complete safe
memory rows remain in the paired statistics rather than being retried or removed.
Every candidate memory arm passed its image checks. Private niri's CPU closed
median is +41.223 MiB (mostly DRM). A [post-close diagnostic](compositor-retirement-diagnostic.json)
shows the gap is frame-history-sensitive: extra captures changed a roughly
46 MiB one-pair gap to 4.56 MiB, and it reversed after private UI exit. The exact
mechanism is unresolved. The UI budget pass is not a combined desktop-memory
budget pass.
No laptop-cap claim.

[Paired contours and API outcomes](pairs/README.md) and [raw return trajectories](return-trials.json.gz)
use pinned QML and Bead
states independently of the final release cohorts. Return captures have no
blanks and are monotonic; both configure 180 ms OutCubic. Their 2 px settling
threshold is not exact animation completion. Control and AI cold API detach
commit native content and restore docked state. Different gesture input regions
and content prevent a full pixel-style parity claim. Full-output images remain
private because provider content includes a package update inventory; published
48×48 logical contour crops omit it and the top bar.

[Complete CI replica](full-ci.sh) and [receipt](ci.txt) cover the remote Arch
container. Tests use one thread to respect the existing 50 ms Luau budget;
that product budget is unchanged. A fresh root-owned 0700 temporary directory
avoids stale test fixtures and preserves temporary-directory security checks.
Ordering tests cover stale placement, configuration, failed/busy draws and source
retirement after content commit; proof tests cover measurement classification.
No live installation, desktop configuration or installed shell was changed.
