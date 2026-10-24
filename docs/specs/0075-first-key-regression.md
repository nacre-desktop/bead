# B0075 — launcher first-key regression

Status: **Review — specified standalone median gates pass; no live switch.**

The regression is real, but [B0044](0044-launcher-flip-paint.md)
measured its historical 6.21 ms result on a different host. Matched desktop lanes
put both sway and niri near 18 ms on the same build.
The adjacent-commit comparison isolates cold opening motion (`ed2557e`).
Home routing and idle code-page reclamation are not exercised by the standalone
typing lane; removing either would conceal the cause without fixing it.
This follows [B0040](0040-typing-cpu-bisect.md)’s lane warning and
[B0069](0069-code-pages-verify.md)’s recommendation to keep code-page release.

## Method and bisect

CPU tiny-skia, private nested desktop sessions, 2880×1800 at scale 1.5, 320 exact
fixture apps, serial `a`/BackSpace with 40 ms pause, clocks-only input tracing.
Release builds use Rust 1.99.1 with identical flags; full CI uses pinned 1.99.0.
Each bisect row has three alternating pairs, 60 keys per arm. The same-build
lane run uses `dce93d2`, three pairs and the same fonts. The stock typing-proof
setup waits about two seconds after its first frame before typing. Callback
timing is synthetic, not physical scanout timing.

| Comparison | Before first-key median, ms | After, ms |
| --- | ---: | ---: |
| Same build: sway / niri | 17.924 | 18.329 |
| `035cd18` / `6644fbe` | 8.632 | 8.850 |
| `035cd18` / `86c8d9a` | 8.672 | 17.866 |
| `d7ba4e4` / `ed2557e` | 9.491 | 17.715 |

No pool compaction precedes the first key in these matched lanes. Archive
source mtimes initially allowed stale Cargo artifacts; those trials were
excluded, every workspace input was refreshed, and accepted binaries were hashed.

## Cause and change

An animation callback arriving after its duration could stop before drawing the
terminal pose. The first input then completed that pose and damaged almost the
whole panel. In the diagnostic trace, retaining redraw through the final pose
reduced semantic damage from 1,698,240 to 606,068 pixels, but alone leaves full animation debt in free SHM
buffers. The newest settled buffer can still be compositor-held.

The presenter now chooses the available painted slot with least repair and can
copy fresher pixels, together with their repair history, from a read-only held
source. It copies only for an unpainted target or when saved repair exceeds a
quarter frame. Existing safe read/copy handling is reused; held slots are never
written. Only motion-capable surfaces use the freshness search, behind a call
boundary; ordinary surfaces retain their first-free/unpainted-adoption path.
Tests compare complete RGBA/BGRA output at scales 1 and 1.5 and verify
that adoption leaves its source unchanged.

Cold raster preparation reuses one scene context, shares identical immutable
pixels within one prepare, and skips unusable deltas from moved paint slots.
Each node retains independent delta metadata and its conservative byte charge.
Rasters outside a repair are culled before mask setup, and masks cover only
their ink bounds. No persistent cache or view state is added. These are general
renderer changes; shell behaviour remains in Luau.

## Final acceptance

Five alternating pairs compare candidate `6055636` against `62e759f`, 60 keys
per arm. All five candidate first keys are below 8 ms (6.899–7.446 ms).

| Standalone CPU lane | Main | Candidate | Gate |
| --- | ---: | ---: | --- |
| First key median, ms | 17.953 | 7.259 | ≤8, pass |
| Later `a` median, ms | 6.631 | 6.379 | ≤8, pass |
| BackSpace median, ms | 7.878 | 7.505 | ≤8, pass |
| Per-arm CPU p50 median, ms | 7.195 | 6.859 | — |
| Per-arm CPU p99 median, ms | 17.953 | 8.005 | — |
| Presented p99 median, ms | 15.401 | 8.492 | Nested callback only |

Five paired bar arms each run two open/type/close cycles, with 12 seconds closed
settle and five one-second memory samples per phase. All gate comparisons use
paired medians; all arms remain in the result. The fixture has an empty Home
reply, followed by a 64-item prefix of 320 total search results.

| CPU UI phase | Total paired Δ, MiB | Anonymous Δ | File Δ | SHM Δ |
| --- | ---: | ---: | ---: | ---: |
| Initial idle | +0.054 | +0.086 | −0.014 | 0 |
| Open 1 | −6.656 | −6.348 | −0.077 | −0.217 |
| Open 2 | −6.662 | −6.445 | −0.022 | −0.217 |
| Closed 1 | −0.067 | +0.102 | −0.099 | 0 |
| Closed 2 | −0.278 | −0.180 | −0.099 | 0 |

Every phase passes the ≤+0.5 MiB relative total gate. Component medians are
independent, so they need not sum to the median total. No paired anonymous
increase exceeds 0.586 MiB; no new persistent state is retained. Open savings
come mainly from anonymous raster storage, with lower buffer residency; closed
file/anonymous differences remain within the recorded spread. DRM fds and
SwapPss are zero in every sample, and all ten arms have zero host swap-in.

Opening medians are 43.731/43.901 ms (control/candidate), including IPC client
spawn to first PANEL_PRESENTED. Paired Δ is +0.066 ms, passing +2 ms. Individual
paired deltas span −1.108 to +7.408 ms; the slow sample is retained.

The separate bar Home→search lane measures 11.314/10.017 ms first-key CPU.
It still exceeds 8 ms; the successful 8 ms gate above is B0072's standalone
window lane, not a claim about every launcher path or real-engine latency.
The real Home command is checked before typing and the first key must show the
64-item search prefix of 320 total results. Every arm requires host swap below
64 MiB, zero host swap-in and process SwapPss, private buses and matched geometry.
The evidence records CPU backend PSS, DRM, SwapPss and anonymous/file residency.
This is the relative desktop gate; no laptop absolute-ceiling claim is made.

The callback-only and free-buffer-only prototypes still took about 18 ms.
The first complete structural fix took 8.764 ms in a four-key pilot; the first
five-pair raster candidate reached 8.173 ms and failed the strict 8 ms gate.
The next font/trim candidate also failed at 8.248 ms. The subsequent raster-bounds
cull reached 7.280 ms, but two complete bar-memory pairs exceeded the relative
idle/closed gate, mostly in clean executable residency. Its remaining prototype
loop was stopped; an interrupted third candidate arm is excluded. The final
candidate drops the unhelpful font/trim changes and keeps the effective repair
and raster changes. Its full older-base memory run still failed at +1.104 MiB
idle and +1.375/+1.395 MiB closed, mostly clean file-backed residency, despite
open memory falling by about 5.4 MiB. Opening passed at −0.041 ms.

The final version also keeps the extra buffer search out of non-motion surfaces.
Main advanced during testing with weather and panel-close trace changes, so both
control and candidate were rebuilt and tested again on `58a7775`. That complete
run passed at 7.440 ms first key, +0.102 ms opening and +0.106/+0.042/−0.129 MiB
idle/closed deltas. Extras then landed, so both were rebuilt again on `62e759f`.
Earlier-base numbers are retained separately, not credited to the final binary.
Clean executable residency is sensitive to build layout; these are matched
binary measurements, not a claim that one branch explains all file-page changes.

No visible surface or motion policy changes, live runs, installs, or temporary
replacement implementations are part of this unit. QML parity captures are not
repeated for this pixel-preserving renderer repair; existing rendering goldens
and full-reference repair tests cover the output contract.

## Validation

The complete `.github/workflows/ci.yml` command sequence passed in a private
Arch container with pinned Rust 1.99.0, `nice -n 10`, Cargo `-j2`: fmt, clippy with
warnings denied, workspace tests and trace memory-policy tests, feature graphs,
facade and headless UI checks, pinned Luau analyzer/generated-schema checks,
spike and packaging suites, wallpaper/service/private-tray proofs, cargo-deny
for the workspace and each independent framework graph. Existing fractional
text/clip/alpha and partial-repair reference tests pass alongside the new
buffer-adoption, duplicate-raster and moved-slot tests. No product dependencies
were added; the remote environment includes DejaVu/IBM Plex fonts, swaybg,
wlr-randr, libcap and tmux as required by the capture/CI tooling.

Sanitized numbers and source/binary identities: [metrics](evidence/0075/metrics.json).
Product changes: +142/−33 lines; test changes: +138/−2 lines. No duplicated
implementation or temporary product path needs an expiry.
