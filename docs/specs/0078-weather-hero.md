# B0078 — Weather photo / sky

QML's Weather page alone uses `NacreWeatherSky`; Today uses the active wallpaper,
not the city photo. No other QML consumer exists. Photo selection is by city and
coordinates, independent of WMO and daylight. There is no bundled photo set.
`weatherPhotos` defaults on, is user-switchable, and only a visible Weather page
leases the service. Sleeping/locked stops fetching. Wikipedia pageimages must be
within 50 km; Commons metadata must identify JPEG/PNG, a familiar free licence
and attribution. HTTPS-only curl, no redirects, 10-second deadline, 256 KiB JSON,
2 MiB download, 1400×1400/1.96M-pixel limits, magic validation, atomic install.
City/coordinates identify a seven-day cache. No Python helper owns the fetch.

QML decodes at 960×600, synchronously, caches across visits, uses centred
PreserveAspectCrop, and applies **no blur**. Two gradients shade the photo:
vertical body .6/.7 to raised .94; horizontal body .65/.35/.1. Without a photo,
the vertical overlay is .16/.28/.94 and a body/secondary/accent sky is drawn
behind it. WMO selects clear/clouds/mist/rain/snow/storm (unknown has clouds);
daylight changes tint strength and sun/moon. Non-clear fallback drifts 14 px
in 90 seconds each direction, InOutSine, only active with motion enabled and
unlocked/awake. Weather text stays separate from the clipped background; credit
and a Commons source action accompany photos. Today has wallpaper/video,
dark shading, white clock/date/weather text, and its own wallpaper controls.

Bead now projects published `wmo`, `is_day`, and the full trimmed city to the shell.
Local photos also respect `settings.store.dashboard-extras.data.weatherPhotos`. `weather-sky.luau`
selects palette-templated local SVGs; `weather-style.luau` holds layout, typography,
overlay asset paths and a replaceable city-photo map. Photos require explicit
local enablement, bundle-relative path, original dimensions, author and licence;
centre crop uses existing Scroll clipping and Image primitives, without blur.
All modules/assets belong to the lazy Weather page; no service or permanent cache
was added. Framework primitive changes: none. Temporary duplicate paths: none.

## Contract ask / known gaps

Publish a leased weather-photo Source with enabled/ready, bounded local asset,
city/coordinate identity, checked/version, author, licence, Commons source and
status, preserving the above settings, network limits and cancellation rules.
Until published, Bead does not read QML's private cache or fetch anything. The
local photo map is the usable path now. Source-link activation awaits that contract.
Fallback drift is static at x=0 (QML starts non-clear scenes at −14 px), pending
a reusable continuous animation primitive. The radial glow interpolation and
normal temperature weight also differ from QML’s Light weight.
Today wallpaper hero parity is a separate existing gap, not a weather-photo consumer.
Sun-arc, forecast layout, degree-unit suffix/half-degree rounding, wind direction and full detail-line
differences are pre-existing and outside this unit. Local credit currently omits
QML’s “Photo:” / “cropped/softened” prose; author and licence remain visible.

## Validation

**Blocked from integration by the per-phase +0.5 MiB memory gate.** Code and
functional checks are complete; this candidate is not published.

Full CI-equivalent ran in an Arch container with Rust 1.99, DejaVu/IBM Plex fonts,
swaybg/wlr-randr and Sway's file capability removed. See [commands](evidence/0078/check.sh)
and [results](evidence/0078/check-results.txt). Workspace tests, trace memory policy,
fmt, denied-warning clippy, all renderer configurations, Luau analysis (131 modules),
private lifecycle proofs and all license graphs passed. Weather goldens at 1×/1.5×
were regenerated and inspected; [local-photo crop](evidence/0078/local-photo-crop.png)
proves centred cropping, credit and weak-reference release on view destruction.

`spikes/parity-compare.py` ran under the shared lock in private nested niri,
2880×1800/1.5, CPU Bead renderer. Both arms use the Harbour/Cloudy fixture and
matching optional tabs, palette, wallpaper and `weatherPhotos=false`. The private
adapter intercepts only the native helper's `weather` verb in current QML and
passes Bead its published-shaped fixture. [Adapter patch](evidence/0078/parity-fixture.patch).
Native PNGs and the gallery are preserved in the private b0078 parity gallery;
repository JPEGs are full-resolution derivatives, with original PNG hashes.

| Full-window RMSE (0–100%) | Value |
|---|---:|
| Idle | 3.872 |
| Weather | 6.913 |

The comparison includes the documented foreground/layout gaps, preview label,
bar service values and clock differences. It is not a claim of complete Weather
page parity. [QML](evidence/0078/qml-weather-hero.jpg) ·
[Bead](evidence/0078/bead-weather-hero.jpg) · [metrics](evidence/0078/parity.json).

Three alternating control/candidate pairs used identical release flags/backend,
an 8-second boot wait and 6-second closed wait (retirement is 2.2 seconds, then a
3-second code-page reclaim tick). Source: control `996031b`, candidate `ec3a572`,
QML/helper `16436a343e3f20f9bf990a1abf6571ecb82fd44f`.

| PSS+DRM+SwapPss, MiB | Control median | Candidate median | Difference |
|---|---:|---:|---:|
| Initial idle | 33.874 | 33.565 | -0.309 |
| Open Weather | 69.179 | 72.260 | +3.081 |
| Closed | 32.591 | 32.812 | +0.221 |

[Raw memory samples](evidence/0078/memory.json) also retain paired medians:
open +2.452 MiB, closed +0.597 MiB. Both fail the strict per-phase gate.
Open paired anonymous growth is +2.609 MiB; file-backed +0.052 MiB, SHM unchanged.
Two 846×447 RGBA hero image rasters alone require 2.885 MiB of pixel storage,
which explains the added working set. DRM and process SwapPss are measured zero.
Closed minus each candidate's initial idle is -1.033/-1.171/+7.623 MiB. The last
sample's increase is file-backed +7.096 and SHM +0.699 MiB, with anonymous -0.172;
it is not proof that every close settles within +0.5 MiB. The median-total closed
increase is small, but file/buffer refault activity needs a time-series proof.

Owner decision: allow the measured image working set, or authorize a separate
image-memory/rendering unit before integration. Existing full-resolution Image
rasters cannot be assumed to fit a +0.5 MiB phase allowance. No new daemon
contract, animation primitive, desktop install or preview refresh was attempted.
Product delta: +105/-5 lines; no framework or dependency changes.
