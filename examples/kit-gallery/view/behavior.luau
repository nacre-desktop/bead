-- SPDX-License-Identifier: MIT
return {
    stores = { mode = "dark", volume = 60, muted = false, wifi = true, filter = "all", next_toast = 4,
        toasts = { { id = "t1", title = "First" }, { id = "t2", title = "Second" }, { id = "t3", title = "Third" } } },
    actions = {
        set_mode = function(s, id) s.mode = id end,
        set_volume = function(s, v) s.volume = v end,
        toggle_muted = function(s) s.muted = not s.muted end,
        toggle_wifi = function(s) s.wifi = not s.wifi end,
        set_filter = function(s, id) s.filter = id end,
        noop = function() end,
        add_toast = function(s)
            local next = { { id = "t" .. s.next_toast, title = "Toast " .. s.next_toast } }
            for _, toast in s.toasts do table.insert(next, toast) end
            s.toasts, s.next_toast = next, s.next_toast + 1
        end,
        drop_toast = function(s, id)
            local next = {}
            for _, toast in s.toasts do if toast.id ~= id then table.insert(next, toast) end end
            s.toasts = next
        end,
    },
    computed = {},
}
