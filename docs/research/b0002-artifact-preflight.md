# B0002 artifact and capture preflight

Status: Review, preflight request only. No builds, remote probes, previews,
captures, or product changes. No private measurement grant is active.

## Source identity and byte comparison

| Role | Full Bead commit | Meaning |
| --- | --- | --- |
| Baseline | `83f5ecf536c2ec449f9ab0433ecf9ffc69cd8e47` | B0002 baseline; advisory alignment remains subject to the owner’s `0192` ruling |
| B0001 candidate | `989bfbea4e70cd6b875d3b417f3922717d984b55` | Submitted functional dashboard scope |
| B0004 test/docs result | `ef5406f20ff860d1faa011b63108bf81b46c60bf` | Coverage follow-up; implementation CI was green on `56210a6` |
| Current docs head | `08370efd9fd270b7a294ee3696d571d54590aa0a` | Current published source at preflight |

The B1 diff from baseline changes `dashboard_tests.rs`, advisory files and
documentation; B4 changes tests and documentation. No production runtime file,
dashboard fixture, or measurement script changed in B1/B4. The dashboard fixture
tree is identical at baseline, B1 and B4 (`51dbf56ff49bf55656d7f04257f532dde73ece47`);
`spikes/palette-stress.json` is also identical (`88bca0dc5f9bde647c097204dab16f993674edc5`).
Thus B1/B4 production runtime inputs are byte-identical; this is not a runtime
optimization comparison. The B1 test module itself changed, so test executables
are not claimed byte-identical.

`83f5ecf` contains the inherited `RUSTSEC-2026-0192` ignore. The narrow
`3512dd706cfc00e1115e73bccc4847970cda8ade` change scopes three zbus advisories,
but does not resolve or authorize `0192`. No advisory policy was changed here;
this remains an owner-policy blocker for capture readiness.

## Existing binaries and receipts

No existing local binary is source-proven for the intended B0002 baseline/candidate
pair. Two cached `nacre-shell` targets were found but have no verified source
revision, so they are not selected: debug SHA-256
`9cf08708ad00dfca3442e6748fce5fd6337ef4d34a2a939204ae6777ed0f14b0`; release
SHA-256 `5a7e7dded2ffe1bf9c37449adf61ec9dd4b0a7206f2be2cbf7acee961ecc4176`
(ELF build ID `3762ecc4a2bfe268a28548292572aecf2b240d4c`).

An older private receipt named control source `d88e94e14eb611344440af68a870d6990f9200c1`
and candidate source `e944b8fa9542c43aae725dc53e96001e7b867060`; their archived
binary SHA-256 values are respectively
`f2ed18044f9d07c2c0637c96c95bb4e150325a6b176b19cf91e6cae3df75fc30` and
`1a821f8dffc77786575e90ea89179cfaa317c87bb683369b656c7c02e1e4e46f`. They are
not B0002 artifacts: `e944b8f` is not the baseline ancestor and differs from
`83f5ecf` across runtime files. Their directory labels are not proof of which
source produced the binaries.

The archived helper file SHA-256 is
`c803fe026a935049c1f19eda85c828d7ae3d081b24b9176a94ea6a790e39cd18`. Its
receipt labels it `75157fd3`; the read-only local Nacre checkout resolves that
source revision to
`75157fd32fdea26ef8defcfb0430e39be439fd89`. No build manifest cryptographically
links this binary to that source revision, so the pairing is reported but not
certified.

The old receipt records harness commit
`e6e41c17f3686daad01e674387ec4496f2edd77d` with `harness_dirty=true`. Its saved
`launcher-proof.py` SHA-256 is
`7d5c4697b1a187a0e51afd59842047788620345eb588e0e450f691d76f06b6cf`; its saved
`dashboard-residency.py` SHA-256 is
`888be8855d4480d35c6a8952384c5bc0bb9bb4380ad79566a66ac9a3be582a41`. Neither
matches the tracked current source; the latter was not tracked at the receipt’s
named revision. The receipt has ten cell logs, nine exit-0 and one exit-130 on
the third Hyprland candidate. It is incomplete and uses 35/50/65/65-second phase
windows, not the required full 60-second idle windows. These measurements are
historical only and do not count toward B0002.

Current tracked harness identities at `08370ef`:

| File | Git blob | SHA-256 |
| --- | --- | --- |
| `spikes/measure.py` | `c31320fca4cf412ffbb90dc35c80f4b3ff144906` | `59b9bd78287103f5ddfa76c484fc487b289b09b0a3f739de4e98b069da348b4e` |
| `spikes/bar-ui.py` | `e76d1e9901dee14527a2520bf03d964e0903dbff` | `bf69af0ef1345101bf5d94382834e2c805fed6bafc5cbeb95bc8b1e7250b55bc` |
| `spikes/launcher-proof.py` | `81afa2445d3353b3c3d15c5d74bd47ca2ea0980a` | `05fe934fe54643f4811341607f8643bf42fa5201908db939c2f592dad7b38731` |
| `spikes/bar-matrix.py` | `fbb7f9341c5a5b9cb2e2aa6bf1937ccd003448e9` | `be69ecc355a163692703049f79d8b2c78d0a94bbd0de88f190557b719092b273` |
| `spikes/dashboard-residency.py` | `771e7dc0dd865d99831f01c35fa0fc2535f8e1eb` | `010cfec909aed2425b1e1b37923e83ceedbcfae62dc7f1fcc3ca744336f24feb` |
| `spikes/menu-fixtures.py` | `4000962e9389363132ea4a7b5e8ca5df34cb6e36` | `691301658c7799223bdd29e97e2dfe06212c336db7abb90d13e2ad63f162a835` |

These harness sources and dashboard fixtures are unchanged across the B1/B4
test/docs commits. The residency sampler currently targets 35/55/75/65 seconds
per phase and asserts only 30 seconds idle. It has no verified flag for a full
60-second idle window or a 600-second per-arm warmup. Before any granted capture,
the harness needs only the spec-authorized wait/sample-duration adjustment and
an assertion that each phase idles for at least 60 seconds; no such edit was
made during this preflight.

## Proposed nested method and duration

After exact current bundles and a grant exist, run one cell per command and
alternate control/candidate five times per compositor. Both arms use the same
source-proven runtime binary, helper, catalog, palette and fixtures, with
`BEAD_DASHBOARD_VIEWS_PROOF=1`, all four pages, and four residency slots. Use the
tracked `spikes/measure.py` entry point with `--compositor hyprland --bar-ui
--modes bead --runs 1 --virtual120` for isolated Hyprland; for niri use
`--compositor niri --bar-ui --modes bead --runs 1` (omit `--virtual120`). The
outer cell runner sets `BEAD_BAR_MATRIX=1`, a unique private `BEAD_RAW_DIR`,
`BEAD_HARDENING=menu-engine`, the exact `BEAD_HARDENING_BINARY`,
`BEAD_MENU_ENGINE_BINARY`, and `BEAD_MENU_ENGINE_BUNDLE`, plus
`BEAD_TYPING_PROFILE=clocks`, `BEAD_ENGINE_SETTLE_SECONDS=32`,
`BEAD_ENGINE_LIFECYCLE_ONLY=1`, `BEAD_LAUNCHER_UI_PROVIDER=1`,
`BEAD_CORE_VIEWS_PROOF=1`, `BEAD_CORE_STATUS_PROOF=1`,
`BEAD_DASHBOARD_VIEWS_PROOF=1`, `BEAD_DASHBOARD_FIXTURES`,
`BEAD_PANEL_IDLE_STATE=release`, `BEAD_DASHBOARD_RESIDENCY=1`,
`BEAD_DASHBOARD_PAGES=today,weather,media,system`,
`BEAD_DASHBOARD_RESIDENCY_SLOTS=4`, and
`BEAD_DASHBOARD_RESIDENCY_LEGACY=1`. Acceptance cells additionally set
`BEAD_DASHBOARD_ACCEPTANCE=1` and explicitly choose
`BEAD_DASHBOARD_WARMUP_SECONDS=0` or `600`. All commands run under `nice -n 10`.

Proposed one-cell command template (not run; `BUNDLE`, `ENGINE_BIN`,
`ENGINE_BUNDLE`, and `CELL_RAW` must resolve to reviewed exact artifacts):

```sh
umask 077
extra=()
case "$COMP" in
  hyprland) extra=(--virtual120) ;;
  niri) ;;
  *) exit 2 ;;
esac
nice -n 10 env \
  BEAD_BAR_MATRIX=1 BEAD_RAW_DIR="$CELL_RAW" \
  BEAD_HARDENING=menu-engine BEAD_HARDENING_BINARY="$BUNDLE/nacre-shell" \
  BEAD_MENU_ENGINE_BINARY="$ENGINE_BIN" BEAD_MENU_ENGINE_BUNDLE="$ENGINE_BUNDLE" \
  BEAD_TYPING_PROFILE=clocks BEAD_ENGINE_SETTLE_SECONDS=32 \
  BEAD_ENGINE_LIFECYCLE_ONLY=1 BEAD_LAUNCHER_UI_PROVIDER=1 \
  BEAD_CORE_VIEWS_PROOF=1 BEAD_CORE_STATUS_PROOF=1 \
  BEAD_DASHBOARD_VIEWS_PROOF=1 \
  BEAD_DASHBOARD_FIXTURES="$BUNDLE/shell/tests/fixtures/dashboard" \
  BEAD_PANEL_IDLE_STATE=release BEAD_DASHBOARD_RESIDENCY=1 \
  BEAD_DASHBOARD_PAGES=today,weather,media,system \
  BEAD_DASHBOARD_RESIDENCY_SLOTS=4 BEAD_DASHBOARD_RESIDENCY_LEGACY=1 \
  BEAD_DASHBOARD_ACCEPTANCE=1 BEAD_DASHBOARD_WARMUP_SECONDS=0 \
  python3 spikes/measure.py --compositor "$COMP" --bar-ui --modes bead --runs 1 "${extra[@]}"
```

Use `BEAD_DASHBOARD_WARMUP_SECONDS=600` instead for the warmup variant. The
acceptance runner requires the full current page set, four slots and combined
legacy phase; invalid modes or warmup values fail before the launcher starts.

The outer scheduler alternates control/candidate arms and records the exact
bundle/helper hashes for each cell. It must not use different runtime flags for
the two arms when B1/B4 runtime inputs are identical. A 600-second warmup is not
implemented by this current command template; the existing 32-second engine
settle flag is not a substitute. If that warmup mode is needed, its timing hook
must be reviewed before requesting the slot.

The existing runner uses a private D-Bus session and mode-0700 temporary XDG
roots; the launcher proof refuses the live runtime/display. Its cleanup tracks
owned process groups, sends SIGTERM, waits five seconds and escalates only those
owned groups if necessary. It verifies clean UI exit and `STOP surfaces=0`.
The proposed operator cleanup is to let the harness finish normally, inspect its
owned-process audit, and only signal a still-running PID recorded by that cell;
never use a process-name kill.

The prior ten cells took about 43 minutes total (roughly 4m17s per cell). The
acceptance phase budgets add 120 seconds per cell over the old 35/55/75/65s
windows; the measured baseline adds 58 seconds per cell over its old 2s sample.
For 20 cells, that projects to about 145 minutes total before recovery margin.
This supersedes the earlier 94–124-minute planning estimate, which omitted the
baseline window and the final fixed phase-budget arithmetic. A 600-second warmup
adds 200 minutes (ten arms per compositor), for about 345 minutes total (5h45).
That is about 2h53 per compositor, so two separate three-hour private windows
nominally fit but leave little recovery margin. Without warmups the estimate is
about 1h13 per compositor. Keep all five pairs per compositor and request exact
windows from the spec agent; no grant or reservation is active.

## Blockers and disposition

`RUSTSEC-2026-0192` remains an owner-policy dependency. Intended source-linked
baseline/candidate binaries and a certified helper binary/source pairing are
missing. The harness correction implements timing only; it certifies no artifact
and grants no capture. There are no B0002 memory results or acceptance claims.
