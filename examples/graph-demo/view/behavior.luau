-- SPDX-License-Identifier: MIT
local hubs = {}
for i = 1, 40 do hubs[i] = "n" .. i * 50 end
return {
    stores = {
        hovered = "",
        event = "hover a node, click selects, double-click or Enter activates, arrows walk, Esc clears",
        highlight = {},
    },
    actions = {
        hover = function(s, id) s.hovered = id end,
        select = function(s, id)
            s.event = id == "" and "cleared" or "selected " .. id
            if id == "" then s.highlight = {} end
        end,
        -- Activation highlights the node with the forty hubs; everything else dims.
        activate = function(s, id)
            local set = { { id = id } }
            for _, hub in ipairs(hubs) do set[#set + 1] = { id = hub } end
            s.event = "activated " .. id
            s.highlight = set
        end,
    },
    computed = {
        status = function(s)
            local hover = s.hovered ~= "" and ("hover " .. s.hovered .. "  |  ") or ""
            return hover .. s.event
        end,
    },
}
