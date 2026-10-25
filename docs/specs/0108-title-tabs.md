# B0108 — Bar title and dashboard tabs (lead-accepted 2026-10-25)

The title group used a byte-count width estimate, leaving empty space after the
actual caption. Tab labels similarly reserved 90 px regardless of shaped width.
Both groups now use the renderer's measured advance in Luau. The title line box
is centered using the configured bar height; tab slots keep fractional widths.

The bar remembers the last focused window ID through a compositor focus clear
while that window remains on the active workspace of its output. Real focus wins;
closing the window, changing workspace, disconnecting or removing its output clears it.
An absent workspace cannot retain an unscoped window. This follows QML's title helper.

Tab clicks previously selected a new panel spec, destroying its worker and Wayland
surface and replaying the reveal. Navigation now stages the selected page's own
schema and source leases in the resident runtime. Failed validation restores the
accepted page and subscriptions; commit releases the previous page's leases.
The strip keeps stable layer IDs. One shared pearl follows its target, with its
clock starting after acceptance; geometry, duration and easing remain Luau data.
B0106's lazy page bodies, Dashboard label and raster scale are preserved.

QML reference: Nacre `ab04e938`, `NacreHeader`, `NacreActiveTitle`,
`NacreDashboardNavigation` and `NacreDashboardPanel`. The current QML pearl has
no position animation. The requested Bead pearl motion uses the page transition's
180 ms OutCubic curve; this is a requested addition rather than a QML pearl timing.

General primitives: `ui.text_width(text, family, size)` exposes regular-weight
shaped advance through the host font picker and bounded text meter;
`Session::navigate` stages another entry/schema; `PanelNavigated` changes catalog
identity after commit without replacing the surface. No temporary product duplicate
or new dependency is introduced. ADR 0004 records the framework boundaries.

Native QML-left/Bead-right pairs and 25 ms frame series are in [evidence](evidence/0108/).
Both arms used private nested niri, CPU Bead, 2880×1800 at scale 1.5, identical
wallpaper/palette/settings and the same focused real-window fixture. Full native
frames remain private; published crops exclude page-body machine details, with
full-frame hashes/dimensions in `capture-metadata.json`. Sequence timestamps are
screencopy launch/completion proxies, not physical presentation or scanout timing.
The visual candidate `da5473b` retained the real title in all 69 observed primary
states. Its visible title-group center differs from QML by 0.5 physical px in each
axis. All four tab series contain intermediate pearl positions without a strip
reveal replay. Those full series predate the two heap fixes. After lead acceptance,
a quick title/tab refresh on final code `4a4ed0c` passed idle, hover-open title and
Media selection at the same native geometry, with 35/35 title states retained and
containment checks passing. [Refreshed pairs and series](evidence/0108/accepted-refresh/) preserve the
final-binary proof. A private GVFS cleanup failure on the first attempt was corrected
by disabling GVFS mounting only in the private arm; the rerun exited 0.

Five alternating matched release pairs compare against B0106 `575afc7` using
`--release --no-default-features --locked -j2`. The gated sum is anon + shmem +
DRM + SwapPss; file-backed PSS and total PSS remain informational in `memory.json`.
Final code `4a4ed0c` fails **Media +0.513672 MiB**, exceeding the +0.500 MiB limit
by 14 KiB. Other median deltas (MiB): idle +0.089844, Today −0.177734,
Weather −0.179688, System −0.814453, closed −0.199219. Media exceeds +0.5 in
four of five pairs. **Lead-accepted 2026-10-25:** the lead judged the +0.5137 MiB
Media median (14 KiB over the limit) within measurement noise and authorized
integration. The measured failure remains recorded; this unit-specific acceptance
does not change the standing memory limit.

The first candidate failed the median gate on open pages (+2.203 to +6.104 MiB).
Retiring unused rasters plus baker scratch reduced but did not eliminate the
failure (+1.424 to +4.807 MiB). Byte-accounted live caches and allocator live bytes
were comparable to control; free anonymous arena pages remained resident after
old presenter frames were released. A heap-only trim now runs 500 ms after the
first committed frame of each resource generation. The existing five-second
focused SHM pool compaction delay is preserved; this trim never repaints the pool.
Both failed five-pair results and allocator diagnostics are retained in the evidence.
No host swap-in, process SwapPss, OOM or containment failure is accepted.

Rebase onto `origin/main` (`575afc7`) was a no-op. The exact-head GitHub run on
`ad73814` then exposed seven Ubuntu golden mismatches: intrinsic widths had used
host fonts before the headless harness supplied its bundled painting fonts.
`72b8498` makes the measurement policy session-owned and installs the harness's
bundled fonts before its first layout. Native sessions retain the existing host
selection policy. A regression checks two-session isolation, reloads and explicit
policy preservation. Full remote Arch CI passes on `72b8498`; Ubuntu 24.04 also
passes the new regression, all 65 dashboard tests and the bar golden test without
regenerating goldens. Existing native/memory captures remain pinned to `4a4ed0c`.

Full remote Arch CI-equivalent `bash docs/specs/evidence/0108/check.sh` passes
on both `4a4ed0c` and correction `72b8498` (`CI_ALL_OK`), including fmt,
clippy −D warnings, workspace/trace
and feature checks, Luau schema/analyzer, spike and packaging suites, wallpaper,
service/DBus proofs and all dependency graphs. Forty changed goldens at scales
1 and 1.5 were regenerated remotely and visually inspected. Tests cover focus
retention boundaries, atomic navigation/rollback, page leases and stable strip
IDs, plus retaining current rasters at both scales through cache retirement.

Known pre-existing strip styling gaps: QML's selected hover fill, pearl highlight/
gradient, and Media/System glyph choices differ. The captured title hover cue
remains visible longer than QML; its inherited dwell timing is not retuned.
Dashboard body and cold/warm motion
gaps remain tracked by B0106. No live desktop, installed software or laptop-cap
acceptance is claimed.
