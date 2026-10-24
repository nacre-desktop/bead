# B0080 — populated Home first key

Status: **Review; five-pair CPU, opening and relative-memory gates pass.**

The launcher still opens on Home and the first key shows search results.
Pending rows keep their accepted geometry; one nonblocking projection recheck
replaces an already superseded view before raster work. Adoption copies only
pixels outside complete repairs, and fresh slots skip already-zero blocks.
Linux SHM advice covers interior pages initially and shared boundary pages when
a fresh spare is used. The focused compaction timeout and buffer count are unchanged.

The general `ui.memo(key, inputs, render)` primitive reuses pure Sidebar, Quick
and hint markup. It tracks store reads, retains no VM, and releases its fragments
with the generation. Payload/index limits are 128/32 KiB; construction work and
expanded output each have an 8 MiB limit. Normal layout, binding/action validation
and Repeat identity still apply. Literal property maps are borrowed, and a fixed
static property index avoids repeated linear validation.

## Matched captures

Control `cc84574` has only the shared [fixture/readiness/profile overlay](evidence/0080/control-probes.patch).
Candidate source is `dfde948`. Both release builds use Rust 1.99.1, the same source
path, flags and CPU tiny-skia backend. Five pairs alternate arm order, with 60
serial `a`/BackSpace keys and a 40 ms pause after each completed presentation.
The Home fixture has 12 actual favourite flags, 8 recent entries and three visible
Quick controls; names/IDs stay stable across queries. Search returns a 64-row prefix
of 320 apps. Icons use the matched fixture SVG from the existing typing lane.
Standalone retains that lane's 320-app grid/list workload. No ranking or engine is copied.

Private niri runs inside headless labwc/pixman at 2880×1800, scale 1.5, nominal
60 Hz. Clocks-only acceptance waits for the matching completed-result commit;
early feedback is recorded separately. Nested presentation callbacks are not
physical scanout. Idle/open/closed memory waits 12 seconds, then takes five
one-second samples; the first Home key therefore follows focused compaction.
Each arm requires host swap below 64 MiB, zero host swap-in/process SwapPss,
contained private buses/surfaces and the existing watchdog/source/geometry gates.

| Five-arm medians, CPU backend | Control | Candidate | Gate |
| --- | ---: | ---: | --- |
| Home first completed search, UI CPU ms | 15.161 | 7.598 | ≤8 pass |
| Standalone first key, UI CPU ms | 9.437 | 6.508 | ≤8 pass |
| Later Home / standalone keys, UI CPU ms | 13.682 / 7.610 | 6.811 / 7.031 | ≤8 pass |
| Opening, paired latency Δ ms | — | −0.683 | ≤+2 pass |
| Idle / closed, paired total Δ MiB | — | −0.165 / −0.316 | ≤+0.5 pass |

Home first keys span 7.449–7.773 ms; standalone spans 6.503–6.594 ms.
Whole-process CPU over the Home typing burst is 13.167 → 6.667 ms/key, including
post-presentation work (process-stat resolution is 10 ms per whole burst).
Early-feedback CPU is 12.400 → 7.598 ms; candidate feedback already contains
the accepted result in all 300 keys. Matching-result timing is stricter than
B0075’s earlier feedback clock, so those empty-Home numbers are not a direct baseline.
Medians of paired differences can differ from differences of displayed medians.

## First-key stages

Five separate paired 60-key diagnostic arms retain detailed traces. The table
sums each stage through the matching result presentation, then takes arm medians.
Spans nest and cross threads; these columns must not be summed as disjoint CPU.
Text editing is native: no Luau action handler is invoked; computed bindings are separate.

| First-key CPU ms, diagnostic traces | Control | Candidate |
| --- | ---: | ---: |
| Luau action handler | not invoked | not invoked |
| Computed bindings | 0.106 | 0.264 |
| Markup / view switch | 3.622 | 1.767 |
| Typed view construction | 0.880 | 0.465 |
| Layout | 0.548 | 0.301 |
| Text shaping, glyphs | 0.104 | 0.096 |
| Text shaping, editor | 0.064 | 0.044 |
| Glyph raster | 0.377 | 0.369 |
| Visual damage | 0.088 | 0.047 |
| Buffer allocation/adoption/repair setup | 3.632 | 1.027 |
| Composition | 5.803 | 2.444 |
| Wayland commit | 0.032 | 0.011 |
| Whole UI process, trace overhead included | 17.961 | 8.851 |

## Settled UI memory

PSS + DRM + SwapPss, CPU backend; all acceptance table-phase DRM and SwapPss samples are zero.
These are relative desktop gates, not the laptop's absolute 25 MiB acceptance.

| Phase | Control total | Candidate total | Paired Δ | Candidate anon / file / shared, MiB |
| --- | ---: | ---: | ---: | ---: |
| Idle | 10.720 | 10.544 | −0.165 | 5.629 / 4.241 / 0.686 |
| Home open | 35.949 | 35.944 | +0.039 | 15.727 / 16.372 / 3.846 |
| Closed + idle | 10.786 | 10.437 | −0.316 | 6.578 / 3.180 / 0.686 |

Shared/frame-buffer residency is unchanged in every matched settled phase.
The +0.039 MiB open median occurs in two pairs: +0.180 anonymous/−0.141 file,
and −0.078 anonymous/+0.117 file. The largest open anonymous increase is
+0.242 MiB; cache payload/index are bounded and no additional VM is retained.
Loaded-font PSS idle/open/closed is 1.647/2.063/0.604 MiB for control and
1.659/2.064/0.597 MiB for candidate. Column medians need not add to total medians.

## Checks and limits

The full `.github/workflows/ci.yml` command sequence passed remotely on exact
source `dfde948`, in the required private Arch container with pinned Rust 1.99.0,
`nice -n 10` and Cargo `-j2`: fmt, clippy with warnings denied, workspace/trace
memory tests, feature graphs, facade/headless/goldens, Luau analyzer/generated
schema, spike/packaging suites, wallpaper/private services/tray and every
cargo-deny graph. No product dependencies were added; capture/check tools include
DejaVu/IBM Plex fonts, swaybg, wlr-randr, libcap and tmux.

New proofs cover bounded/coalesced projection and atomic failure, pending-row
geometry, fixture preference flags, nested memo reads/inputs/Repeat identity,
early memo-work rejection and full-reference RGBA/BGRA fractional SHM repairs.
[Home control](evidence/0080/control-home.png) / [candidate](evidence/0080/candidate-home.png)
and [search control](evidence/0080/control-search.png) / [candidate](evidence/0080/candidate-search.png)
were eyeballed; all pixels below the changing clock bar match exactly. Existing
goldens pass unchanged. No new matched QML parity/timing capture was made.
Slow-source pending geometry now stays with accepted rows until a reply; its
transient timing has not been compared with QML. Real daemon/provider typing,
other catalogs/icons, live scanout, Hyprland and laptop absolute memory are not claimed.

The pre-cache candidate still missed at about 8.13 ms. A deferred-VM prototype
increased open anonymous residency and was removed. Eager boundary advice touched
unused spare pages; initial advice is now interior-only. Early 2–7 second open
samples preceded focused compaction and are not labelled settled measurements.
Only the final exact-binary pairs above are acceptance evidence; interrupted and
invalid-route/readiness pilots are excluded. No temporary product implementation,
live run, deployment or Nacre change remains.

[Sanitized samples, stages, gates and source/binary identities](evidence/0080/metrics.json).
Product changes: +355/−46 lines; tests: +216/−1 lines.
