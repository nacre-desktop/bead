-- SPDX-License-Identifier: MIT
return {
    stores = { hovered = "", selected = "", event = "hover a node, click selects, double-click activates, drag pans, wheel zooms" },
    actions = {
        hover = function(s, id) s.hovered = id end,
        select = function(s, id) s.selected = id; s.event = "selected " .. id end,
        activate = function(s, id) s.selected = id; s.event = "activated " .. id end,
        pin = function(s, p) s.event = string.format("pinned %s at %.0f, %.0f", p[1], p[2], p[3]) end,
        view = function(s, v) s.event = string.format("zoom %.2f at %.0f, %.0f", v[1], v[2], v[3]) end,
    },
    computed = {
        status = function(s)
            local hover = s.hovered ~= "" and ("hover " .. s.hovered .. "  |  ") or ""
            return hover .. s.event
        end,
    },
}
