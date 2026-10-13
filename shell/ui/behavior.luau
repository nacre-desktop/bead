-- SPDX-License-Identifier: GPL-3.0-only
-- Persistent data belongs to Rust stores; this VM is replaced on every reload.
return {
  stores = {
    ["launcher.open"] = true,
    ["launcher.search.text"] = "",
    ["launcher.scroll.y"] = 0,
  },
  actions = {
    toggle = function(s) s["launcher.open"] = not s["launcher.open"] end,
    scroll = function(s,delta) s["launcher.scroll.y"] = math.clamp(s["launcher.scroll.y"] + delta,0,160) end,
  },
  computed = {
    ["color_surface"] = function(s) return "surface" end,
    ["color_surfaceContainer"] = function(s) return "surfaceContainer" end,
    ["color_onSurface"] = function(s) return "onSurface" end,
    ["color_onSurfaceVariant"] = function(s) return "onSurfaceVariant" end,
    ["color_primary"] = function(s) return "primary" end,
    ["color_outlineVariant"] = function(s) return "outlineVariant" end,
    row_0_visible = function(s) local y = 128 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_1_visible = function(s) local y = 171 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_2_visible = function(s) local y = 214 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_3_visible = function(s) local y = 257 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_4_visible = function(s) local y = 300 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_5_visible = function(s) local y = 343 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_6_visible = function(s) local y = 386 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_7_visible = function(s) local y = 429 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    row_8_visible = function(s) local y = 472 - s["launcher.scroll.y"]; return s["launcher.open"] and y >= 128 and y <= 472 end,
    scroll_offset = function(s) return -s["launcher.scroll.y"] end,
    search_label = function(s)
      local text = s["launcher.search.text"]
      return text == "" and "Search applications…" or text
    end,
    status_label = function(s)
      return "Font fallback: Ελληνικά · العربية · 日本語 · 😀 🌈 🚀    Scroll: " .. tostring(s["launcher.scroll.y"])
    end,
  },
}
