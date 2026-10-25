-- SPDX-License-Identifier: GPL-3.0-only
-- QML NacreHeader / NacreWorkspaceRow geometry, in logical pixels.
return {
    brand_size=32,
    line_height=1.4, text_lift=2/3, icon_font=20, icon_width=24, detail_font=40/3,
    clock_font=16, clock_min_width=44, status_height=28, item_gap=14/3,
    -- Optical ink widths at icon_font, rather than equal advance-width cells.
    status_gap=6, status_padding=3, status_count_padding=2,
    status_ink={package=44/3,audio=44/3,network=16,bluetooth=10,battery=8,notifications=40/3,calendar=44/3,power=52/3},
    status_symbol_ink={wifi_off=56/3,battery_unknown=16},
    status_text_offset=0, clock_text_offset=2/3,
    workspace_height=30, workspace_radius=15, active_height=28, active_radius=14, app_font=14,
    pearl_current=22, pearl_occupied=20, pearl_empty=18,
    pearl_current_border=1, pearl_empty_border=1.5,
    pearl_cream={255,249,241,255}, pearl_dark={48,48,54,255}, pearl_light={245,242,238,255},
    title_height=30, title_font=16, title_max_width=550, title_cue_max=560, title_gap=7, title_trace_height=2, title_trace_inset=5,
}
