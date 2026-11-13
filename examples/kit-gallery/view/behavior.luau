-- SPDX-License-Identifier: MIT
return {
    stores = { mode = "dark", volume = 60, muted = false, wifi = true, filter = "all" },
    actions = {
        set_mode = function(s, id) s.mode = id end,
        set_volume = function(s, v) s.volume = v end,
        toggle_muted = function(s) s.muted = not s.muted end,
        toggle_wifi = function(s) s.wifi = not s.wifi end,
        set_filter = function(s, id) s.filter = id end,
        noop = function() end,
    },
    computed = {},
}
