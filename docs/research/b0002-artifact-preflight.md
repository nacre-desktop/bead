# B0002/B0003 measurement readiness — 2026-10-19

Status: light source preparation only. The previous heavy window ended; there
is no active measurement grant. The published Bead queue says the missing
`wayland-info` blocker is superseded for light work. No measurement, build,
preview, capture, package operation, or test suite was run for this update.

## Frozen artifacts and current evidence

| Role | Source/profile | Recorded binary SHA-256 | Evidence status |
| --- | --- | --- | --- |
| B2 Bead UI | `1cff9d17d61fb05451546bb0ea13c826c93cb4c9`, release | `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b` | Frozen published identity; binary not rehashed in this prep |
| Selected real-daemon helper lane | Nacre `5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`, release | `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa` | Published identity; binary not rehashed in this prep |
| Older helper reference | Nacre `75157fd32fdea26ef8defcfb0430e39be439fd89` | Archived helper `c803fe026a935049c1f19eda85c828d7ae3d081b24b9176a94ea6a790e39cd18` | Historical receipt only; not selected |

The first bounded runner attempt used the inherited remote `PATH` and stopped at
`command -v wayland-info`; its remaining checks did not run. A single authorized
retry with an explicit `PATH` found no `/usr/bin/wayland-info`, and
`pacman -Q wayland-utils` reported that the package was not found on that
trusted runner host. This does not revive the superseded blocker for this light
task and does not authorize an install. The retry printed the two recorded
binary hashes above; it did not rehash files. Their exact preserved file paths
were not specified, and no artifact search was made. The owner's installed-tool
report and the runner-host package result therefore remain distinct evidence.

The reviewed capture guard is `4d0861daa947ff55c137f81fa1618b5f98c19f4c`;
the accepted B0003-S trace parser/test commit is
`e237460bb3c01add2e62bd51b15b240cc53aa786`. Current docs head is
`9449f620cfde2e0d9ff5f883d8cc006425b224ae`. At this head the harness entry
files are `spikes/measure.py` SHA-256
`cbc1eb3150ea83ef41fd5f96e7d43e274594896e36b471d709cebc223d207b6a`,
`spikes/launcher-proof.py` `ab16448fbebe41675a41ea2e61f54917971bd3b01d9785feb48574a6505ad594`,
`spikes/dashboard-residency.py` `767d390e71726b9c815a57e2003efe51dadfb816b3f395fc1f6713d332e131fa`,
and `spikes/capture_safety.py`
`ff9a8743a99d509225519feb7968a6607bd6d8178ab004d83b41825a86521f0f`.
The frozen UI bundle's dashboard fixture tree is
`51dbf56ff49bf55656d7f04257f532dde73ece47` at both source `1cff9d17` and the
current source head. Keep that fixture tree and frozen UI binary paired; do not
substitute a newer shell build.

Old cached UI hashes `9cf08708…` and `5a7e7ddd…` remain unproven and are not
selected. The old receipt's harness was dirty, its archived helper was not
cryptographically tied to its source, and one Hyprland candidate exited 130.
Its partial timings are historical only. B0001/B0004 changed tests/docs, not
runtime inputs; that does not certify a cached executable or a matched B2 run.

## Geometry and command audit

The intended profiles remain five alternating A/B pairs per compositor at
2880×1800, scale 1.5: Hyprland virtual 120 Hz and nested niri 60 Hz. At the
2026-10-19 prep head, `spikes/measure.py` configured Hyprland's virtual output
and printed `hyprctl -j monitors` fields without asserting width, height, scale,
or refresh. For niri's dashboard matrix it set scale 1.5 and asserted physical
2880×1800 plus logical width 1920, but omitted explicit scale, logical height,
and 60 Hz assertions. The accepted B2 source at `eca9cc5` now checks the
required output metadata from actual compositor IPC before arming and preserves
it for the run record. `wayland-info` collects protocol globals; it is not the
geometry proof. Host preflight must still obtain and retain the actual output
IPC evidence for each lane; a source assertion does not establish current host
geometry.

The previous command template also combined unrelated paths:
`BEAD_HARDENING=menu-engine` dispatches to `spikes/menu-engine-proof.py`, the
typing experiment, so B2 dashboard acceptance variables have no effect there.
For B2 the existing dispatcher is `BEAD_HARDENING=launcher`, which reaches
`launcher-proof.py`; use the fixture-backed dashboard path and omit
`BEAD_LAUNCHER_UI_PROVIDER=1` unless a real engine socket is explicitly supplied.
For B3 use `menu-engine` separately, only after B2 PASS and its own grant.

Unrun B2 cell template (replace artifact and private raw-directory variables
only after review; `WARMUP` must be `0` or `600`):

```sh
extra=()
case "$COMP" in
  hyprland) extra=(--virtual120) ;;
  niri) ;;
  *) exit 2 ;;
esac
nice -n 10 env \
  BEAD_BAR_MATRIX=1 BEAD_RAW_DIR="$CELL_RAW" \
  BEAD_HARDENING=launcher BEAD_HARDENING_BINARY="$BUNDLE/nacre-shell" \
  BEAD_CORE_VIEWS_PROOF=1 BEAD_CORE_STATUS_PROOF=1 \
  BEAD_DASHBOARD_VIEWS_PROOF=1 \
  BEAD_DASHBOARD_FIXTURES="$BUNDLE/shell/tests/fixtures/dashboard" \
  BEAD_PANEL_IDLE_STATE=release BEAD_DASHBOARD_RESIDENCY=1 \
  BEAD_DASHBOARD_PAGES=today,weather,media,system \
  BEAD_DASHBOARD_RESIDENCY_SLOTS=4 BEAD_DASHBOARD_RESIDENCY_LEGACY=1 \
  BEAD_DASHBOARD_ACCEPTANCE=1 BEAD_DASHBOARD_WARMUP_SECONDS="$WARMUP" \
  python3 spikes/measure.py --compositor "$COMP" --bar-ui --modes bead --runs 1 "${extra[@]}"
```

For B3's separate real-daemon lane, the existing entry is
`BEAD_HARDENING=menu-engine` with `BEAD_HARDENING_BINARY` set to the frozen Bead
release, `BEAD_MENU_ENGINE_BINARY` set to the selected helper release, and
`BEAD_MENU_ENGINE_BUNDLE` set to the matching helper source bundle. Use
`BEAD_TYPING_KEYS=200`, `BEAD_ENGINE_SETTLE_SECONDS=32`, and run both
`BEAD_TYPING_PROFILE=1` for the initial diagnostic and `clocks`/`1` as matched
clock-only/stage-enabled arms. Do not set `BEAD_ENGINE_LIFECYCLE_ONLY=1` for
typing samples. Invoke the same `measure.py --compositor "$COMP" --bar-ui
--modes bead --runs 1` entry and add `--virtual120` only for Hyprland. These are
templates, not capture authorization; exact artifact paths, geometry assertions,
and safety correction must be resolved before a later grant.

## Safety, schedule, and B3 dependency

The accepted B2 source guard at `eca9cc5c8526730aea125fb6fe59d9364aa6a1a8`
enforces `max(recorded baseline scope peak × 1.5, 12 GiB)`, sets swap limit
zero, verifies manager and effective cgroup limits, and proves the held child's
PID/start-time/cgroup membership both before and immediately before release.
It retains the host start/abort rails at 4 GiB/2 GiB, independent watchdog,
OOM counters, and owned-scope cleanup. Exact-main hosted CI
[39400923434](https://github.com/nacre-desktop/bead/actions/runs/39400923434)
passed and all 42 targeted stdlib tests passed. This is source validation only;
no host artifact verification, host preflight, or B2 capture has occurred.

B2 uses ten cells per compositor, twenty total. The published timing model is
about 145 minutes total without optional warmups (about 72.5 minutes per
compositor); matched 600-second warmups add 200 minutes, about 345 minutes total
(172.5 minutes per compositor), before recovery margin. Every arm requires a
60-second baseline idle observation and full 60-second phase observations;
never reduce pair count or idle windows. No slot is active. The owner reports
Nacre F4 v14 geometry smokes passed, with only two Niri arms valid and the
Hyprland baseline invalid on host swap; those are Nacre observations, not Bead
B2 evidence. Nacre's QML memory fix remains in progress.

B3 is measured only after B2 PASS, with a separate explicit grant and safety
preflight. Its spec caps the profiling window at 120 minutes for one 200-key
diagnostic per compositor plus five alternating clock-only/stage-enabled pairs
per compositor. No validated per-compositor runtime estimate exists yet. The
accepted parser tests are `serial_key_trace_accepts_exact_match`,
`serial_key_trace_ignores_unrelated_presentations`,
`serial_key_trace_rejects_wrong_generation_or_update`,
`serial_key_trace_checks_bounded_busy_chain`,
`serial_key_trace_rejects_missing_or_duplicate_success`,
`serial_key_trace_rejects_nonmonotonic_clocks`, and
`serial_key_trace_checks_profile_mode`. Markers do not always include query
text, so the parser proves RPC/result/commit/presentation linkage but does not
prove query identity or burst behavior. Source preparation is accepted; no
typing measurement or target acceptance follows from it.

No performance result, binary rehash, actual host geometry proof, or B2/B3
readiness claim is made. The B2 source asserts required geometry from actual IPC
output metadata before arming; host geometry still needs preflight. B2 capture
remains blocked pending exact host artifact verification, N9/N10 checks, and a
fresh exact grant. The first full hosted run's unrelated recovery test failure
was not reproduced on the unchanged-SHA rerun; cause unknown. Raw private
artifacts remain outside Git.

## Lead review

Accepted as light preparation only. The first command's lookup failure does not
prove PATH was empty; only the explicit-PATH retry established the reported
missing executable/package on that runner route. The dependency discrepancy
was reported to the owner with minimal install command; no package operation
was attempted. Artifact hashes remain historical bindings, not new file rehashes.
The B2 source geometry and safety guard gaps were addressed at `eca9cc5` and
passed exact-main CI; host artifact verification, host preflight, N9/N10 checks,
and an exact capture grant remain prerequisites. No measurement or product
acceptance follows from this report.
