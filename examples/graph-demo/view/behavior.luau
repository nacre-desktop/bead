-- SPDX-License-Identifier: MIT
return {
    stores = { hovered = "", event = "hover a node, click selects, drag pans, wheel zooms" },
    actions = {
        hover = function(s, id) s.hovered = id end,
        select = function(s, id) s.event = "selected " .. id end,
    },
    computed = {
        status = function(s)
            local hover = s.hovered ~= "" and ("hover " .. s.hovered .. "  |  ") or ""
            return hover .. s.event
        end,
    },
}
