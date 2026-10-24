-- SPDX-License-Identifier: GPL-3.0-only
-- Geometry of the selector prompt, ported from QML NacreAppBrowser (picker mode) and NacreMenuForm.
-- Font sizes are px (QML point size * 4/3). The panel is the launcher's size and look.
return {
  panel = { edge = "bottom", width = 1100, height = 590, radius = 12, border = 1, duration = .2, transition = "ease" },
  title = { left = 20, top = 16, height = 28, font = 20 },
  list = {
    left = 20, top = 54, width = 1060, height = 436, row_height = 55, row_gap = 3, row_pitch = 58, row_radius = 10,
    icon_left = 12, icon_size = 30, icon_font = 20, text_left = 53, name_top = 8, name_height = 20, name_font = 14.667,
    detail_top = 31, detail_height = 16, detail_font = 12, accent_top = 9, accent_width = 3, accent_inset = 18,
    selected_tint = .085, hover_tint = .04,
  },
  empty = { left = 34, top = 80, width = 1032, height = 20, font = 14.667 },
  hint = { left = 20, bottom = 88, width = 1060, height = 16, font = 12 },
  search = { left = 16, bottom = 67, inset = 16, height = 49, padding = 15, radius = 10, font = 16 },
  form = {
    width = 520, top = 89, title_height = 32, title_font = 24, gap = 16, label_height = 20, label_font = 14.667,
    label_gap = 7, input_height = 44, input_radius = 8, input_padding = 12, input_font = 16,
    footer_bottom = 54, button_height = 38, button_radius = 9, button_gap = 10, button_pad = 12, button_font = 14.667,
    char_width = 7.6, error_height = 20,
  },
}
