# B0134 — Bound wallpaper picker retention

Status: shipped review candidate; retention/static gates pass, video-picker and
expired warm-presentation timing gates fail. No live deployment.

B0123's historical five-pair closed-after-125-second delta was +303.539 MiB
unreclaimable: +40.293 anonymous, +0.094 SHM and +263.152 DRM (component medians).
This unit targets ≤40 MiB retained above the picker's own never-opened idle,
≤16 ms warm first content, ≤150 ms first-ever content, and <150 ms selection to
wallpaper **and** bar/picker palette. These targets are not universal guarantees.
The final timing distribution and its misses must be read alongside the medians.

## Resource lifetime

The picker keeps a finished CPU first frame while closed. After two seconds it
retires GL images, its opacity composite, canvas and window surfaces, and trims
CPU scratch. A transparent 1×1 SHM buffer with a full-size viewport replaces the
compositor's last GL buffer while keeping the layer role mapped. An off-thread
lossless repeated/literal RGBA row snapshot replaces dense opacity storage;
incompressible rows retain a dense fallback. Reopen/replacement/scale epochs
reject stale work. Reopening draws cached CPU content while a worker prepares
and transfers the sole non-current GL context to the UI thread.

EGL termination applies only to an exclusively owned display with a CPU primary
and no other GL surfaces. A GL primary or another active/prepared GL target keeps
its required context. There was already one shared context, not one per card or
output. The wgpu transport explicitly uses Vulkan. This is not a universal
≤40 MiB claim for combinations that must retain other GL surfaces.

General primitives are `Renderer::release_gpu_resources`,
`TreeCpuRenderer::snapshot`, movable `PreparedTransport`, optional
`Panel.shadow` (default true), and `WallpaperProvider::prepare_staged`. Spotlight disables its redundant viewport shadow.
Image slots retain one previous exact raster to avoid rebaking adjacent extents
one pixel apart during carousel motion. Full path/content/scale/geometry/clip
signatures remain authoritative; damage metadata is refreshed against the actual
previous frame. Both variants carry budget charges; pressure evicts spare variants
first, and idle/look retirement releases them. Non-image slots stay single-raster.
Stable Luau image IDs avoid generation-only cache invalidations.

Encoded thumbnails are capped at 16 MiB, 2 MiB/file and 1600×1000, loading the
visible set plus neighbours. Preview is a fallback only when a primary thumbnail
is missing. The one display-sized predecode worker has bounded current/pending
work, a two-second idle expiry and no UI-thread image decoding. Never-opened
added memory has a ≤1 MiB budget; closed picker retention has the explicit 40 MiB
budget. Spare open-state rasters spend bounded memory to remove repeated work.

## Attribution

| Diagnostic strategy, one arm each | Retained above own idle | DRM after close |
| --- | ---: | ---: |
| Delete textures/surfaces, keep EGL context | 226.004 MiB | 169.250 MiB |
| Dense CPU frame, terminate exclusive EGL display | 44.045 MiB | 0 MiB |
| Row-compressed CPU frame, terminate EGL display | 26.217 MiB | 0 MiB |

The original release trace counted 24.716 MiB of GL image payload and 20.296 MiB
of opacity composite. Exclusive EGL retirement releases much more than those
payloads, implicating driver/window allocation pools. DRM fdinfo does not identify
individual BO classes, so this is not an allocation-level reconciliation. Picker
cards use bounded thumbnails, not full-resolution wallpaper textures.

With Spotlight's viewport shadow removed, 16 image payloads total 4,748,952 bytes;
the opacity composite remains 21,281,904 bytes. Its compressed CPU frame occupies
2,651,348 bytes (2.529 MiB). CPU owners also include TreeCache layer/decoded
rasters, font/text scratch, node/frame maps, Luau/markup state, encoded thumbnails
and allocator arenas. In a smaller carousel diagnostic, encoded images were
113 KiB, Luau 0.783 MiB, markup/build state 1.449 MiB and source state 47 KiB.
Shared references overlap these counters; no per-cache anonymous-PSS split is
claimed. Clean file-backed PSS is informational and is reported separately.

## Selection and contracts

Selection predecodes in parallel with the existing daemon activation call and
prewarms cached palettes for visible IDs, even when thumbnails are already cached.
Cached pixels are consumed only for a confirmed active wallpaper/palette pair,
fenced by daemon boot, selection generation, complete file identities, output
size and presentation. The required revision activation confirmation remains.
SHM conversion is staged in an existing free buffer while palette workers prepare;
at most two buffers exist. Revision/token/dimension/index fences prevent stale or
reserved-buffer reuse. Staging now overlaps the unchanged activation confirmation RPC. Exact
output/revision/token identities retain each job until its result; old visible
buffers remain while staging. The group waits for both current bar/panel epochs
and matching confirmations before attach. Same-palette frames also wait. Failed
confirmation rolls the group back before releasing a newer request; discarded
sibling outputs retry only when a newer request was already queued. No extra
RGBA copy is retained. The default provider keeps prepare-and-return behavior;
internal frame events carry Ready/Staged/Confirmation phases.
Accepted groups no longer redundantly notify an already prepared panel palette.
Overlay and legacy entry points keep their notifications.

The additive published `video_predecode` catalogue field advertises the exact
cached PNG poster and source/poster identities only for explicitly requested video
IDs. Partial hydration preserves descriptors for unrequested rows. Older daemons
omit the descriptor and retain confirmed decoding. Video starts with that poster;
playing motion is verified separately and is not claimed to start within 150 ms.
Native selection writes complete dynamic media identity before publishing the
pair and restores the exact previous bytes on failure. While media leads a new
poster, the daemon preserves only an unchanged identity-checked previous motion
source. Malformed or unsafe metadata clears it. Previous-history readiness-only
updates do not repaint; epoch, connectivity and real pair changes still do.
No ranking, provider, engine copy or new activation method was introduced.

The JPEG/static PNG path uses existing decoders plus `fast_image_resize` 6.1.0
(MIT OR Apache-2.0, default std, no Rayon), with hot dependencies optimized at 3.
ICC/unsupported PNG formats, animation and large/unsupported inputs retain the
pinned streamed loader. Before/after descriptor identity checks remain. Profiling
goes to stderr; stdout remains exact raw RGBA. No system build dependency was added.

## Measurement and sources

Five alternating private control/candidate pairs use generated five-photo plus
one-MP4 fixtures, separate HOMEs, 2880×1800 at scale 1.5, CPU bar and femtovg GL
picker. Static retention is sampled before video activation. Every command takes
the shared desktop lock, performs swap preflight and completes the driver's scope,
geometry, watchdog, source and cleanup gates. An invalid pair excludes both arms.
Synthetic 120 Hz scheduling and `refresh_ns=0` feedback do not establish physical
scanout. UI memory is separate from daemon/provider totals.

Unreclaimable is `Pss_Anon + Pss_Shmem + DRM + SwapPss`. Closed retention subtracts
the candidate's own never-opened idle; paired phase deltas subtract the matched
control. The relative regression limit is +0.5 MiB median per phase. Missing
samples are unavailable, not zero. Active video memory is a separate phase.
The pointer clock starts at button release; its 40 ms press hold and approach are
recorded separately. Wallpaper feedback matches output/revision; bar/picker
feedback matches palette-tagged commit/update. Warm first draw, first current-
visibility presentation and later scene refresh are distinct measurements.
Controls lacking cached presentation tracing report that metric unavailable.

Frozen final comparison: Bead control `d58dfc67`, candidate `c63acedf`; Nacre
helpers `c9adf0f4` and `6abe2498`. Both use the same fixture/QML tree.
[Source patches, binary hashes and historical pins](evidence/0134/provenance.json)
keep earlier diagnostics separate from this measured source. Integrated source
`588d9506` rebases this unit onto `cb3de7e3` shared scrolling and raster-budget
changes. Native window/overlay feedback uses neutral visibility epochs. Its full
CI and targeted proof are separate; frozen pairs are not relabelled as integrated
five-pair acceptance. B0141 owns follow-up grid scrolling and raster-cache
bounds; this unit preserves the shared scrolling interfaces and does not replace
its work. The correctly rooted integrated smoke completes all five states:
static 116.911/120.512 ms, video 147.061/150.468 ms (wallpaper+bar/picker);
static retention is 29.563 MiB and expired draw/feedback is 12.536/18.059 ms.
It reproduces the timing limits. A first smoke with an incorrectly nested shell
fixture showed missing-state reload errors and is excluded; no product change
was needed.

The superseded V16 five-pair cohort passes retention, relative memory, cold
presentation, warm drawing and static selection. Its largest static close median
is 28.810 MiB above own idle. Video medians 153.053 ms wallpaper/bar and
156.482 ms picker miss 150 ms; [all samples](evidence/0134/diagnostic16-summary.json)
remain diagnostic. Cached first-presentation feedback on expired opens is about
18 ms despite ≤16 ms draws; these are distinct clocks.

Final V18 has five valid alternating pairs; [all samples and strict gate flags](evidence/0134/final18-summary.json)
include every valid timing miss. All ten arms pass driver, containment, geometry,
swap and memory-watchdog gates; swap is zero and no reset is needed.

| Candidate, CPU bar / GL picker | Median | Range |
| --- | ---: | ---: |
| Largest static closed retention above own idle | 28.868 MiB | 28.185–40.087 MiB |
| Never-opened idle, candidate − control | +0.320 MiB | see all samples |
| First-ever content presentation | 107.430 ms | 106.996–109.042 ms |
| Warm repeat first draw / feedback | 10.227 / 14.195 ms | draw 9.929–10.371 ms |
| Warm expired first draw / feedback | 12.401 / 17.956 ms | feedback 17.655–18.259 ms |
| Static wallpaper and bar palette | 124.341 ms | 119.225–136.152 ms |
| Static picker palette | 127.925 ms | 122.664–139.961 ms |
| Video wallpaper and bar palette | 147.363 ms | 130.784–147.767 ms |
| Video picker palette | **150.743 ms** | 134.151–150.966 ms |

Static closed phase medians above own idle are 24.994 / 26.787 / 28.295 /
28.868 MiB (cold/repeat/expired/selection). All phase medians pass the +0.5 MiB
relative regression limit; idle is the largest at +0.320 MiB. Other sampled
idle/expired/open/closed deltas are −241.921 / −36.485 / −263.632 MiB for the
last three phases. The active-video closed median is 37.441 MiB above idle,
reported separately from static picker retention because playback remains active.
One static-selection close sample is 40.087 MiB (outside 40), including
20.478 MiB SHM versus approximately 10.591 MiB in the other four arms. DRM and
SwapPss are zero; it is not an unexplained anonymous-memory step. Its cause is
not fully attributed and no sample is excluded; acceptance is the specified
five-pair median. Clean file PSS and all components are in the summary.

All static selections and all video wallpaper/bar samples pass 150 ms. Three of
five video-picker samples fail; its median also fails. All warm draws pass 16 ms,
but every expired first-presentation feedback sample fails 16 ms. This is a
measured improvement with remaining acceptance failures, not an all-gates pass.
The earlier single combined probe's 144.734/147.970 ms video result is not used
to erase the misses. Its trace places SHM staging before confirmation and
attachment after it.

| Diagnostic source | Wallpaper/bar | Picker palette | Result |
| --- | ---: | ---: | --- |
| C1 video, incomplete motion pair | unavailable here | 762.954 ms | Failed; identity publication fixed |
| V12 exact cached poster | 155.343 ms | 158.507 ms | Failed; duplicate palette notification fixed |
| V13 later paired video | 157.570 ms | 160.783 ms | Failed despite an earlier single pass |
| V14 static | 163.455 ms | 166.957 ms | Failed; repeated image extent bakes |
| V15 static, stable IDs alone | 165.195 ms | 167.971 ms | Failed; previous exact raster added |
| V16 combined video | 156.740 ms | 159.990 ms | Valid miss; retained in provenance |

The final GL carousel proof has 40 frames: render p50/p99
0.279/1.141 ms, frame interval p99 8.077 ms and callback p99
8.031 ms. Travel render p99 is 0.419 ms over 18 frames; its
interval/callback p99 values include content and idle waits.
[Client clocks](evidence/0134/motion-pacing18-detail.json) are not physical
dropped-frame counts or CPU-motion acceptance.

## Visual review and limits

Paired private QML/Bead captures use identical output, wallpaper, palette and
fixture window. QML wallpaper view blobs are unchanged in the published Nacre
source. Spotlight retains the inherited smaller floating panel over the desktop;
QML fills the screen and hides the fixture window. Bead fades/scales while QML
slides upward. Font metrics/resampling, disabled `+ Add`, outer-frame clipping
and fixture update counts remain known parity gaps. Pixel parity is not claimed.
Idle through selection rows below are the source-pinned C1 diagnostic review;
the last two rows repeat the changed cache paths on final frozen source V18.

| State | QML | Bead |
| --- | --- | --- |
| Idle | [image](evidence/0134/qml-spotlight-desktop.png) | [image](evidence/0134/bead-spotlight-desktop.png) |
| Cold | [image](evidence/0134/qml-spotlight-cold.png) | [image](evidence/0134/bead-spotlight-cold.png) |
| Warm | [image](evidence/0134/qml-spotlight-repeat.png) | [image](evidence/0134/bead-spotlight-repeat.png) |
| Hover | [image](evidence/0134/qml-spotlight-hover.png) | [image](evidence/0134/bead-spotlight-hover.png) |
| Pressed | [image](evidence/0134/qml-spotlight-pressed.png) | [image](evidence/0134/bead-spotlight-pressed.png) |
| Carousel | [image](evidence/0134/qml-wallpaper-carousel.png) | [image](evidence/0134/bead-wallpaper-carousel.png) |
| Selection | [image](evidence/0134/qml-palette-sync.png) | [image](evidence/0134/bead-palette-sync.png) |
| Final-cache post-grace | [sequence](evidence/0134/qml-spotlight-settled-final18.jpg) | [sequence](evidence/0134/bead-spotlight-settled-final18.jpg) |
| Final-cache video click | [sequence](evidence/0134/qml-wallpaper-format-mp4-final18.jpg) | [sequence](evidence/0134/bead-wallpaper-format-mp4-final18.jpg) |

The first V18 observer run missed button delivery and is excluded. The paired
retry uses the existing input helper’s 200 ms press settling; numeric captures
retain 40 ms. No product or pacing flags changed.
The final post-grace sequences retain populated thumbnails through the fade with
no sampled black/empty flash or reversal. Screenshot requests at about 25 ms
steps are intrusive; observer-loaded latency is not mixed into the numeric cohort.
The old [forced EGL failure](evidence/0134/fallback11-analysis.json) completes with
one GL-to-CPU fallback marker and correct cached content; it is functional
fallback coverage, not CPU-motion performance acceptance. Unsampled frames, actual
physical refresh and the absolute laptop memory cap remain unproven. No live UI,
installation, desktop configuration, unit or keybinding changed.

## Checks and size

[CI record](evidence/0134/ci.json): both frozen and integrated Bead full Arch
mirrors pass `cargo fmt --all --check`, workspace/all-target clippy `-D warnings`,
workspace tests, feature/trace/facade/SDK checks, every workflow Python suite,
private-bus services, video smoke and all independent cargo-deny graphs. Rust is
1.99, Luau 0.740 and cargo-deny 0.20.2; remote commands use nice 10 and Cargo -j2.
Nacre's entire desktop job passes, including strict native/Python parity, QML,
translation/font audits, gallery and ignored memory tests. Its official
[desktop/fresh-install run](https://github.com/nacre-desktop/nacre/actions/runs/40264901647)
is green. Bead’s [integrated official check](https://github.com/nacre-desktop/bead/actions/runs/40287010521)
also passes on `588d9506`. Operational retries and excluded diagnostics are preserved in CI JSON.
No local build, test or capture ran.

Product changes add/remove 2,611/364 Bead lines and 410/90 Nacre lines:
**3,021 added, 454 removed, 23 product files; zero removed files**. Counts exclude
tests (including embedded modules), docs, proof tools, lockfile, goldens, fixtures
and generated artifacts; manifests and the Luau protocol declaration are included.
The unused preview watcher and unconditional re-decode path are removed. Dense
storage and streamed decoding are required fallbacks, not temporary duplicates.
All framework crates remain unpublished and permissively licensed.
