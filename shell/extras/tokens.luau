-- SPDX-License-Identifier: GPL-3.0-only
-- Geometry and typography ported from SearchSurface, Overview, Palette and
-- Keybindings. Settings-ready layout values are grouped by view.
return {
  panel = {
    edge = "bottom", reveal = true, transition = "ease", fade = false,
    overview_width = 1020, search_width = 860, max_width_fraction = .94,
    default_screen_height = 900, overview_body_max = 690, overview_body_reserve = 230,
    body_min = 0, search_body_height = 450, body_internal_loss = 26, search_chrome = 112,
    horizontal_inset = 16, heading_top = 16, bottom_inset = 16, search_gap = 16,
    heading_height = 30, heading_gap = 12, heading_font = 22.667,
    search_height = 44, search_padding = 15, search_vertical=8, search_radius = 22, search_font = 16,
    radius = 12, border = 1, duration = .3,
    refresh_size = 30, close_width = 97, close_height = 34, close_font=16, close_radius=9999, close_icon=20, close_gap=8, close_label_width=40, close_content_height=24,
    empty_top = 122, empty_height = 24, empty_font = 16,
  },
  overview = {
    min_columns = 1, max_columns = 3, workspace_width = 320, workspace_height = 215,
    workspace_radius = 18, workspace_gap = 12, workspace_title_left = 12, workspace_title_top = 12,
    workspace_title_width = 296, workspace_title_height = 24, workspace_title_radius = 0,
    workspace_title_font = 16, workspace_icon_font = 18.667, workspace_icon_width = 20,
    title_icon_gap = 8, tile_left = 10, tile_top = 43, tile_area_width = 300, thumbnail_height = 160,
    thumbnail_radius = 8, preview_inset = 4, icon_size = 35, single_tile_width = 295, multi_tile_width = 145,
    two_window_cell_height = 160, multi_window_cell_height = 80, tile_cell_inset = 4,
    tile_padding = 0, title_height = 22, title_gap = 0, title_font = 12, caption_height=16,
    window_columns = 2, window_gap_x = 0, window_gap_y = 0,
    empty_top = 111, empty_height = 24, empty_font = 14.667,
  },
  palette = {
    row_height = 58, row_gap = 6, content_gap = 12, row_radius = 14, padding = 12,
    icon_size = 20, icon_font = 20, icon_height=24, icon_top=17, label_left=44, name_height = 22, description_height = 18, description_top=36, description_gap = 2,
    name_font = 16, description_font = 13.333,
  },
  shortcuts = {
    row_height = 42, row_gap = 5, row_radius = 10,
    key_width = 340, description_offset = 365, padding = 12,
    font = 14.667, keyboard_step = 47, text_top=11, text_height=20,
  },
}
