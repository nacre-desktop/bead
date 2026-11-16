-- SPDX-License-Identifier: MIT
-- Demo state for the dashboard; every value here is made up for the showcase.
return {
    stores = { wifi = true, bluetooth = true, night = false, dnd = false, volume = 62, mic = 40 },
    actions = {
        toggle_wifi = function(s) s.wifi = not s.wifi end,
        toggle_bluetooth = function(s) s.bluetooth = not s.bluetooth end,
        toggle_night = function(s) s.night = not s.night end,
        toggle_dnd = function(s) s.dnd = not s.dnd end,
        set_volume = function(s, v) s.volume = v end,
        set_mic = function(s, v) s.mic = v end,
        noop = function() end,
    },
    computed = {
        -- Greeting from the same clock the bar shows ("HH:MM"), so the two can never disagree.
        greeting = function(s)
            local hour = tonumber(string.match(tostring(s["desktop.clock"]), "^%s*(%d+)"))
            if hour == nil then return "Hello" end
            if hour < 12 then return "Good morning" elseif hour < 18 then return "Good afternoon" end
            return "Good evening"
        end,
    },
}
