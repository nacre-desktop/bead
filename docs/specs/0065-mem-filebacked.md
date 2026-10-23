# B0065 - Always-on shell: file-backed memory back under the laptop target

Status: **Review** (Claude worker, branch `task/b0065-mem-filebacked-20261023`). Follows the 2026-10-23 rehearsal of `56b9499`, which read
+1.9 MiB (never-opened) and +3.2 MiB (opened) over the frozen control `b01dfcca`, mostly file-backed, with 26.8-27.9 MiB absolute on the desktop.
Harness: `spikes/b0065-smaps.py` (`dump`, `diff`, `memory`), desktop nested niri in headless labwc, CPU renderer (tiny-skia), no daemon link,
launcher without a fixture, `nacre-shell --bar --core-views` only, PSS in MiB, 3 alternating pairs, medians of 7 samples after a 20 s settle.

## Attribution (smaps per mapping, candidate `56b9499` minus control, idle)

| Mapping | control PSS | candidate PSS | delta |
| --- | --- | --- | --- |
| `nacre-shell` r-xp (program text) | 13.57 MiB | 15.50 MiB | **+1.93** |
| `nacre-shell` r--p (rodata, unwind tables) | 2.36 MiB | 2.74 MiB | **+0.38** |
| everything else (fonts, libm, fontconfig, freetype, libpng, heap, stack, anon) | | | **+0.1 net** (largest single: heap +44 KiB, `NotoSansMono` +79 KiB, IBM Plex +28 KiB) |

So the whole file-backed growth is the larger binary (text 19.5 -> 23.5 MB). Not the cause: fonts (same files mapped in both, no icon font mapped at idle),
SVG assets, libmpv (not mapped in the shell: the video renderer is a separate process), Luau bundles (read, not mapped). The text mapping is 93 % (control) and
88 % (candidate) resident: the loader, fault-around and large-folio mapping map nearly all of it, while a closed-idle shell touches a small part.

## Change

`bead-wayland/src/code_pages.rs`: `release()` reads `/proc/self/smaps` and `madvise(MADV_DONTNEED)`s every private `r-xp`/`r--p` file mapping with no
`Anonymous`, `Private_Dirty`, `Shared_Dirty` or `Swap` pages (RELRO, `.data` and anything relocated never qualify). The pages stay in the page cache; only this
process's mapping goes, and a later touch faults them back. A calloop timer in `run_runtime` runs it every 3 s while no window, overlay or panel (nor a panel
target) is up, so open surfaces keep their pages and a closed-idle shell sheds whatever a panel just touched. No new wakeups (the loop already ticks each second);
one call costs about 1.7 ms. First release logs `CODE_PAGES_RELEASED mapped_kib=... us=...`. No behaviour change, no golden change. Rejected: an external
`process_madvise` (needs CAP_SYS_NICE), a lighter `MADV_COLD` (does not unmap), splitting apps into a different binary (apps already run as `--app` processes; the
shell binary's size is not the cost, its resident fraction is).

## Result (3 alternating pairs, medians, MiB)

| Phase | control | candidate `56b9499` | HEAD~1 (this tree, no release) delta | **with release**: candidate | delta | anon delta |
| --- | --- | --- | --- | --- | --- | --- |
| never opened | 24.79 | 26.8-27.9 (lead rehearsal, other harness) | +1.50 | **10.67** | **-14.12** | +0.24 |
| opened once | 23.57 | - | +2.20 | **10.18** | **-13.39** | +1.02 |
| several | 23.91 | - | +3.10 | **11.47** | **-12.44** | +1.94 |
| combined | 23.97 | - | +3.11 | **11.20** | **-12.77** | +1.57 |

Relative gate (<= +0.5 over control): passes by 12-14 MiB. Absolute on this desktop (CPU backend, per-sample max 13.5, one 17.8): well under 25 MiB.
Anonymous: the `several`/`combined` anon delta (+1.1 to +1.9) is **not** from this change: the same pair without the release (HEAD~1) reads +1.19/+1.14 with
the same run-to-run spikes (one candidate run reads 11.4 MiB anon in `several`, transient allocations while the launcher opens without a fixture). Open question
for the lead: a fixture-fed launcher would show whether that is the empty-launcher path; the control also shows one 7.15 MiB anon outlier.

Idle-timeline after opening and closing a panel (2.5 s samples, one run): 8.1-12.6 MiB with occasional single samples at 19-21 MiB when something touches code
between two releases; the settled medians are unaffected, but a single sample is not bounded by 24.5, so the capture's per-sample stop rule is still a risk only
if a spike lands on a sample at the laptop.

## Checks

Bead CI job (Ubuntu 24.04 container on dev-2, Rust 1.99.0): `cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace` (34 suites ok incl. the new
`code_pages` test), feature configurations, facade/Luau schema, spikes unit tests, packaging, wallpaper smoke, services, SNI, `cargo deny` (all ok).
