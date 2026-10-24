-- SPDX-License-Identifier: GPL-3.0-only
-- Shell settings: replace the asset set or opt into a bundled local city photo.
-- Photo paths are bundle-relative; dimensions describe the original asset.
return {
    width=564,height=298,radius=20,padding=20,top=16,gap=6,
    location_size=14.667,temperature_size=61.333,condition_size=18.667,detail_size=12,
    ink="onSurface",accent="primary",muted="onSurfaceVariant",border_alpha=.4,
    sky="dashboard/weather/assets/weather/",overlay="dashboard/weather/assets/weather/overlay.themed.svg",
    photo_overlay="dashboard/weather/assets/weather/photo-overlay.themed.svg",
    photos={enabled=false,city={},fallback=nil},
}
