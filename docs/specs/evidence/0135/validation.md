# B0135 validation

Full remote Arch CI passes on clean commit
`b6c61725c6727ca9c2724e1ba4c5796daa5f6c56`, tree
`7ee7958061e60952ba296b617468a99ae3182138`, actual exit `0`.
Command in the owned container: `nice -n 10 bash /work/final-ci.sh`.
The [exact script](full-ci.sh) has SHA-256
`159347a95e216c44e888693273b9abf7d49ce4a62a514dd75242ad2f91e683c3`;
complete private log SHA-256
`80821f7b3ec1d8a8c6090f8f20101b442d875123f44077d29e0132187d7432ad`.
The nacre-shell package suite reports 665 passed, 0 failed, 1 ignored; other
workspace packages also pass. Luau checks cover 170 modules; scrolling proof
passes 22 tests. CI includes fmt, clippy with warnings denied, service boundaries,
workspace and trace-policy tests, no-default/all-features builds, facade/schema
checks, spike/packaging suites, wallpaper/private-DBus smoke and all cargo-deny
graphs. Toolchain: Rust/Cargo 1.99.0, cargo-deny 0.20.2, Luau 0.740, Arch image
`ghcr.io/archlinux/archlinux:latest`. Container-only additions were swaybg and
removal of Sway's file capability for headless Docker smoke; no host installation.

CI alone did not catch an overfull native panel catalog: an earlier all-page
registration passed CI but failed the native startup capture. The final repair
registers only the missing Clipboard route and keeps the 32-view cap. Native
capture guards and timing results are reported separately from CI.

## Matched runtime pins

Both arms start from Bead `e2d9576917b06093dd8d6281ba0b74876ff88798`,
excluding unrelated later launcher, slider, hover and pearl changes on main.
Control adds [diagnostic tracing](control-trace.patch), SHA-256
`f529485abe77f37857520ac8610c32a48825e005fcbb318b1b5b06ad6e7ecf5b`.
Both receive the same [one-line Clipboard registration](control-registration.patch),
SHA-256 `39ecae0d663de5be3180598ebac62fce2d3a2b4e5a6848f3ed9cbf6e03f93491`.
This exposes the old clipped seven-row control projection without changing its
scrolling. Candidate adds the unit implementation, including clip-aware raster
allocation, vector hex outlines and 68 MiB frame and settled GL-pixel limits. CPU old/new frame and GL old/new
texture admission allow 136 MiB each; global admission is 256 MiB (formerly 192).

Both also receive the same [standalone app trace flag](control-app-trace.patch),
SHA-256 `99b7cad9665c20def90330d151cf151f1f7ca655116d9e018c6bf67e92087fe1`.

| Pin | Control | Candidate |
| --- | --- | --- |
| Source tree | `aac59abb1f2b9c2b65c6c0bb858ba51c63d98203` | `203f19d8764b0a41577eea8bed42481d88f26835` |
| Shell tree | `b7d6f89d48b983180a75ef06839c18d3d67c5ab1` | `52a5777ab4f7a86a9ceaefb8d0024ad267170eca` |
| Bead SHA-256 | `e3545eefe0bd939f72ad711ae07394b82d3a0797f6731de9478edf324a6a6a3e` | `952082d790fcf3b8951e5d97c0660c88f24c64e1fba3938a6f8bfffc2ca567de` |
| Bundle SHA-256 | `3fca92c940048732cc56b5315e315571853fabe089fdc2ab215fbe295d21312b` | `4983abe14c3fe0540288aad6bde86bcb6506378a8fc8bdaecf0ec08bd32d41a4` |

The coordinator independently reconstructed both Git trees. Source-to-stage
shell diffs are empty, no extra shell files were packaged, all six binary hashes
and all twelve read-only library checks pass. Release command:
`nice -n 10 cargo build --release -p nacre-shell --features trace --locked --bins -j2`.
Final control and candidate use separate, fresh Cargo targets. Earlier control
artifacts from a target shared with candidate builds are superseded: identical
source produced different binary hashes, so their dependency provenance is
ambiguous. Their explicit diagnostic measurements remain labelled historical;
none supplies a final memory-gate arm. Original e2/9bf control captures predate
that target switching. Integrated CI uses its own target.

Final canonical harness SHA-256
`d3bbed42b4e9ccedf72fe51c295dc5ad1c4cf8ed1728d56f234170b9617a2415`;
tests `782532ad84e6ca6477658f86357ae851edc3816a29ea7aadb07b6a9fffb82c5a`.
Regular memory pointer helper:
`2cfa935d4a3b21bb047f1c99d823a7db4fb9f30770fee84073d6848d7cbf04a8`;
nonblocking timing helper:
`c8b201892a92bd2a116bd30e0fbde5e126be63a45b27486cfeb14411651147b7`.
A separately hashed one-off kinetic scene does not replace the canonical harness.

Nacre/QML helper reference: `37583d61928368bc22a46aa806fd0a2e06cfee55`;
Orient: `3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8`.
The 754 exported QML shell/shell-tools entries match reference tree
`fbea62d73374f4a71239bbfbda96ac975c917718`, canonical Git mode/blob manifest
`69ac2783d98bddf93cdf1cabf1ed7ce821c1f6f97148327698c289d93d126e8f`.
Current arms verify that manifest before acquisition. Historical arms did not
record it then, so their per-arm QML source pin is unproven; a later matching
export check cannot retroactively establish it. The export has no Git metadata.

## Visual and rejected evidence

Three notification scroll goldens and four static/dynamic hexagon goldens at
1x/1.5x were regenerated remotely and eyeballed. Notification top/reverse match;
vector outlines preserve geometry with small antialiasing differences. Native
QML/Bead pairs expose inherited gaps listed in the [spec](../../0135-scrolling.md).
Endpoint screenshots do not establish animation quality.

Rejected captures remain diagnostic: raster-overflow setups, a package containing
unrelated main-shell files, unregistered/overfull Clipboard startup, wrong QML
Clipboard routing and launcher-border hit points. The earlier default GL80 run
failed image/cache budgets and cannot establish smoothness. Historical CI
failures include a 50 ms Luau shortcut-list timeout, test-only clippy lints and a
stale shared Cargo artifact; subsequent unchanged or corrected full runs passed.
No rejected attempt is counted as a passing native or memory arm. Submission
feedback is a nested timing proxy, not physical scanout certification.

Integration rebased only this unit's unpublished commits onto main `d58dfc67`.
The last rebase preserved all 54 implementation/test patches exactly; only the
status/queue documentation context changed. An earlier rebase retained main's
`view.detached=false` default. Full integration CI includes the test initializer
repair for main's two new return-animation fields, the launcher axis-action
regression, and the final global admission ceiling.

The final same-process native-GL 80-image finger/close+12s/wheel-reopen probe
passes source/swap/safety guards with zero UI errors. The largest sampled phase cache total is
165,466,331 bytes against 268,435,456; transient overlap peaks were not sampled. This is
functional recovery, not smoothness acceptance: reversal remains 281.93/559.72 ms.
The measured process unreclaimable totals are 38.47 MiB idle, 481.52 MiB at the
finger phase and 528.64 MiB at wheel reopen, including DRM. These stress numbers
are separate from the final five-pair eight-image CPU memory cohort and do not
establish a laptop-cap pass. Earlier 192 MiB same-process attempts failed before
wheel input and remain rejected.

The final integration additionally prevents main's new idle native-buffer
preparation while shared wheel or fling animation is active. This six-line
guard is not present in the matched capture snapshots; exact integrated CI is
recorded separately.

The first exact a798 full job failed only the existing 50 ms Luau shortcut-list
timeout. Its unchanged focused test passed 1/1, then the unchanged full job
passed; both attempts remain preserved. No timeout or source was changed.

The late AI follow correction is separate from the memory/stress source table:
base tree203 plus the two-file patch SHA-256
`82f6410ac54a55413c85283a9ee52625d309050faf684c128e0ff26a752c6c45`
yields source tree `315c83b80d31285cfdcf0c9f938fdaae565676ad`, with the
same shell tree52a. Its fresh-target Bead binary is
`07384dc83e14e495dc98170f3c6225f0bd99f6c9ae3264b16c4a3566452c28c1`,
archive `619bc97de3334cf7a282ddd19e9d389dbb715a2b03a55da7de289bb8fea1225b`,
six-bin manifest `60138aaadea95bdaccc9c97975166fddd9e29650da5cf2af82ab714966490b1d`.
Desktop manifest/library checks pass; the coordinator independently checks all
six binary entries. Only chat adapter and test files differ from tree203.
These bytes do not relabel any earlier memory or timing result.

The first b6 full workflow passed without the required nice wrapper. Its
priority-compliant repeat aborted after `CHECK_OK` with
`malloc(): unaligned tcache chunk detected`; the exact standalone check then
passed. A complete unchanged b6 rebuild into a fresh integrated target passed
the full workflow at verified nice value10, with a clean source tree and matching
Cargo workspace/target paths. The top log hash records that compliant pass.
The allocator abort remains unexplained; a passing clean build is not a claimed
source fix. Earlier logs and actual exit markers remain preserved.
