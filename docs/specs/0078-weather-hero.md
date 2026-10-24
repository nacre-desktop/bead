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

Bead now projects published `wmo` and `is_day` to the shell. `weather-sky.luau`
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
Fallback drift is static pending a reusable continuous animation primitive.
Today wallpaper hero parity is a separate existing gap, not a weather-photo consumer.
Sun-arc and forecast-layout differences are pre-existing and outside this unit.

## Validation

Pending remote CI, regenerated goldens, private parity and relative closed memory.
